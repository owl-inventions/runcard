//! Reads a GPX file into a `Track` (the external crate is reachable as `::gpx`).

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use crate::error::{Error, Result};
use crate::model::{Track, TrackPoint};

/// Reads and parses the GPX file at `path`.
pub fn load(path: &Path) -> Result<Track> {
    parse(BufReader::new(File::open(path)?))
}

/// Parses GPX XML into one `Track`: every track and segment, in file order.
///
/// The name is the first track's name, else the metadata name. Points without
/// a `<time>` are kept; whether timestamps are required is up to the caller.
pub fn parse(reader: impl Read) -> Result<Track> {
    let gpx = ::gpx::read(reader).map_err(|e| Error::Gpx(e.to_string()))?;
    let name = gpx
        .tracks
        .first()
        .and_then(|t| t.name.clone())
        .or_else(|| gpx.metadata.as_ref().and_then(|m| m.name.clone()));
    let points: Vec<TrackPoint> = gpx
        .tracks
        .iter()
        .flat_map(|t| &t.segments)
        .flat_map(|s| &s.points)
        .map(track_point)
        .collect();
    if points.len() < 2 {
        return Err(Error::TooFewPoints);
    }
    Ok(Track { name, points })
}

fn track_point(waypoint: &::gpx::Waypoint) -> TrackPoint {
    let position = waypoint.point();
    TrackPoint {
        lat: position.y(),
        lon: position.x(),
        time: waypoint.time.map(Into::into),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;

    fn gpx(body: &str) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="runcard-tests" xmlns="http://www.topografix.com/GPX/1/1">
{body}
</gpx>"#
        )
    }

    fn trkpt(lat: f64, lon: f64, time: &str) -> String {
        format!(r#"<trkpt lat="{lat}" lon="{lon}"><time>{time}</time></trkpt>"#)
    }

    fn coordinates(track: &Track) -> Vec<(f64, f64)> {
        track.points.iter().map(|p| (p.lat, p.lon)).collect()
    }

    #[test]
    fn concatenates_segments_in_file_order() {
        let xml = gpx(&format!(
            "<trk><name>Long run</name><trkseg>{}{}</trkseg><trkseg>{}{}</trkseg></trk>",
            trkpt(52.50, 13.40, "2026-09-27T09:00:00Z"),
            trkpt(52.51, 13.41, "2026-09-27T09:01:00Z"),
            trkpt(52.52, 13.42, "2026-09-27T09:02:00Z"),
            trkpt(52.53, 13.43, "2026-09-27T09:03:00Z"),
        ));
        let track = parse(xml.as_bytes()).unwrap();
        assert_eq!(
            coordinates(&track),
            vec![
                (52.50, 13.40),
                (52.51, 13.41),
                (52.52, 13.42),
                (52.53, 13.43)
            ]
        );
    }

    #[test]
    fn concatenates_tracks_in_file_order() {
        let xml = gpx(&format!(
            "<trk><name>First</name><trkseg>{}{}</trkseg></trk><trk><name>Second</name><trkseg>{}</trkseg></trk>",
            trkpt(52.50, 13.40, "2026-09-27T09:00:00Z"),
            trkpt(52.51, 13.41, "2026-09-27T09:01:00Z"),
            trkpt(52.52, 13.42, "2026-09-27T09:02:00Z"),
        ));
        let track = parse(xml.as_bytes()).unwrap();
        assert_eq!(
            coordinates(&track),
            vec![(52.50, 13.40), (52.51, 13.41), (52.52, 13.42)]
        );
        assert_eq!(track.name.as_deref(), Some("First"));
    }

    #[test]
    fn reads_the_track_name_and_timestamps() {
        let xml = gpx(&format!(
            "<metadata><name>Metadata name</name></metadata><trk><name>Berlin Marathon</name><trkseg>{}{}</trkseg></trk>",
            trkpt(52.50, 13.40, "2026-09-27T09:00:00Z"),
            trkpt(52.51, 13.41, "2026-09-27T09:00:05Z"),
        ));
        let track = parse(xml.as_bytes()).unwrap();
        assert_eq!(track.name.as_deref(), Some("Berlin Marathon"));
        assert_eq!(
            track.points[0].time,
            Some(datetime!(2026-09-27 09:00:00 UTC))
        );
        assert_eq!(
            track.points[1].time,
            Some(datetime!(2026-09-27 09:00:05 UTC))
        );
    }

    #[test]
    fn falls_back_to_the_metadata_name() {
        let xml = gpx(&format!(
            "<metadata><name>Sunday long run</name></metadata><trk><trkseg>{}{}</trkseg></trk>",
            trkpt(52.50, 13.40, "2026-09-27T09:00:00Z"),
            trkpt(52.51, 13.41, "2026-09-27T09:01:00Z"),
        ));
        let track = parse(xml.as_bytes()).unwrap();
        assert_eq!(track.name.as_deref(), Some("Sunday long run"));
    }

    #[test]
    fn has_no_name_when_neither_track_nor_metadata_has_one() {
        let xml = gpx(&format!(
            "<trk><trkseg>{}{}</trkseg></trk>",
            trkpt(52.50, 13.40, "2026-09-27T09:00:00Z"),
            trkpt(52.51, 13.41, "2026-09-27T09:01:00Z"),
        ));
        assert_eq!(parse(xml.as_bytes()).unwrap().name, None);
    }

    #[test]
    fn missing_timestamps_are_not_an_error() {
        let xml = gpx(
            r#"<trk><trkseg><trkpt lat="52.5" lon="13.4"/><trkpt lat="52.51" lon="13.41"/></trkseg></trk>"#,
        );
        let track = parse(xml.as_bytes()).unwrap();
        assert!(track.points.iter().all(|p| p.time.is_none()));
    }

    #[test]
    fn malformed_xml_is_a_gpx_error() {
        let err = parse("<gpx version=\"1.1\"><trk><trkseg>".as_bytes()).unwrap_err();
        assert!(matches!(err, Error::Gpx(_)), "got {err:?}");
    }

    #[test]
    fn a_single_point_is_too_few() {
        let xml = gpx(&format!(
            "<trk><trkseg>{}</trkseg></trk>",
            trkpt(52.50, 13.40, "2026-09-27T09:00:00Z")
        ));
        let err = parse(xml.as_bytes()).unwrap_err();
        assert!(matches!(err, Error::TooFewPoints), "got {err:?}");
    }

    #[test]
    fn load_reads_a_file() {
        let dir = std::env::temp_dir().join(format!("runcard-unit-gpx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("two-points.gpx");
        let xml = gpx(&format!(
            "<trk><name>From disk</name><trkseg>{}{}</trkseg></trk>",
            trkpt(52.50, 13.40, "2026-09-27T09:00:00Z"),
            trkpt(52.51, 13.41, "2026-09-27T09:01:00Z"),
        ));
        std::fs::write(&path, xml).unwrap();
        let track = load(&path).unwrap();
        assert_eq!(track.name.as_deref(), Some("From disk"));
        assert_eq!(track.points.len(), 2);
    }

    #[test]
    fn load_of_a_missing_file_is_an_io_error() {
        let err = load(Path::new("/definitely/not/here.gpx")).unwrap_err();
        assert!(matches!(err, Error::Io(_)), "got {err:?}");
    }
}
