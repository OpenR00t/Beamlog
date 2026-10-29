// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Beamlog contributors
//! Writing MoTeC i2 log files (.ld).
//!
//! The .ld format isn't published; this follows the layout established by
//! reverse engineering (and seen in files from other sim-racing exporters):
//!
//! ```text
//! 0     header (1762 bytes): pointers, device, date and time, driver,
//!       vehicle, venue, a "pro logging" marker and a short comment
//! 1762  event (1154 bytes): name, session, comment, venue pointer
//! 2916  channel headers (116 bytes each), linked by prev/next pointers
//! ...   channel data, one block per channel
//! ```
//!
//! Values are stored as 32-bit integers: stored = value x 10^dec, so a
//! channel's decimal places set its resolution.

use crate::channels::Channel;
use std::io::{self, Write};

pub const HEADER_SIZE: usize = 1762;
pub const EVENT_SIZE: usize = 64 + 64 + 1024 + 2;
pub const CHANNEL_SIZE: usize = 116;

const MARKER: u32 = 0x40;
const DEVICE_SERIAL: u32 = 12007;
const DEVICE_TYPE: &str = "ADL";
const DEVICE_VERSION: u16 = 420;
const PRO_LOGGING: u32 = 0x00D2_0822;
const TYPE_INT: u16 = 5; // integer data ...
const SIZE_INT32: u16 = 4; // ... of 4 bytes
const FIRST_CHANNEL_ID: u16 = 50000;

#[derive(Debug, Clone, Default)]
pub struct Session {
    /// dd/mm/yyyy
    pub date: String,
    /// hh:mm:ss
    pub time: String,
    pub driver: String,
    pub vehicle: String,
    pub venue: String,
    pub event: String,
    pub session: String,
    pub comment: String,
    pub short_comment: String,
}

struct Buf(Vec<u8>);

impl Buf {
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i16(&mut self, v: i16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn zeros(&mut self, n: usize) {
        self.0.resize(self.0.len() + n, 0);
    }
    /// fixed-width ASCII field, zero padded, cut to fit
    fn text(&mut self, s: &str, width: usize) {
        let bytes: Vec<u8> = s
            .chars()
            .map(|c| {
                if c.is_ascii() && !c.is_ascii_control() {
                    c as u8
                } else {
                    b'?'
                }
            })
            .take(width)
            .collect();
        let n = bytes.len();
        self.0.extend_from_slice(&bytes);
        self.zeros(width - n);
    }
}

fn stored(value: f64, dec: i16) -> i32 {
    let v = (value * 10f64.powi(dec as i32)).round();
    v.clamp(i32::MIN as f64, i32::MAX as f64) as i32
}

/// The whole .ld file for these channels, all at `rate` Hz.
pub fn encode(session: &Session, channels: &[Channel], rate: u16) -> Vec<u8> {
    let n = channels.len();
    let meta_ptr = HEADER_SIZE + EVENT_SIZE;
    let data_ptr = meta_ptr + n * CHANNEL_SIZE;
    let mut b = Buf(Vec::with_capacity(
        data_ptr + channels.iter().map(|c| c.data.len() * 4).sum::<usize>(),
    ));

    // header
    b.u32(MARKER);
    b.zeros(4);
    b.u32(if n > 0 { meta_ptr as u32 } else { 0 });
    b.u32(data_ptr as u32);
    b.zeros(20);
    b.u32(HEADER_SIZE as u32); // event
    b.zeros(24);
    b.u16(0);
    b.u16(0x4240);
    b.u16(0x000F);
    b.u32(DEVICE_SERIAL);
    b.text(DEVICE_TYPE, 8);
    b.u16(DEVICE_VERSION);
    b.u16(0x0080);
    b.u32(n as u32);
    b.u16(10);
    b.u16(10);
    b.text(&session.date, 16);
    b.zeros(16);
    b.text(&session.time, 16);
    b.zeros(16);
    b.text(&session.driver, 64);
    b.text(&session.vehicle, 64);
    b.zeros(64);
    b.text(&session.venue, 64);
    b.zeros(64);
    b.zeros(1024);
    b.u32(PRO_LOGGING);
    b.zeros(66);
    b.text(&session.short_comment, 64);
    b.zeros(126);
    debug_assert_eq!(b.0.len(), HEADER_SIZE);

    // event (no venue record)
    b.text(&session.event, 64);
    b.text(&session.session, 64);
    b.text(&session.comment, 1024);
    b.u16(0);
    debug_assert_eq!(b.0.len(), meta_ptr);

    // channel headers
    let mut data_at = data_ptr;
    for (i, c) in channels.iter().enumerate() {
        let here = meta_ptr + i * CHANNEL_SIZE;
        b.u32(if i == 0 { 0 } else { (here - CHANNEL_SIZE) as u32 });
        b.u32(if i + 1 < n { (here + CHANNEL_SIZE) as u32 } else { 0 });
        b.u32(data_at as u32);
        b.u32(c.data.len() as u32);
        b.u16(FIRST_CHANNEL_ID.wrapping_add(i as u16));
        b.u16(TYPE_INT);
        b.u16(SIZE_INT32);
        b.u16(rate);
        b.i16(0); // shift
        b.i16(1); // multiplier
        b.i16(1); // scale
        b.i16(c.dec);
        b.text(&c.name, 32);
        b.text(&c.short, 8);
        b.text(&c.unit, 12);
        b.zeros(32);
        data_at += c.data.len() * 4;
    }
    debug_assert_eq!(b.0.len(), data_ptr);

    // channel data
    for c in channels {
        for &v in &c.data {
            b.0.extend_from_slice(&stored(v, c.dec).to_le_bytes());
        }
    }
    b.0
}

pub fn write(path: &std::path::Path, session: &Session, channels: &[Channel], rate: u16) -> io::Result<()> {
    let bytes = encode(session, channels, rate);
    let mut f = std::fs::File::create(path)?;
    f.write_all(&bytes)
}

/// One channel as read back: name, unit, rate (Hz), values.
pub type Decoded = (String, String, u16, Vec<f64>);

/// Read back what `encode` wrote. Used by the tests and by `beamlog check`.
pub fn decode(bytes: &[u8]) -> Result<Vec<Decoded>, String> {
    let u32_at = |o: usize| -> Result<u32, String> {
        bytes
            .get(o..o + 4)
            .map(|s| u32::from_le_bytes(s.try_into().unwrap()))
            .ok_or_else(|| format!("short file at {o}"))
    };
    let u16_at = |o: usize| -> Result<u16, String> {
        bytes
            .get(o..o + 2)
            .map(|s| u16::from_le_bytes(s.try_into().unwrap()))
            .ok_or_else(|| format!("short file at {o}"))
    };
    let text_at = |o: usize, w: usize| -> Result<String, String> {
        let s = bytes.get(o..o + w).ok_or_else(|| format!("short file at {o}"))?;
        Ok(String::from_utf8_lossy(s).trim_end_matches('\0').trim().to_string())
    };
    if u32_at(0)? != MARKER {
        return Err("not an .ld file".into());
    }
    let mut out = Vec::new();
    let mut p = u32_at(8)? as usize;
    let mut guard = 0;
    while p != 0 && guard < 10_000 {
        let next = u32_at(p + 4)? as usize;
        let data = u32_at(p + 8)? as usize;
        let count = u32_at(p + 12)? as usize;
        let (kind, size, rate) = (u16_at(p + 18)?, u16_at(p + 20)?, u16_at(p + 22)?);
        let dec = u16_at(p + 30)? as i16;
        if !(kind == TYPE_INT && size == SIZE_INT32) {
            return Err(format!("unexpected data type {kind}/{size}"));
        }
        let name = text_at(p + 32, 32)?;
        let unit = text_at(p + 72, 12)?;
        let mut values = Vec::with_capacity(count);
        for i in 0..count {
            let raw = u32_at(data + i * 4)? as i32;
            values.push(raw as f64 / 10f64.powi(dec as i32));
        }
        out.push((name, unit, rate, values));
        p = next;
        guard += 1;
    }
    Ok(out)
}
