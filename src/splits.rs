//! Cuts an analysed track into per-kilometre splits and finds the fastest one.

use time::OffsetDateTime;

use crate::error::{Error, Result};
use crate::geo;
use crate::model::{Analysis, Split, Track};

const KM: f64 = 1000.0;

/// A kilometre boundary this close (metres) to a point counts as reached there;
/// it absorbs floating-point noise in the summed distances.
const BOUNDARY_TOLERANCE_M: f64 = 1e-3;

/// A trailing remainder at most this long (metres) is GPS noise, not a split.
const MIN_PARTIAL_M: f64 = 1.0;

/// A track at most this long (metres) has not gone anywhere.
const MIN_TRACK_M: f64 = 1.0;

/// Analyses a timed track: totals, per-kilometre splits and the fastest full split.
///
/// Every point needs a timestamp, none earlier than the one before it, and the
/// track must be longer than one metre. Kilometre boundaries that fall between
/// two points are timed by linear interpolation over the distance between them.
pub fn analyze(track: &Track) -> Result<Analysis> {
    if track.points.len() < 2 {
        return Err(Error::TooFewPoints);
    }
    let times = timestamps(track)?;
    let start = times[0];
    let seconds: Vec<f64> = times
        .iter()
        .map(|t| (*t - start).as_seconds_f64())
        .collect();
    let distances = geo::cumulative_m(&track.points);

    let distance_m = distances[distances.len() - 1];
    if distance_m <= MIN_TRACK_M {
        return Err(Error::TooShort);
    }
    let elapsed_s = seconds[seconds.len() - 1];
    let splits = splits(&distances, &seconds, distance_m, elapsed_s);
    Ok(Analysis {
        distance_m,
        elapsed_s,
        fastest: fastest(&splits),
        splits,
        start: Some(start),
        points: track.points.clone(),
    })
}

/// The timestamp of every point, each no earlier than the one before.
fn timestamps(track: &Track) -> Result<Vec<OffsetDateTime>> {
    if track.points.iter().all(|p| p.time.is_none()) {
        return Err(Error::NoTimestamps);
    }
    let times = track
        .points
        .iter()
        .enumerate()
        .map(|(index, p)| p.time.ok_or(Error::MissingTimestamp { index }))
        .collect::<Result<Vec<_>>>()?;
    match times.windows(2).position(|pair| pair[1] < pair[0]) {
        Some(before) => Err(Error::TimeNotMonotonic { index: before + 1 }),
        None => Ok(times),
    }
}

/// Seconds from the start at which each whole kilometre is crossed.
fn crossings(distances: &[f64], seconds: &[f64]) -> Vec<f64> {
    let mut crossings = Vec::new();
    for i in 1..distances.len() {
        let (d0, d1) = (distances[i - 1], distances[i]);
        let (t0, t1) = (seconds[i - 1], seconds[i]);
        loop {
            // Every earlier boundary was reached by d0, so this one lies beyond
            // d0 + tolerance; reaching it by d1 implies d1 > d0.
            let boundary = (crossings.len() + 1) as f64 * KM;
            if boundary > d1 + BOUNDARY_TOLERANCE_M {
                break;
            }
            let fraction = ((boundary - d0) / (d1 - d0)).min(1.0);
            crossings.push(t0 + fraction * (t1 - t0));
        }
    }
    crossings
}

fn splits(distances: &[f64], seconds: &[f64], distance_m: f64, elapsed_s: f64) -> Vec<Split> {
    let crossings = crossings(distances, seconds);
    let starts = std::iter::once(0.0).chain(crossings.iter().copied());
    let mut splits: Vec<Split> = starts
        .zip(&crossings)
        .zip(1..)
        .map(|((from, &to), index)| Split {
            index,
            distance_m: KM,
            seconds: to - from,
            partial: false,
        })
        .collect();
    let remainder = distance_m - crossings.len() as f64 * KM;
    if remainder > MIN_PARTIAL_M {
        splits.push(Split {
            index: crossings.len() as u32 + 1,
            distance_m: remainder,
            seconds: elapsed_s - crossings.last().unwrap_or(&0.0),
            partial: true,
        });
    }
    splits
}

/// Index of the quickest full split; on a tie the earlier one.
fn fastest(splits: &[Split]) -> Option<u32> {
    splits
        .iter()
        .filter(|s| !s.partial)
        .reduce(|best, s| if s.seconds < best.seconds { s } else { best })
        .map(|s| s.index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geo::EARTH_RADIUS_M;
    use crate::model::TrackPoint;
    use time::macros::datetime;
    use time::{Duration, OffsetDateTime};

    const START: OffsetDateTime = datetime!(2026-09-27 09:00:00 UTC);
    const LAT: f64 = 52.5;
    const LON: f64 = 13.4;
    const STEP_M: f64 = 50.0;

    /// Seconds from the start until `distance_m` is reached, given a pace (s/km) per kilometre.
    fn elapsed_at(distance_m: f64, pace_for_km: &impl Fn(u32) -> f64) -> f64 {
        let whole_km = (distance_m / 1000.0).floor() as u32;
        let full: f64 = (1..=whole_km).map(pace_for_km).sum();
        let rest_m = distance_m - f64::from(whole_km) * 1000.0;
        full + rest_m / 1000.0 * pace_for_km(whole_km + 1)
    }

    /// A point `distance_m` east of the start along the parallel at `LAT`.
    fn point_east(distance_m: f64, time: Option<OffsetDateTime>) -> TrackPoint {
        let metres_per_degree = EARTH_RADIUS_M.to_radians();
        TrackPoint {
            lat: LAT,
            lon: LON + distance_m / (metres_per_degree * LAT.to_radians().cos()),
            time,
        }
    }

    /// Points every 50 m due east (plus one at the very end), timed by `pace_for_km`.
    fn straight_track(total_m: f64, pace_for_km: impl Fn(u32) -> f64) -> Track {
        let steps = (total_m / STEP_M).floor() as u32;
        let mut distances: Vec<f64> = (0..=steps).map(|i| f64::from(i) * STEP_M).collect();
        if total_m - distances[distances.len() - 1] > 1e-9 {
            distances.push(total_m);
        }
        let points = distances
            .into_iter()
            .map(|d| {
                let t = START + Duration::seconds_f64(elapsed_at(d, &pace_for_km));
                point_east(d, Some(t))
            })
            .collect();
        Track {
            name: Some("Synthetic".into()),
            points,
        }
    }

    fn full_splits(analysis: &Analysis) -> Vec<&Split> {
        analysis.splits.iter().filter(|s| !s.partial).collect()
    }

    #[test]
    fn constant_pace_marathon_has_42_even_splits_and_a_partial() {
        let analysis = analyze(&straight_track(42_195.0, |_| 300.0)).unwrap();

        let full = full_splits(&analysis);
        assert_eq!(full.len(), 42);
        for (i, split) in full.iter().enumerate() {
            assert_eq!(split.index, i as u32 + 1);
            assert_eq!(split.distance_m, 1000.0);
            assert!((split.seconds - 300.0).abs() < 0.5, "{split:?}");
        }

        let last = analysis.splits.last().unwrap();
        assert_eq!(analysis.splits.len(), 43);
        assert!(last.partial);
        assert_eq!(last.index, 43);
        assert!((last.distance_m - 195.0).abs() < 1.0, "{last:?}");
        assert!((last.seconds - 58.5).abs() < 0.5, "{last:?}");

        assert_eq!(
            analysis.fastest,
            Some(1),
            "even splits tie, the earliest wins"
        );
    }

    #[test]
    fn totals_come_from_the_last_point() {
        let track = straight_track(42_195.0, |_| 300.0);
        let analysis = analyze(&track).unwrap();
        assert!(
            (analysis.distance_m - 42_195.0).abs() < 1.0,
            "{}",
            analysis.distance_m
        );
        assert!(
            (analysis.elapsed_s - 42.195 * 300.0).abs() < 1e-3,
            "{}",
            analysis.elapsed_s
        );
        assert_eq!(analysis.start, Some(START));
        assert_eq!(analysis.points, track.points);
    }

    #[test]
    fn the_one_fast_kilometre_is_the_fastest() {
        let pace = |km| if km == 30 { 270.0 } else { 330.0 };
        let analysis = analyze(&straight_track(42_195.0, pace)).unwrap();
        assert_eq!(analysis.fastest, Some(30));
        let km30 = &analysis.splits[29];
        assert_eq!(km30.index, 30);
        assert!((km30.seconds - 270.0).abs() < 0.5, "{km30:?}");
    }

    #[test]
    fn a_run_under_a_kilometre_is_one_partial_split() {
        let analysis = analyze(&straight_track(600.0, |_| 300.0)).unwrap();
        assert_eq!(analysis.splits.len(), 1);
        let only = &analysis.splits[0];
        assert_eq!(only.index, 1);
        assert!(only.partial);
        assert!((only.distance_m - 600.0).abs() < 0.5, "{only:?}");
        assert!((only.seconds - 180.0).abs() < 1e-3, "{only:?}");
        assert_eq!(analysis.fastest, None);
    }

    #[test]
    fn crossings_are_interpolated_between_points() {
        let track = Track {
            name: None,
            points: vec![
                point_east(0.0, Some(START)),
                point_east(1500.0, Some(START + Duration::seconds(450))),
            ],
        };
        let analysis = analyze(&track).unwrap();
        assert_eq!(analysis.splits.len(), 2);
        let (first, rest) = (&analysis.splits[0], &analysis.splits[1]);
        assert_eq!((first.index, first.partial), (1, false));
        assert!((first.seconds - 300.0).abs() < 0.01, "{first:?}");
        assert_eq!((rest.index, rest.partial), (2, true));
        assert!((rest.distance_m - 500.0).abs() < 0.01, "{rest:?}");
        assert!((rest.seconds - 150.0).abs() < 0.01, "{rest:?}");
        assert_eq!(analysis.fastest, Some(1));
    }

    #[test]
    fn a_whole_number_of_kilometres_has_no_partial() {
        let analysis = analyze(&straight_track(3000.0, |_| 300.0)).unwrap();
        assert_eq!(analysis.splits.len(), 3);
        assert!(analysis.splits.iter().all(|s| !s.partial));
    }

    /// Points due east at (metres from the start, seconds from the start).
    fn timed(points: &[(f64, f64)]) -> Track {
        Track {
            name: None,
            points: points
                .iter()
                .map(|&(m, s)| point_east(m, Some(START + Duration::seconds_f64(s))))
                .collect(),
        }
    }

    /// (index, distance, seconds, partial) of every split.
    fn summary(analysis: &Analysis) -> Vec<(u32, f64, f64, bool)> {
        analysis
            .splits
            .iter()
            .map(|s| (s.index, s.distance_m, s.seconds, s.partial))
            .collect()
    }

    fn assert_splits(analysis: &Analysis, expected: &[(u32, f64, f64, bool)]) {
        let actual = summary(analysis);
        let matches = actual.len() == expected.len()
            && actual.iter().zip(expected).all(|(a, e)| {
                a.0 == e.0 && (a.1 - e.1).abs() < 0.01 && (a.2 - e.2).abs() < 0.01 && a.3 == e.3
            });
        assert!(matches, "splits {actual:?}, expected {expected:?}");
    }

    #[test]
    fn a_missing_timestamp_names_its_point() {
        let mut track = straight_track(2000.0, |_| 300.0);
        track.points[7].time = None;
        assert!(
            matches!(analyze(&track), Err(Error::MissingTimestamp { index: 7 })),
            "{:?}",
            analyze(&track)
        );
    }

    #[test]
    fn a_missing_first_timestamp_names_point_0() {
        let mut track = straight_track(2000.0, |_| 300.0);
        track.points[0].time = None;
        assert!(
            matches!(analyze(&track), Err(Error::MissingTimestamp { index: 0 })),
            "{:?}",
            analyze(&track)
        );
    }

    #[test]
    fn a_track_without_any_timestamp_has_no_timestamps() {
        let mut track = straight_track(2000.0, |_| 300.0);
        track.points.iter_mut().for_each(|p| p.time = None);
        assert!(matches!(analyze(&track), Err(Error::NoTimestamps)));
    }

    #[test]
    fn a_timestamp_earlier_than_the_one_before_is_an_error() {
        let track = timed(&[
            (0.0, 0.0),
            (1000.0, 300.0),
            (2000.0, 200.0),
            (2500.0, 350.0),
        ]);
        assert!(
            matches!(analyze(&track), Err(Error::TimeNotMonotonic { index: 2 })),
            "{:?}",
            analyze(&track)
        );
    }

    #[test]
    fn a_track_that_never_moves_is_too_short() {
        let track = timed(&[(0.0, 0.0), (0.0, 10.0), (0.0, 20.0)]);
        assert!(
            matches!(analyze(&track), Err(Error::TooShort)),
            "{:?}",
            analyze(&track)
        );
    }

    #[test]
    fn a_track_under_one_metre_is_too_short() {
        let track = timed(&[(0.0, 0.0), (0.9, 5.0)]);
        assert!(
            matches!(analyze(&track), Err(Error::TooShort)),
            "{:?}",
            analyze(&track)
        );
    }

    #[test]
    fn one_segment_can_cross_several_kilometres() {
        let analysis = analyze(&timed(&[(0.0, 0.0), (3500.0, 1050.0)])).unwrap();
        assert_splits(
            &analysis,
            &[
                (1, 1000.0, 300.0, false),
                (2, 1000.0, 300.0, false),
                (3, 1000.0, 300.0, false),
                (4, 500.0, 150.0, true),
            ],
        );
    }

    #[test]
    fn standing_still_on_a_kilometre_boundary_adds_no_nan() {
        let track = timed(&[
            (0.0, 0.0),
            (500.0, 150.0),
            (500.0, 160.0),
            (1000.0, 310.0),
            (1000.0, 320.0),
            (1500.0, 470.0),
        ]);
        let analysis = analyze(&track).unwrap();
        assert_splits(
            &analysis,
            &[(1, 1000.0, 310.0, false), (2, 500.0, 160.0, true)],
        );
    }

    #[test]
    fn two_points_with_the_same_timestamp_add_no_nan() {
        let track = timed(&[(0.0, 0.0), (900.0, 270.0), (1100.0, 270.0), (1500.0, 390.0)]);
        let analysis = analyze(&track).unwrap();
        assert_splits(
            &analysis,
            &[(1, 1000.0, 270.0, false), (2, 500.0, 120.0, true)],
        );
    }

    #[test]
    fn a_remainder_over_one_metre_is_a_partial_split() {
        let track = timed(&[(0.0, 0.0), (1000.0, 300.0), (1001.5, 301.0)]);
        let analysis = analyze(&track).unwrap();
        assert_splits(&analysis, &[(1, 1000.0, 300.0, false), (2, 1.5, 1.0, true)]);
    }

    #[test]
    fn a_remainder_under_one_metre_is_gps_noise() {
        let track = timed(&[(0.0, 0.0), (1000.0, 300.0), (1000.5, 301.0)]);
        let analysis = analyze(&track).unwrap();
        assert_splits(&analysis, &[(1, 1000.0, 300.0, false)]);
    }

    #[test]
    fn a_single_point_is_too_few() {
        let track = Track {
            name: None,
            points: vec![point_east(0.0, Some(START))],
        };
        assert!(matches!(analyze(&track), Err(Error::TooFewPoints)));
    }
}
