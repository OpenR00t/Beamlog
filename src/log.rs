// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Beamlog contributors
//! Reading the CSV logs written by the in-game recorder.
//!
//! ```text
//! # beamlog 1
//! # date 2026-10-29
//! # time 14:40:05
//! # vehicle bx
//! # config crankcase_v6
//! # rate 0
//! # units s,m/s,rpm,...
//! time,speed,rpm,...
//! 0.0167,0.42,950,...
//! # lap 61.2500
//! # end
//! ```

use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Log {
    #[allow(dead_code)]
    pub version: u32,
    pub meta: BTreeMap<String, String>,
    pub names: Vec<String>,
    #[allow(dead_code)]
    pub units: Vec<String>,
    /// one vector per column, `columns[0]` is time in seconds
    pub columns: Vec<Vec<f64>>,
    /// times (s) at which a lap was marked
    pub laps: Vec<f64>,
    /// the recorder wrote its closing line (the run is over)
    pub complete: bool,
}

impl Log {
    pub fn column(&self, name: &str) -> Option<&[f64]> {
        self.names
            .iter()
            .position(|n| n == name)
            .map(|i| self.columns[i].as_slice())
    }
    pub fn time(&self) -> &[f64] {
        &self.columns[0]
    }
    pub fn len(&self) -> usize {
        self.columns.first().map_or(0, |c| c.len())
    }
    pub fn duration(&self) -> f64 {
        self.time().last().copied().unwrap_or(0.0)
    }
}

pub fn parse(text: &str) -> Result<Log, String> {
    let mut meta = BTreeMap::new();
    let mut version = 0;
    let mut names: Vec<String> = Vec::new();
    let mut units: Vec<String> = Vec::new();
    let mut columns: Vec<Vec<f64>> = Vec::new();
    let mut complete = false;
    let mut laps = Vec::new();

    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            let rest = rest.trim();
            let (key, value) = rest.split_once(' ').unwrap_or((rest, ""));
            match key {
                "beamlog" => {
                    version = value
                        .trim()
                        .parse()
                        .map_err(|_| format!("line {}: bad version", n + 1))?
                }
                "end" => complete = true,
                "lap" => {
                    if let Ok(t) = value.trim().parse::<f64>() {
                        laps.push(t);
                    }
                }
                "units" => units = value.split(',').map(|s| s.trim().to_string()).collect(),
                _ => {
                    meta.insert(key.to_string(), value.trim().to_string());
                }
            }
            continue;
        }
        if version == 0 {
            return Err("not a beamlog file (no '# beamlog' line)".into());
        }
        if names.is_empty() {
            names = line.split(',').map(|s| s.trim().to_string()).collect();
            if names.first().map(String::as_str) != Some("time") {
                return Err("the first column must be time".into());
            }
            columns = vec![Vec::new(); names.len()];
            continue;
        }
        let values: Vec<&str> = line.split(',').collect();
        if values.len() != names.len() {
            // a row cut short by the game closing mid-write: stop here
            break;
        }
        let mut row = Vec::with_capacity(values.len());
        for v in values {
            match v.trim().parse::<f64>() {
                Ok(x) if x.is_finite() => row.push(x),
                _ => {
                    row.clear();
                    break;
                }
            }
        }
        if row.len() != names.len() {
            break;
        }
        for (col, x) in columns.iter_mut().zip(row) {
            col.push(x);
        }
    }
    if version == 0 {
        return Err("not a beamlog file (no '# beamlog' line)".into());
    }
    if version > 1 {
        return Err(format!("written by a newer recorder (format {version})"));
    }
    if names.is_empty() {
        return Err("no column names".into());
    }
    if units.len() != names.len() {
        units = vec![String::new(); names.len()];
    }
    Ok(Log {
        version,
        meta,
        names,
        units,
        columns,
        laps,
        complete,
    })
}
