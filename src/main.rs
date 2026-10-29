// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Beamlog contributors
//! beamlog: turns BeamNG.drive driving logs into MoTeC i2 files.
//!
//! ```text
//! beamlog                     convert every finished log in BeamNG's beamlog folder
//! beamlog watch               keep doing that, every few seconds, until stopped
//! beamlog FILE.csv ...        convert these logs
//! beamlog info FILE ...       print a summary of each run (.ld or .csv)
//! beamlog check FILE.ld ...   list the channels in a converted file
//!
//! options: --out DIR    where the .ld/.ldx files go (default: beside each log)
//!          --rate HZ    channel rate (default: as recorded, or 50)
//! ```

mod channels;
mod ld;
mod ldx;
mod log;
mod summary;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const DEFAULT_RATE: u16 = 50;
/// an unfinished log that hasn't changed for this long is treated as finished
/// (the recorder flushes every second while it records)
const STALE: Duration = Duration::from_secs(30);

fn user_folder() -> Option<PathBuf> {
    if let Ok(p) = env::var("BEAMLOG_BEAMNG_USER") {
        let p = PathBuf::from(p);
        if p.is_dir() {
            return Some(p);
        }
    }
    let mut candidates = Vec::new();
    if let Ok(local) = env::var("LOCALAPPDATA") {
        let local = PathBuf::from(local);
        candidates.push(local.join("BeamNG").join("BeamNG.drive").join("current"));
        candidates.push(local.join("BeamNG.drive").join("current"));
    }
    if let Ok(home) = env::var("HOME") {
        candidates.push(PathBuf::from(home).join(".local/share/BeamNG/BeamNG.drive/current"));
    }
    candidates.into_iter().find(|p| p.is_dir())
}

struct Options {
    out: Option<PathBuf>,
    /// channel rate; None: the rate the log was recorded at, or DEFAULT_RATE
    rate: Option<u16>,
}

/// "2026-10-29" + "14:40:05" → ("29/10/2026", "14:40:05")
fn motec_date(date: &str, time: &str) -> (String, String) {
    let d: Vec<&str> = date.split('-').collect();
    let date = if d.len() == 3 {
        format!("{}/{}/{}", d[2], d[1], d[0])
    } else {
        date.to_string()
    };
    (date, time.to_string())
}

fn convert(path: &Path, opts: &Options) -> Result<PathBuf, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let log = log::parse(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let recorded = log
        .meta
        .get("rate")
        .and_then(|r| r.parse::<u16>().ok())
        .filter(|r| *r > 0);
    let rate = opts.rate.or(recorded).unwrap_or(DEFAULT_RATE);
    let built = channels::build(&log, rate).map_err(|e| format!("{}: {e}", path.display()))?;
    let get = |k: &str| log.meta.get(k).cloned().unwrap_or_default();
    let (date, time) = motec_date(&get("date"), &get("time"));
    let config = get("config");
    let session = ld::Session {
        date,
        time,
        driver: String::new(),
        vehicle: if config.is_empty() {
            get("vehicle")
        } else {
            format!("{} {}", get("vehicle"), config)
        },
        venue: "BeamNG.drive".into(),
        event: "BeamNG.drive".into(),
        session: path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default(),
        comment: "Recorded with beamlog".into(),
        short_comment: config,
    };
    let dir = opts
        .out
        .clone()
        .unwrap_or_else(|| path.parent().map(Path::to_path_buf).unwrap_or_default());
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let stem = path.file_stem().ok_or("no file name")?;
    let ld_path = dir.join(stem).with_extension("ld");
    ld::write(&ld_path, &session, &built.channels, built.rate).map_err(|e| format!("{}: {e}", ld_path.display()))?;
    fs::write(ld_path.with_extension("ldx"), ldx::encode(&log.laps, log.duration()))
        .map_err(|e| format!("{}: {e}", ld_path.display()))?;
    for n in &built.notes {
        eprintln!("  note: {n}");
    }
    Ok(ld_path)
}

/// The channels `info` needs, from an .ld file (and the laps from its .ldx).
fn log_from_ld(bytes: &[u8], path: &Path) -> Result<log::Log, String> {
    let chans = ld::decode(bytes)?;
    let get = |name: &str| chans.iter().find(|c| c.0 == name);
    let (_, _, rate, speed) = get("Ground Speed").ok_or("no Ground Speed channel")?;
    let rate = f64::from(*rate);
    let mut names = vec!["time".to_string(), "speed".to_string()];
    let mut columns = vec![
        (0..speed.len()).map(|i| i as f64 / rate).collect::<Vec<_>>(),
        speed.iter().map(|v| v / 3.6).collect(),
    ];
    for (motec, ours) in [("Engine RPM", "rpm"), ("Gear", "gear")] {
        if let Some(c) = get(motec) {
            names.push(ours.into());
            columns.push(c.3.clone());
        }
    }
    let ldx = fs::read_to_string(path.with_extension("ldx")).unwrap_or_default();
    let laps = ldx
        .split("Time=\"")
        .skip(1)
        .filter_map(|s| s.split('"').next()?.parse::<f64>().ok())
        .map(|us| us / 1e6)
        .collect();
    let vehicle = bytes
        .get(222..286)
        .map(|b| String::from_utf8_lossy(b).trim_end_matches('\0').to_string());
    let mut meta = std::collections::BTreeMap::new();
    meta.insert("vehicle".to_string(), vehicle.unwrap_or_default());
    let units = vec![String::new(); names.len()];
    Ok(log::Log {
        version: 1,
        meta,
        names,
        units,
        columns,
        laps,
        complete: true,
    })
}

fn modified(p: &Path) -> Option<SystemTime> {
    fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Logs that are finished and not yet converted (or changed since): everything
/// in the log folder, plus `beamlog_*.csv` in the user folder itself, where the
/// recorder writes if it couldn't create the folder.
fn pending(dir: &Path, opts: &Options) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map(|r| r.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    if let Some(root) = dir.parent() {
        if let Ok(r) = fs::read_dir(root) {
            files.extend(r.flatten().map(|e| e.path()).filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("beamlog_"))
            }));
        }
    }
    let mut out = Vec::new();
    for p in files {
        if p.extension().and_then(|x| x.to_str()) != Some("csv") {
            continue;
        }
        let Some(m) = modified(&p) else { continue };
        let out_dir = opts
            .out
            .clone()
            .unwrap_or_else(|| p.parent().map(Path::to_path_buf).unwrap_or_default());
        let ld_path = out_dir.join(p.file_stem().unwrap()).with_extension("ld");
        if modified(&ld_path).is_some_and(|l| l >= m) {
            continue;
        }
        let finished = fs::read_to_string(&p)
            .map(|t| t.trim_end().ends_with("# end"))
            .unwrap_or(false)
            || SystemTime::now().duration_since(m).unwrap_or_default() > STALE;
        if finished {
            out.push(p);
        }
    }
    out.sort();
    out
}

/// Logs still being written (no closing line, changed recently).
fn recording(dir: &Path) -> Vec<PathBuf> {
    let Ok(r) = fs::read_dir(dir) else { return Vec::new() };
    r.flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("csv"))
        .filter(|p| {
            let fresh = modified(p)
                .and_then(|m| SystemTime::now().duration_since(m).ok())
                .is_some_and(|d| d <= STALE);
            fresh
                && !fs::read_to_string(p)
                    .map(|t| t.trim_end().ends_with("# end"))
                    .unwrap_or(true)
        })
        .collect()
}

fn convert_pending(dir: &Path, opts: &Options) -> usize {
    let mut n = 0;
    for p in pending(dir, opts) {
        match convert(&p, opts) {
            Ok(ld) => {
                println!("{} -> {}", p.display(), ld.display());
                n += 1;
            }
            Err(e) => eprintln!("skipped {e}"),
        }
    }
    n
}

/// True when this process owns its console window alone, i.e. it was started by
/// double-clicking the .exe rather than from a terminal: the window would close
/// before anyone could read it, so wait for Enter.
#[cfg(windows)]
fn own_console() -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    }
    let mut ids = [0u32; 4];
    // SAFETY: the buffer and its length match; the call only writes into it.
    unsafe { GetConsoleProcessList(ids.as_mut_ptr(), ids.len() as u32) == 1 }
}

#[cfg(not(windows))]
fn own_console() -> bool {
    false
}

fn pause_if_double_clicked() {
    if own_console() {
        println!("\nPress Enter to close.");
        let _ = std::io::stdin().read_line(&mut String::new());
    }
}

fn main() {
    run();
    pause_if_double_clicked();
}

fn run() {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let mut opts = Options { out: None, rate: None };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--out" if i + 1 < args.len() => {
                opts.out = Some(PathBuf::from(args.remove(i + 1)));
                args.remove(i);
            }
            "--rate" if i + 1 < args.len() => {
                opts.rate = Some(
                    args.remove(i + 1)
                        .parse()
                        .ok()
                        .filter(|r| (1..=1000).contains(r))
                        .unwrap_or_else(|| {
                            eprintln!("--rate must be 1 to 1000");
                            std::process::exit(2)
                        }),
                );
                args.remove(i);
            }
            "-h" | "--help" => {
                println!("{}", include_str!("usage.txt"));
                return;
            }
            _ => i += 1,
        }
    }

    let logs_dir = || -> PathBuf {
        match user_folder() {
            Some(u) => u.join("beamlog"),
            None => {
                eprintln!("BeamNG's user folder wasn't found; set BEAMLOG_BEAMNG_USER to it, or pass log files.");
                pause_if_double_clicked();
                std::process::exit(1)
            }
        }
    };

    match args.first().map(String::as_str) {
        None => {
            let dir = logs_dir();
            let n = convert_pending(&dir, &opts);
            println!("{n} log(s) converted from {}", dir.display());
        }
        Some("watch") => {
            let dir = logs_dir();
            println!("Watching {} (Ctrl+C to stop)", dir.display());
            println!(
                "A run is converted once it ends: reset or leave the car, stand still for a minute, or quit the game."
            );
            let mut shown = std::collections::BTreeSet::new();
            loop {
                convert_pending(&dir, &opts);
                for p in recording(&dir) {
                    if shown.insert(p.clone()) {
                        println!("recording: {}", p.display());
                    }
                }
                std::thread::sleep(Duration::from_secs(5));
            }
        }
        Some("check") => {
            for f in &args[1..] {
                match fs::read(f).map_err(|e| e.to_string()).and_then(|b| ld::decode(&b)) {
                    Ok(chans) => {
                        println!("{f}: {} channels", chans.len());
                        for (name, unit, rate, v) in chans {
                            let (lo, hi) = v.iter().fold((f64::MAX, f64::MIN), |(a, b), &x| (a.min(x), b.max(x)));
                            println!("  {name} [{unit}] {rate} Hz, {} samples, {lo} to {hi}", v.len());
                        }
                    }
                    Err(e) => eprintln!("{f}: {e}"),
                }
            }
        }
        Some("info") => {
            for f in &args[1..] {
                let read = if f.to_lowercase().ends_with(".ld") {
                    fs::read(f)
                        .map_err(|e| e.to_string())
                        .and_then(|b| log_from_ld(&b, Path::new(f)))
                } else {
                    fs::read_to_string(f)
                        .map_err(|e| e.to_string())
                        .and_then(|t| log::parse(&t))
                };
                match read {
                    Ok(l) => println!("{}\n{}\n", f, summary::summary(&l)),
                    Err(e) => eprintln!("{f}: {e}"),
                }
            }
        }
        Some(_) => {
            let mut failed = false;
            for f in &args {
                match convert(Path::new(f), &opts) {
                    Ok(ld) => println!("{f} -> {}", ld.display()),
                    Err(e) => {
                        eprintln!("{e}");
                        failed = true;
                    }
                }
            }
            if failed {
                pause_if_double_clicked();
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_log() -> String {
        // a car accelerating in a straight line, then turning right at a steady speed
        let mut s =
            String::from("# beamlog 1\n# date 2026-10-29\n# time 14:40:05\n# vehicle bx\n# config test_config\n");
        s.push_str("# units s,m/s,rpm,,ratio,deg,m/s2,m/s2,m/s2,rad/s,m,m,m,rad/s,m\n");
        s.push_str("time,speed,rpm,gear,throttle,steering,accel_x,accel_y,accel_z,yaw_rate,pos_x,pos_y,pos_z,wheel_av_RL,wheel_radius_RL\n");
        let (mut t, mut v, mut x, mut y, mut h) = (0.0f64, 0.0f64, 0.0, 0.0, 0.0f64);
        while t < 20.0 {
            let dt = 1.0 / 60.0 + 0.002 * (t * 7.0).sin(); // uneven frames
            let (a, w) = if t < 10.0 { (3.0, 0.0) } else { (0.0, 0.3) }; // right turn: heading decreasing
            let gear = if v < 15.0 { 1 } else { 2 };
            let rpm = v * if gear == 1 { 400.0 } else { 250.0 };
            // sensor frame: y to the rear, x to the left; game yaw rate positive to the left here
            let (ax, ay) = (-w * v, -a);
            // the game's sensor shakes with the chassis: about 2 m/s^2 frame to frame
            let shake = if (t * 60.0) as i64 % 2 == 0 { 2.0 } else { -2.0 };
            let (ax, ay) = (ax + shake, ay - shake);
            s.push_str(&format!(
                "{t:.4},{v:.4},{rpm:.1},{gear},1,{steer:.2},{ax:.4},{ay:.4},-9.81,{w:.4},{x:.3},{y:.3},0,{av:.3},0.3\n",
                steer = -w * 100.0, // the game steers positive to the left
                av = v / 0.3
            ));
            v += a * dt;
            h -= w * dt;
            x += v * h.sin() * dt;
            y += v * h.cos() * dt;
            t += dt;
        }
        s.push_str("# end\n");
        s
    }

    #[test]
    fn parse_and_summary() {
        let l = log::parse(&sample_log()).unwrap();
        assert!(l.complete);
        assert_eq!(l.meta["vehicle"], "bx");
        assert!((l.duration() - 20.0).abs() < 0.1);
        let s = summary::summary(&l);
        assert!(s.contains("Top speed 108"), "{s}");
        assert!(s.contains("Upshifts: 1→2"), "{s}");
        assert!(s.contains("0-100 km/h: 9."), "{s}");
        let with_laps = sample_log().replace("# end\n", "# lap 5.0\n# lap 12.5\n# end\n");
        assert_eq!(log::parse(&with_laps).unwrap().laps, vec![5.0, 12.5]);
        assert!(summary::summary(&log::parse(&with_laps).unwrap()).contains("Laps: 0:07.500"));
        assert!(log::parse("time,a\n1,2\n").is_err());
        assert!(log::parse("# beamlog 9\ntime\n1\n").is_err());
    }

    #[test]
    fn channels_signs_and_round_trip() {
        let l = log::parse(&sample_log()).unwrap();
        let b = channels::build(&l, 50).unwrap();
        let ch = |n: &str| b.channels.iter().find(|c| c.name == n).unwrap_or_else(|| panic!("{n}"));
        assert_eq!(ch("Ground Speed").data.len(), (l.duration() * 50.0) as usize + 1);
        // speeding up reads positive; the right turn reads as negative lateral, positive yaw and steering
        let mid = 5 * 50;
        assert!(ch("CG Accel Longitudinal").data[mid] > 0.25);
        let turn = 15 * 50;
        assert!(ch("CG Accel Lateral").data[turn] < -0.5);
        assert!(ch("Chassis Yaw Rate").data[turn] > 10.0);
        assert!(ch("Steering Angle").data[turn] > 0.0);
        assert!(ch("Wheel Speed RL").data[turn] > 100.0);
        assert!(ch("CG Accel Vertical").data[mid].abs() < 0.05);
        assert!(b.notes.is_empty(), "{:?}", b.notes);

        let session = ld::Session {
            date: "29/10/2026".into(),
            time: "14:40:05".into(),
            vehicle: "bx".into(),
            ..Default::default()
        };
        let bytes = ld::encode(&session, &b.channels, 50);
        assert_eq!(&bytes[94..104], b"29/10/2026");
        let back = ld::decode(&bytes).unwrap();
        assert_eq!(back.len(), b.channels.len());
        for (c, (name, unit, rate, values)) in b.channels.iter().zip(&back) {
            assert_eq!((&c.name, &c.unit, *rate), (name, unit, 50));
            let tol = 0.51 * 10f64.powi(-(c.dec as i32));
            for (a, v) in c.data.iter().zip(values) {
                assert!((a - v).abs() <= tol, "{name}: {a} vs {v}");
            }
        }
    }

    #[test]
    fn dates() {
        assert_eq!(
            motec_date("2026-10-29", "14:40:05"),
            ("29/10/2026".into(), "14:40:05".into())
        );
    }
}
