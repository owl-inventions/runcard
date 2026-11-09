//! Human-readable formatting of distances, durations, paces and dates for the card.

use time::Date;

/// Separator between parts of a caps line: a middle dot with spaces.
const DOT: &str = " \u{b7} ";

/// Distance in kilometres with three decimals: `42.195 km`.
pub fn distance(m: f64) -> String {
    format!("{:.3} km", m / 1000.0)
}

/// Duration rounded to whole seconds: `3:58:41`, `59:59` or `0:05`.
pub fn duration(s: f64) -> String {
    let total = whole_seconds(s);
    let hours = total / 3600;
    if hours > 0 {
        format!("{hours}:{:02}:{:02}", total % 3600 / 60, total % 60)
    } else {
        minutes_seconds(total)
    }
}

/// Pace rounded to whole seconds: `5:39 /km`.
pub fn pace(s_per_km: f64) -> String {
    format!("{} /km", pace_short(s_per_km))
}

/// Pace rounded to whole seconds, without the unit: `5:39`.
pub fn pace_short(s_per_km: f64) -> String {
    minutes_seconds(whole_seconds(s_per_km))
}

/// Pace in capitals for the fastest-split line: `5:16 /KM`.
pub fn pace_caps(s_per_km: f64) -> String {
    pace(s_per_km).to_uppercase()
}

/// Date in capitals: `SEPTEMBER 27, 2026`.
pub fn date(d: Date) -> String {
    let month = d.month().to_string().to_uppercase();
    format!("{month} {}, {}", d.day(), d.year())
}

/// Card header: `RUN · BERLIN`, or `RUN` without a city.
pub fn header(city: Option<&str>) -> String {
    match city.map(str::trim).filter(|c| !c.is_empty()) {
        Some(city) => format!("RUN{DOT}{}", city.to_uppercase()),
        None => "RUN".to_owned(),
    }
}

/// Fastest split line: `FASTEST KM 30 · 5:16 /KM`.
pub fn fastest_line(index: u32, s_per_km: f64) -> String {
    format!("FASTEST KM {index}{DOT}{}", pace_caps(s_per_km))
}

fn whole_seconds(s: f64) -> u64 {
    s.round() as u64
}

/// `m:ss`, minutes unpadded.
fn minutes_seconds(total: u64) -> String {
    format!("{}:{:02}", total / 60, total % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    #[test]
    fn distance_has_three_decimals() {
        assert_eq!(distance(42_195.0), "42.195 km");
        assert_eq!(distance(10_000.0), "10.000 km");
        assert_eq!(distance(42_194.7), "42.195 km");
    }

    #[test]
    fn duration_over_an_hour_shows_hours() {
        assert_eq!(duration(3.0 * 3600.0 + 58.0 * 60.0 + 41.0), "3:58:41");
        assert_eq!(duration(3600.0), "1:00:00");
    }

    #[test]
    fn duration_under_an_hour_shows_minutes_and_seconds() {
        assert_eq!(duration(59.0 * 60.0 + 59.0), "59:59");
        assert_eq!(duration(5.0), "0:05");
    }

    #[test]
    fn duration_rounds_to_whole_seconds() {
        assert_eq!(duration(4.4), "0:04");
        assert_eq!(duration(3599.6), "1:00:00");
    }

    #[test]
    fn pace_is_minutes_and_seconds_per_km() {
        assert_eq!(pace(5.0 * 60.0 + 39.0), "5:39 /km");
        assert_eq!(pace(339.4), "5:39 /km");
        assert_eq!(pace(359.6), "6:00 /km");
    }

    #[test]
    fn pace_short_is_the_pace_without_the_unit() {
        assert_eq!(pace_short(302.0), "5:02");
        assert_eq!(pace_short(359.6), "6:00");
    }

    #[test]
    fn pace_caps_shouts_the_unit() {
        assert_eq!(pace_caps(316.0), "5:16 /KM");
    }

    #[test]
    fn date_is_spelled_out_in_capitals() {
        assert_eq!(date(date!(2026 - 09 - 27)), "SEPTEMBER 27, 2026");
        assert_eq!(date(date!(2026 - 05 - 03)), "MAY 3, 2026");
    }

    #[test]
    fn header_names_the_city_in_capitals() {
        assert_eq!(header(Some("Berlin")), "RUN \u{b7} BERLIN");
    }

    #[test]
    fn header_without_a_city_is_just_run() {
        assert_eq!(header(None), "RUN");
        assert_eq!(header(Some("")), "RUN");
        assert_eq!(header(Some("  ")), "RUN");
    }

    #[test]
    fn fastest_line_names_the_kilometre_and_its_pace() {
        assert_eq!(fastest_line(30, 316.0), "FASTEST KM 30 \u{b7} 5:16 /KM");
    }
}
