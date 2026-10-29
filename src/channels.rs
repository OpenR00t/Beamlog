// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Beamlog contributors
//! Turning a recorded log into fixed-rate MoTeC channels.
//!
//! The recorder writes one row per graphics frame, so its time steps vary.
//! MoTeC needs every channel at a fixed rate, so each column is resampled onto
//! a regular grid: linear interpolation for smooth signals, the last value for
//! states (gear, ABS active), and for accelerations the mean over a short
//! window (ACCEL_WINDOW): the game's acceleration sensor carries the whole
//! chassis's vibration and swings by a couple of m/s^2 from frame to frame.
//!
//! Channel names follow the ones common in sim-racing MoTeC files, so existing
//! worksheets find them. Sign conventions (checked against the data, below):
//! lateral acceleration positive to the left, longitudinal positive when
//! speeding up, yaw rate and steering positive to the right.

use crate::log::Log;

pub const G: f64 = 9.80665;
const DEG: f64 = 180.0 / std::f64::consts::PI;
const PSI_TO_KPA: f64 = 6.894757;
const METRES_PER_DEGREE: f64 = 111_320.0;
/// s, centred mean applied to accelerations and yaw rate
pub const ACCEL_WINDOW: f64 = 0.2;
/// s, smoothing used only to decide signs from the data
const SIGN_WINDOW: f64 = 0.5;

#[derive(Debug, Clone)]
pub struct Channel {
    pub name: String,
    pub short: String,
    pub unit: String,
    /// decimal places stored (values are kept as whole numbers x 10^dec)
    pub dec: i16,
    pub data: Vec<f64>,
}

#[derive(Clone, Copy, PartialEq)]
enum Fill {
    Linear,
    Hold,
    Mean,
}

/// Resample (t, y) onto `grid`; `step` is the window for `Fill::Mean`.
fn resample(t: &[f64], y: &[f64], grid: &[f64], fill: Fill, step: f64) -> Vec<f64> {
    let n = t.len();
    if n == 0 {
        return vec![0.0; grid.len()];
    }
    let mut out = Vec::with_capacity(grid.len());
    let mut j = 0; // t[j] <= g < t[j + 1]
    for &g in grid {
        while j + 1 < n && t[j + 1] <= g {
            j += 1;
        }
        let v = match fill {
            Fill::Hold => y[j],
            Fill::Linear => {
                if g <= t[0] {
                    y[0]
                } else if j + 1 >= n {
                    y[n - 1]
                } else {
                    let (t0, t1) = (t[j], t[j + 1]);
                    let a = if t1 > t0 { (g - t0) / (t1 - t0) } else { 0.0 };
                    y[j] + (y[j + 1] - y[j]) * a
                }
            }
            Fill::Mean => {
                let (lo, hi) = (g - step / 2.0, g + step / 2.0);
                let mut k = j;
                while k > 0 && t[k] >= lo {
                    k -= 1;
                }
                let (mut sum, mut cnt) = (0.0, 0usize);
                while k < n && t[k] < hi {
                    if t[k] >= lo {
                        sum += y[k];
                        cnt += 1;
                    }
                    k += 1;
                }
                if cnt > 0 {
                    sum / cnt as f64
                } else {
                    y[j]
                }
            }
        };
        out.push(v);
    }
    out
}

/// Centred moving average over `n` samples.
fn smooth(a: &[f64], n: usize) -> Vec<f64> {
    let half = n / 2;
    let mut prefix = vec![0.0; a.len() + 1];
    for (i, x) in a.iter().enumerate() {
        prefix[i + 1] = prefix[i] + x;
    }
    (0..a.len())
        .map(|i| {
            let (lo, hi) = (i.saturating_sub(half), (i + half + 1).min(a.len()));
            (prefix[hi] - prefix[lo]) / (hi - lo) as f64
        })
        .collect()
}

/// Pearson correlation, or 0 when either side doesn't vary.
pub fn correlation(a: &[f64], b: &[f64]) -> f64 {
    let n = a.len().min(b.len());
    if n < 3 {
        return 0.0;
    }
    let ma = a[..n].iter().sum::<f64>() / n as f64;
    let mb = b[..n].iter().sum::<f64>() / n as f64;
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for i in 0..n {
        let (x, y) = (a[i] - ma, b[i] - mb);
        sab += x * y;
        saa += x * x;
        sbb += y * y;
    }
    if saa <= 0.0 || sbb <= 0.0 {
        0.0
    } else {
        sab / (saa * sbb).sqrt()
    }
}

/// Signs chosen from the data: how strongly it decided each one, for the notes.
const DECIDES: f64 = 0.3;

pub struct Built {
    pub channels: Vec<Channel>,
    pub rate: u16,
    pub notes: Vec<String>,
}

fn short(name: &str) -> String {
    // first letters of each word, then the rest of the last word, up to 8 characters
    let words: Vec<&str> = name.split_whitespace().collect();
    let mut s: String = words
        .iter()
        .take(words.len().saturating_sub(1))
        .filter_map(|w| w.chars().next())
        .collect();
    if let Some(last) = words.last() {
        s.push_str(last);
    }
    s.chars().take(8).collect()
}

pub fn build(log: &Log, rate: u16) -> Result<Built, String> {
    if log.len() < 2 {
        return Err("the log has fewer than two rows".into());
    }
    let t = log.time();
    let step = 1.0 / rate as f64;
    let steps = (log.duration() / step).floor() as usize + 1;
    let grid: Vec<f64> = (0..steps).map(|i| i as f64 * step).collect();
    let col = |name: &str, fill: Fill| {
        let window = if fill == Fill::Mean {
            ACCEL_WINDOW.max(step)
        } else {
            step
        };
        log.column(name).map(|y| resample(t, y, &grid, fill, window))
    };
    let sw = ((SIGN_WINDOW / step).round() as usize).max(1);
    let mut notes = Vec::new();
    let mut out: Vec<Channel> = Vec::new();
    let mut push = |name: &str, unit: &str, dec: i16, data: Vec<f64>| {
        out.push(Channel {
            name: name.to_string(),
            short: short(name),
            unit: unit.to_string(),
            dec,
            data,
        });
    };
    let scaled = |v: Vec<f64>, k: f64| v.into_iter().map(|x| x * k).collect::<Vec<_>>();

    let speed = col("speed", Fill::Linear).ok_or("the log has no speed column")?;

    // signs, decided from the data where there is enough of it
    let accel_y = col("accel_y", Fill::Mean);
    let mut long_sign = -1.0; // the sensor's y points to the rear
    if let Some(ay) = &accel_y {
        let dv: Vec<f64> = speed.windows(2).map(|w| (w[1] - w[0]) / step).collect();
        let r = correlation(&smooth(&ay[..dv.len()], sw), &smooth(&dv, sw));
        if r.abs() >= DECIDES {
            long_sign = if r > 0.0 { 1.0 } else { -1.0 };
        }
    }
    let lat = col("accel_x", Fill::Mean).map(|v| scaled(v, 1.0 / G));
    let yaw_raw = col("yaw_rate", Fill::Mean);
    // defaults measured in the game (BX, 2026): its yaw rate is already positive to
    // the right, and its steering angle positive to the left
    let mut yaw_sign = 1.0;
    let mut yaw_decided = false;
    if let (Some(lat), Some(yr)) = (&lat, &yaw_raw) {
        let turn: Vec<f64> = yr.iter().zip(&speed).map(|(w, v)| w * v).collect();
        let r = correlation(&smooth(lat, sw), &smooth(&turn, sw));
        if r.abs() >= DECIDES {
            // positive yaw (right) goes with negative lateral (acceleration to the right)
            yaw_sign = if r < 0.0 { 1.0 } else { -1.0 };
            yaw_decided = true;
        }
    }
    let yaw = yaw_raw.map(|v| scaled(v, yaw_sign * DEG));
    let steer_raw = col("steering", Fill::Linear);
    let mut steer_sign = -1.0;
    if let (Some(s), Some(y)) = (&steer_raw, &yaw) {
        let r = correlation(&smooth(s, sw), &smooth(y, sw));
        if r.abs() >= DECIDES && yaw_decided {
            steer_sign = if r > 0.0 { 1.0 } else { -1.0 };
        }
    }
    if !yaw_decided {
        notes.push("Not enough cornering to check the yaw-rate and steering signs; kept the defaults.".into());
    }

    push("Ground Speed", "km/h", 2, scaled(speed.clone(), 3.6));
    if let Some(v) = col("wheelspeed", Fill::Linear) {
        push("Drive Wheel Speed", "km/h", 2, scaled(v, 3.6));
    }
    if let Some(v) = col("rpm", Fill::Linear) {
        push("Engine RPM", "rpm", 0, v);
    }
    if let Some(v) = col("gear", Fill::Hold) {
        push("Gear", "", 0, v);
    }
    for (src, name) in [
        ("throttle", "Throttle Pos"),
        ("brake", "Brake Pos"),
        ("clutch", "Clutch Pos"),
        ("parkingbrake", "Handbrake Pos"),
    ] {
        if let Some(v) = col(src, Fill::Linear) {
            push(name, "%", 1, scaled(v, 100.0));
        }
    }
    if let Some(v) = steer_raw {
        push("Steering Angle", "deg", 1, scaled(v, steer_sign));
    }
    if let Some(v) = col("steering_input", Fill::Linear) {
        push("Steering Input", "%", 1, scaled(v, 100.0 * steer_sign));
    }
    if let Some(v) = col("engine_torque", Fill::Linear) {
        push("Engine Torque", "Nm", 1, v);
    }
    if let Some(v) = col("engine_load", Fill::Linear) {
        push("Engine Load", "%", 1, scaled(v, 100.0));
    }
    if let Some(v) = col("boost", Fill::Linear) {
        push("Boost Pressure", "kPa", 1, scaled(v, PSI_TO_KPA));
    }
    if let Some(v) = col("water_temp", Fill::Linear) {
        push("Water Temp", "C", 1, v);
    }
    if let Some(v) = col("oil_temp", Fill::Linear) {
        push("Oil Temp", "C", 1, v);
    }
    if let Some(v) = col("fuel", Fill::Linear) {
        push("Fuel Level", "%", 1, scaled(v, 100.0));
    }
    if let Some(v) = col("abs", Fill::Hold) {
        push("ABS Active", "", 0, v);
    }
    if let Some(v) = col("tcs", Fill::Hold) {
        push("TC Active", "", 0, v);
    }
    if let Some(v) = lat {
        push("CG Accel Lateral", "G", 3, v);
    }
    if let Some(v) = accel_y {
        push("CG Accel Longitudinal", "G", 3, scaled(v, long_sign / G));
    }
    // vertical with gravity taken out, so it reads 0 standing still; the sensor's
    // z reads -1 G at rest in some builds and +1 G in others, so the sign follows the data
    if let Some(v) = col("accel_z", Fill::Mean) {
        let mean = v.iter().sum::<f64>() / v.len() as f64;
        let k = if mean < 0.0 { -1.0 / G } else { 1.0 / G };
        push(
            "CG Accel Vertical",
            "G",
            3,
            v.into_iter().map(|x| x * k - 1.0).collect(),
        );
    }
    if let Some(v) = yaw {
        push("Chassis Yaw Rate", "deg/s", 2, v);
    }
    let px = col("pos_x", Fill::Linear);
    let py = col("pos_y", Fill::Linear);
    if let Some(v) = &px {
        push("Car Coord X", "m", 2, v.clone());
    }
    if let Some(v) = &py {
        push("Car Coord Y", "m", 2, v.clone());
    }
    if let Some(v) = col("pos_z", Fill::Linear) {
        push("Car Coord Z", "m", 2, v);
    }
    // the world's metres as degrees near (0, 0), so MoTeC can draw a track map
    if let (Some(x), Some(y)) = (&px, &py) {
        push("GPS Latitude", "deg", 7, scaled(y.clone(), 1.0 / METRES_PER_DEGREE));
        push("GPS Longitude", "deg", 7, scaled(x.clone(), 1.0 / METRES_PER_DEGREE));
    }

    // per wheel, for every wheel the recorder found
    let wheels: Vec<String> = log
        .names
        .iter()
        .filter_map(|n| n.strip_prefix("wheel_av_").map(String::from))
        .collect();
    for w in &wheels {
        let av = col(&format!("wheel_av_{w}"), Fill::Linear);
        let radius = col(&format!("wheel_radius_{w}"), Fill::Linear);
        if let (Some(av), Some(r)) = (&av, &radius) {
            let ws: Vec<f64> = av.iter().zip(r).map(|(a, r)| a * r).collect();
            let slip: Vec<f64> = ws
                .iter()
                .zip(&speed)
                .map(|(w, v)| (w - v) / v.max(1.0) * 100.0)
                .collect();
            push(&format!("Wheel Speed {w}"), "km/h", 2, scaled(ws, 3.6));
            push(&format!("Wheel Slip {w}"), "%", 1, slip);
        }
        if let Some(v) = av {
            push(&format!("Wheel Angular Speed {w}"), "rad/s", 2, v);
        }
        if let Some(v) = radius {
            push(&format!("Tire Radius {w}"), "mm", 1, scaled(v, 1000.0));
        }
        if let Some(v) = col(&format!("brake_temp_{w}"), Fill::Linear) {
            push(&format!("Brake Temp {w}"), "C", 1, v);
        }
    }
    if out.iter().any(|c| c.name.len() > 32) {
        notes.push("Some channel names were longer than MoTeC's 32 characters and are cut short.".into());
    }
    Ok(Built {
        channels: out,
        rate,
        notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling() {
        let t = [0.0, 0.1, 0.3];
        let y = [0.0, 1.0, 3.0];
        let g = [0.0, 0.05, 0.2, 0.3, 0.4];
        assert_eq!(resample(&t, &y, &g, Fill::Linear, 0.05), vec![0.0, 0.5, 2.0, 3.0, 3.0]);
        assert_eq!(resample(&t, &y, &g, Fill::Hold, 0.05), vec![0.0, 0.0, 1.0, 3.0, 3.0]);
        let m = resample(
            &[0.0, 0.01, 0.02, 0.03],
            &[1.0, 3.0, 5.0, 7.0],
            &[0.0, 0.02],
            Fill::Mean,
            0.02,
        );
        // windows [-0.01, 0.01) and [0.01, 0.03)
        assert_eq!(m, vec![1.0, 4.0]);
        assert_eq!(smooth(&[0.0, 3.0, 6.0, 9.0], 3), vec![1.5, 3.0, 6.0, 7.5]);
    }

    #[test]
    fn short_names() {
        assert_eq!(short("Ground Speed"), "GSpeed");
        assert_eq!(short("CG Accel Lateral"), "CALatera");
        assert_eq!(short("Gear"), "Gear");
    }
}
