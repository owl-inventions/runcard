//! Geodesic helpers: great-circle distance between track points.

use crate::model::TrackPoint;

/// Mean Earth radius (IUGG), in metres.
pub const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// Great-circle distance between two points in metres (haversine formula).
pub fn haversine_m(a: &TrackPoint, b: &TrackPoint) -> f64 {
    let (lat_a, lat_b) = (a.lat.to_radians(), b.lat.to_radians());
    let half_dlat = (lat_b - lat_a) / 2.0;
    let half_dlon = (b.lon - a.lon).to_radians() / 2.0;
    let h = half_dlat.sin().powi(2) + lat_a.cos() * lat_b.cos() * half_dlon.sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().asin()
}

/// Running distance from the first point, one entry per point, starting at 0.0.
pub fn cumulative_m(points: &[TrackPoint]) -> Vec<f64> {
    if points.is_empty() {
        return Vec::new();
    }
    let running = points.windows(2).scan(0.0, |total, pair| {
        *total += haversine_m(&pair[0], &pair[1]);
        Some(*total)
    });
    std::iter::once(0.0).chain(running).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(lat: f64, lon: f64) -> TrackPoint {
        TrackPoint {
            lat,
            lon,
            time: None,
        }
    }

    const BRANDENBURGER_TOR: (f64, f64) = (52.516275, 13.377704);
    const FERNSEHTURM: (f64, f64) = (52.520817, 13.409419);

    #[test]
    fn brandenburger_tor_to_fernsehturm_is_about_2_2_km() {
        let a = point(BRANDENBURGER_TOR.0, BRANDENBURGER_TOR.1);
        let b = point(FERNSEHTURM.0, FERNSEHTURM.1);
        let d = haversine_m(&a, &b);
        assert!((d - 2200.0).abs() / 2200.0 < 0.01, "distance was {d}");
    }

    #[test]
    fn distance_is_symmetric() {
        let a = point(BRANDENBURGER_TOR.0, BRANDENBURGER_TOR.1);
        let b = point(FERNSEHTURM.0, FERNSEHTURM.1);
        assert_eq!(haversine_m(&a, &b), haversine_m(&b, &a));
    }

    #[test]
    fn same_point_is_zero() {
        let a = point(BRANDENBURGER_TOR.0, BRANDENBURGER_TOR.1);
        assert_eq!(haversine_m(&a, &a), 0.0);
    }

    #[test]
    fn cumulative_starts_at_zero_and_is_monotonic() {
        let points: Vec<_> = (0..20)
            .map(|i| {
                point(
                    52.5 + f64::from(i) * 0.0003,
                    13.4 + f64::from(i % 3) * 0.0002,
                )
            })
            .collect();
        let cumulative = cumulative_m(&points);
        assert_eq!(cumulative.len(), points.len());
        assert_eq!(cumulative[0], 0.0);
        assert!(
            cumulative.windows(2).all(|w| w[1] >= w[0]),
            "{cumulative:?}"
        );
        assert!(cumulative[19] > 0.0);
    }

    #[test]
    fn cumulative_of_two_points_is_their_distance() {
        let a = point(BRANDENBURGER_TOR.0, BRANDENBURGER_TOR.1);
        let b = point(FERNSEHTURM.0, FERNSEHTURM.1);
        assert_eq!(
            cumulative_m(&[a.clone(), b.clone()]),
            vec![0.0, haversine_m(&a, &b)]
        );
    }

    #[test]
    fn cumulative_of_nothing_is_empty() {
        assert!(cumulative_m(&[]).is_empty());
    }
}
