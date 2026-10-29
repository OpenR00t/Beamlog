// SPDX-License-Identifier: AGPL-3.0-or-later
// Copyright (C) 2026 The Beamlog contributors
//! The .ldx file MoTeC keeps beside each .ld: lap markers (beacons) and a few
//! details. Times are in microseconds from the start of the log.

/// `laps`: start time of each lap after the first, in seconds.
pub fn encode(laps: &[f64], duration: f64) -> String {
    let mut markers = String::new();
    for (i, t) in laps.iter().enumerate() {
        markers.push_str(&format!(
            "\t\t\t\t\t<Marker Version=\"100\" ClassName=\"BCN\" Name=\"Manual.{}\" Flags=\"77\" Time=\"{:.6}\"/>\n",
            i + 1,
            t * 1e6
        ));
    }
    // lap times between beacons, and the last part to the end
    let mut edges = vec![0.0];
    edges.extend_from_slice(laps);
    edges.push(duration);
    let times: Vec<f64> = edges.windows(2).map(|w| w[1] - w[0]).collect();
    let mut details = format!("\t\t\t<String Id=\"Total Laps\" Value=\"{}\"/>\n", times.len());
    // a full lap lies between two beacons, so only those count for the fastest
    if laps.len() >= 2 {
        let (best_i, best) = times[1..times.len() - 1]
            .iter()
            .enumerate()
            .fold((0, f64::MAX), |acc, (i, &t)| if t < acc.1 { (i + 1, t) } else { acc });
        details.push_str(&format!(
            "\t\t\t<String Id=\"Fastest Time\" Value=\"{}\"/>\n",
            lap_time(best)
        ));
        details.push_str(&format!("\t\t\t<String Id=\"Fastest Lap\" Value=\"{best_i}\"/>\n"));
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<LDXFile Version=\"1.5\">\n\t<Layers>\n\t\t<Layer>\n\t\t\t<MarkerBlock>\n\
         \t\t\t\t<MarkerGroup Name=\"Beacons\" Index=\"3\">\n{markers}\t\t\t\t</MarkerGroup>\n\t\t\t</MarkerBlock>\n\t\t</Layer>\n\
         \t\t<Details>\n{details}\t\t</Details>\n\t</Layers>\n</LDXFile>\n"
    )
}

/// m:ss.mmm
pub fn lap_time(s: f64) -> String {
    let ms = (s * 1000.0).round() as u64;
    format!("{}:{:02}.{:03}", ms / 60_000, (ms / 1000) % 60, ms % 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_and_details() {
        let x = encode(&[10.0, 70.5, 129.0], 140.0);
        assert!(x.contains("Name=\"Manual.2\" Flags=\"77\" Time=\"70500000.000000\""));
        assert!(x.contains("Id=\"Total Laps\" Value=\"4\""));
        assert!(x.contains("Id=\"Fastest Time\" Value=\"0:58.500\""));
        assert!(x.contains("Id=\"Fastest Lap\" Value=\"2\""));
        let none = encode(&[], 30.0);
        assert!(none.contains("Value=\"1\"") && !none.contains("Marker Version"));
        assert_eq!(lap_time(118.745), "1:58.745");
    }
}
