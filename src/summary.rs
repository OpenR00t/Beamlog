// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Beamlog contributors
//! A short text summary of a run: what a quick look in MoTeC would tell you.

use crate::log::Log;

pub fn summary(log: &Log) -> String {
    let t = log.time();
    let speed: Vec<f64> = log
        .column("speed")
        .map(|v| v.iter().map(|x| x * 3.6).collect())
        .unwrap_or_default();
    let rpm = log.column("rpm").unwrap_or(&[]);
    let gear = log.column("gear").unwrap_or(&[]);
    let mut out = Vec::new();
    let vehicle = format!(
        "{}{}",
        log.meta.get("vehicle").cloned().unwrap_or_default(),
        log.meta
            .get("config")
            .filter(|c| !c.is_empty())
            .map(|c| format!(" ({c})"))
            .unwrap_or_default()
    );
    let distance: f64 = t
        .windows(2)
        .zip(speed.windows(2))
        .map(|(tw, sw)| (tw[1] - tw[0]) * (sw[0] + sw[1]) / 2.0 / 3.6)
        .sum();
    out.push(format!(
        "{vehicle}: {:.1} s, {:.2} km, {} rows{}",
        log.duration(),
        distance / 1000.0,
        log.len(),
        if log.complete {
            ""
        } else {
            " (run still open or cut short)"
        }
    ));
    if speed.is_empty() {
        return out.join("\n");
    }
    let (i_top, top) = speed
        .iter()
        .copied()
        .enumerate()
        .fold((0, f64::MIN), |a, (i, v)| if v > a.1 { (i, v) } else { a });
    let at = |col: &[f64], i: usize| col.get(i).copied().unwrap_or(f64::NAN);
    out.push(format!(
        "Top speed {top:.1} km/h at {:.0} rpm in gear {:.0}, {:.1} s in",
        at(rpm, i_top),
        at(gear, i_top),
        t[i_top]
    ));

    // per gear
    if !gear.is_empty() {
        let mut gears: Vec<i64> = gear.iter().map(|g| *g as i64).filter(|g| *g > 0).collect();
        gears.sort_unstable();
        gears.dedup();
        for g in gears {
            let idx: Vec<usize> = (0..gear.len()).filter(|&i| gear[i] as i64 == g).collect();
            let time: f64 = idx.iter().filter(|&&i| i + 1 < t.len()).map(|&i| t[i + 1] - t[i]).sum();
            let vmax = idx.iter().map(|&i| speed[i]).fold(f64::MIN, f64::max);
            let rmax = idx.iter().map(|&i| at(rpm, i)).fold(f64::MIN, f64::max);
            out.push(format!(
                "  gear {g}: {time:.1} s, up to {vmax:.1} km/h and {rmax:.0} rpm"
            ));
        }
        // upshifts: rpm just before the change, and once the new gear has settled
        // upshifts, through neutral or not: from the last forward gear to the next one
        let mut shifts = Vec::new();
        let mut last: Option<(f64, usize)> = None; // gear, last index in it
        for i in 0..gear.len() {
            if gear[i] < 1.0 {
                continue;
            }
            if let Some((g, j)) = last {
                if gear[i] > g {
                    let before = (0..=j)
                        .rev()
                        .take_while(|&k| t[j] - t[k] < 0.5)
                        .map(|k| at(rpm, k))
                        .fold(f64::MIN, f64::max);
                    let after = (i..gear.len()).find(|&k| t[k] - t[i] >= 0.4).map(|k| at(rpm, k));
                    if let Some(after) = after {
                        shifts.push(format!(
                            "{g:.0}→{:.0} {before:.0}→{after:.0} rpm at {:.0} km/h",
                            gear[i], speed[j]
                        ));
                    }
                }
            }
            last = Some((gear[i], i));
        }
        if !shifts.is_empty() {
            out.push(format!("Upshifts: {}", shifts.join("; ")));
        }
    }
    // laps between marks
    if log.laps.len() >= 2 {
        let times: Vec<String> = log.laps.windows(2).map(|w| crate::ldx::lap_time(w[1] - w[0])).collect();
        out.push(format!("Laps: {}", times.join(", ")));
    }
    // standing starts to 100 km/h
    let mut starts = Vec::new();
    let mut from: Option<usize> = None;
    for i in 0..speed.len() {
        if speed[i] < 2.0 {
            from = Some(i);
        } else if speed[i] >= 100.0 {
            if let Some(s) = from.take() {
                starts.push(format!("{:.2} s", t[i] - t[s]));
            }
        }
    }
    if !starts.is_empty() {
        out.push(format!("0-100 km/h: {}", starts.join(", ")));
    }
    out.join("\n")
}
