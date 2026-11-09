//! Human-readable formatting of distances, durations, paces and dates for the card.

use time::Date;

/// Separator between the parts of a line: a middle dot with spaces.
pub const DOT: &str = " \u{b7} ";

/// Distance in kilometres with two decimals: `21.46 km`.
pub fn distance(m: f64) -> String {
    format!("{} km", kilometres(m))
}

/// Distance in kilometres with two decimals, without the unit: `21.46`.
pub fn kilometres(m: f64) -> String {
    format!("{:.2}", m / 1000.0)
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

/// Pace rounded to whole seconds: `5:43 /km`.
pub fn pace(s_per_km: f64) -> String {
    format!("{} /km", pace_short(s_per_km))
}

/// Pace rounded to whole seconds, without the unit: `5:43`.
pub fn pace_short(s_per_km: f64) -> String {
    minutes_seconds(whole_seconds(s_per_km))
}

/// Date in English, the day without a leading zero: `8 March 2026`.
pub fn date(d: Date) -> String {
    format!("{} {} {}", d.day(), d.month(), d.year())
}

/// The label of the fastest kilometre: `fastest km 11`.
pub fn fastest_km(index: u32) -> String {
    format!("fastest km {index}")
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
    fn distance_has_two_decimals_and_the_unit() {
        assert_eq!(distance(21_460.0), "21.46 km");
        assert_eq!(distance(10_000.0), "10.00 km");
        assert_eq!(distance(42_195.0), "42.20 km");
        assert_eq!(distance(21_096.4), "21.10 km");
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
        assert_eq!(pace(5.0 * 60.0 + 43.0), "5:43 /km");
        assert_eq!(pace(339.4), "5:39 /km");
        assert_eq!(pace(359.6), "6:00 /km");
    }

    #[test]
    fn pace_short_is_the_pace_without_the_unit() {
        assert_eq!(pace_short(302.0), "5:02");
        assert_eq!(pace_short(359.6), "6:00");
    }

    #[test]
    fn date_is_the_day_the_full_month_and_the_year() {
        assert_eq!(date(date!(2026 - 03 - 08)), "8 March 2026");
        assert_eq!(date(date!(2026 - 04 - 19)), "19 April 2026");
        assert_eq!(date(date!(2026 - 09 - 27)), "27 September 2026");
    }

    #[test]
    fn kilometres_is_the_distance_without_the_unit() {
        assert_eq!(kilometres(21_096.4), "21.10");
        assert_eq!(kilometres(600.0), "0.60");
    }

    #[test]
    fn fastest_km_names_the_kilometre() {
        assert_eq!(fastest_km(11), "fastest km 11");
        assert_eq!(fastest_km(1), "fastest km 1");
    }
}
