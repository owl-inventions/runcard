//! Builds the template context (`CardData`) from an `Analysis` and the run `Meta`.

use crate::format;
use crate::layout;
use crate::model::{Analysis, CardData, Meta, Split, SplitView, Stripe};
use crate::palette::{self, Palette};
use crate::route::{self, BBox};
use crate::text::{self, Font, TextMeasure};
use crate::variant::Variant;

/// Left edge of the stripe bar in card pixels.
pub(crate) const STRIP_LEFT: f32 = 32.0;
/// Width of the stripe bar (x = 32 to 688) in card pixels.
pub(crate) const STRIP_WIDTH: f32 = 656.0;
/// Gap between neighbouring stripes in card pixels.
pub(crate) const STRIPE_GAP: f32 = 2.0;
/// Every kilometre divisible by this gets a tick under the bar.
pub(crate) const TICK_EVERY_KM: u32 = 5;
/// The box right of the runner's name that the route outline is fitted into.
pub(crate) const ROUTE_BOX: BBox = BBox {
    x: 564.0,
    y: 628.0,
    w: 120.0,
    h: 90.0,
};
/// Point budget of the simplified route outline.
const ROUTE_MAX_POINTS: usize = 150;
/// Number of swatches in the slow-to-fast legend.
const LEGEND_SWATCHES: usize = 5;
/// Least room between the runner's name and the route box, in card pixels.
const ROUTE_MARGIN: f32 = 16.0;
/// The runner's name: Bebas Neue in capitals from x = 32, ending at least
/// [`ROUTE_MARGIN`] before the route box.
pub(crate) const RUNNER_LINE: Line = Line {
    font: Font::Condensed,
    size: 76.0,
    min_size: 40.0,
    max_width: ROUTE_BOX.x - ROUTE_MARGIN - STRIP_LEFT,
    uppercase: true,
};
/// The event title: Fraunces across the width of the stripe bar.
const TITLE_LINE: Line = Line {
    font: Font::Serif,
    size: 48.0,
    min_size: 28.0,
    max_width: STRIP_WIDTH,
    uppercase: false,
};
/// Header and date: IBM Plex Mono at 13 px with 3 px letter spacing.
const CAPS_SIZE: f32 = 13.0;
const CAPS_LETTER_SPACING: f32 = 3.0;
/// Least room between the header and the right-aligned date, in card pixels.
const HEADER_DATE_GAP: f32 = 24.0;

/// Lays out and formats everything the template of `variant` shows, with
/// `palette` as its colour scale.
///
/// A long runner's name or title is set smaller, down to a minimum size, and
/// cut with an ellipsis if it does not fit even then; a long city is cut so
/// the header does not run into the date. Only the shape block of `variant`
/// is built, if it has one.
pub fn build(
    analysis: &Analysis,
    meta: &Meta,
    palette: &Palette,
    measure: &TextMeasure,
    variant: Variant,
) -> CardData {
    let splits = &analysis.splits;
    let spans = spans(splits);
    let (slow, fast) = anchors(splits);
    let views = split_views(splits, slow, fast, palette);
    let average = analysis.elapsed_s / analysis.distance_m * 1000.0;
    let fastest = analysis
        .fastest
        .and_then(|index| splits.iter().position(|s| s.index == index));
    let route = route::outline(&analysis.points, ROUTE_BOX, ROUTE_MAX_POINTS);
    let route_start = route.first().copied().unwrap_or((
        ROUTE_BOX.x + ROUTE_BOX.w / 2.0,
        ROUTE_BOX.y + ROUTE_BOX.h / 2.0,
    ));
    let date = format::date(meta.date);
    let (title, title_size) = TITLE_LINE.fit(measure, &meta.event);
    let (runner, runner_size) = RUNNER_LINE.fit(measure, &meta.runner);
    CardData {
        header: header(meta.city.as_deref(), &date, measure),
        date,
        title,
        title_size,
        runner,
        runner_size,
        stripes: stripes(&views, &spans),
        fastest_x: fastest.map(|i| spans[i].centre()),
        ticks: ticks(splits, &spans),
        route,
        route_start,
        legend: palette.legend(LEGEND_SWATCHES),
        fastest_line: fastest
            .map(|i| format::fastest_line(splits[i].index, splits[i].pace_s_per_km()))
            .unwrap_or_default(),
        distance: format::distance(analysis.distance_m),
        time: format::duration(analysis.elapsed_s),
        pace: format::pace(average),
        variant: variant.name(),
        theme: variant.theme(),
        avg_t: palette::pace_position(average, slow, fast),
        avg_pace: format::pace_short(average),
        rings: (variant == Variant::Rings)
            .then(|| layout::rings::build(analysis, &views, palette, measure, meta)),
        route_hero: (variant == Variant::Route)
            .then(|| layout::route_hero::build(analysis, &views, palette, measure, meta)),
        bars: (variant == Variant::Bars)
            .then(|| layout::bars::build(analysis, &views, palette, measure, meta)),
        mosaic: (variant == Variant::Mosaic)
            .then(|| layout::mosaic::build(analysis, &views, palette, measure, meta)),
        splits: views,
    }
}

/// A line of text that is shrunk, then cut, to stay within its room.
pub(crate) struct Line {
    pub(crate) font: Font,
    /// The size the card is designed for, in px.
    pub(crate) size: f32,
    /// The smallest size; a text too wide even at this size gets cut.
    pub(crate) min_size: f32,
    /// Room from the start of the text to where it must end, in px.
    pub(crate) max_width: f32,
    /// The template shows the text in capitals (`| upper`).
    pub(crate) uppercase: bool,
}

impl Line {
    fn fits(&self, measure: &TextMeasure, text: &str, size: f32) -> bool {
        let width = if self.uppercase {
            measure.width(self.font, &text.to_uppercase(), size, 0.0)
        } else {
            measure.width(self.font, text, size, 0.0)
        };
        width <= self.max_width
    }

    /// `text` and the largest size, in steps of 0.1 px, at which it fits; at
    /// the minimum size and cut with an ellipsis if it does not fit even then.
    pub(crate) fn fit(&self, measure: &TextMeasure, text: &str) -> (String, f32) {
        if self.fits(measure, text, self.size) {
            return (text.to_owned(), self.size);
        }
        if !self.fits(measure, text, self.min_size) {
            let cut = text::truncate_to_fit(text, |t| self.fits(measure, t, self.min_size));
            return (cut, self.min_size);
        }
        // Bisect in tenths of a pixel: `fitting` fits, `too_large` does not.
        let tenths = |px: f32| (px * 10.0).round() as u32;
        let (mut fitting, mut too_large) = (tenths(self.min_size), tenths(self.size));
        while too_large - fitting > 1 {
            let mid = (fitting + too_large) / 2;
            if self.fits(measure, text, mid as f32 / 10.0) {
                fitting = mid;
            } else {
                too_large = mid;
            }
        }
        (text.to_owned(), fitting as f32 / 10.0)
    }
}

/// `RUN · CITY`, the city cut with an ellipsis so the header ends at least
/// [`HEADER_DATE_GAP`] before the right-aligned `date`.
fn header(city: Option<&str>, date: &str, measure: &TextMeasure) -> String {
    let caps_width = |text: &str| measure.width(Font::Mono, text, CAPS_SIZE, CAPS_LETTER_SPACING);
    let room = STRIP_WIDTH - caps_width(date) - HEADER_DATE_GAP;
    let city = city.map(|city| {
        text::truncate_to_fit(city.trim(), |city| {
            caps_width(&format::header(Some(city))) <= room
        })
    });
    format::header(city.as_deref())
}

/// Horizontal extent of one stripe in card pixels.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Span {
    pub(crate) x: f32,
    pub(crate) width: f32,
}

impl Span {
    pub(crate) fn centre(self) -> f32 {
        self.x + self.width / 2.0
    }
}

/// One span per split, left to right from the strip's left edge with a gap
/// between neighbours. Full kilometres share the strip evenly; the trailing
/// partial gets its share of a full width, so the bar then ends short of 688.
pub(crate) fn spans(splits: &[Split]) -> Vec<Span> {
    let n = splits.len() as f32;
    let full_width = (STRIP_WIDTH - STRIPE_GAP * (n - 1.0)) / n;
    let mut x = STRIP_LEFT;
    splits
        .iter()
        .map(|split| {
            let width = if split.partial {
                full_width * (split.distance_m / 1000.0) as f32
            } else {
                full_width
            };
            let span = Span { x, width };
            x += width + STRIPE_GAP;
            span
        })
        .collect()
}

/// The colour scale anchors (slow, fast) of the full kilometres; a trailing
/// partial does not move them.
fn anchors(splits: &[Split]) -> (f64, f64) {
    let full_paces: Vec<f64> = splits
        .iter()
        .filter(|s| !s.partial)
        .map(Split::pace_s_per_km)
        .collect();
    palette::anchors(&full_paces)
}

/// One view per split, coloured by its pace between the `slow` and `fast`
/// anchors; the partial is coloured by its pace too.
fn split_views(splits: &[Split], slow: f64, fast: f64, palette: &Palette) -> Vec<SplitView> {
    splits
        .iter()
        .map(|split| {
            let pace = split.pace_s_per_km();
            let t = palette::pace_position(pace, slow, fast);
            SplitView {
                index: split.index,
                pace: format::pace_short(pace),
                color: palette.hex(t),
                partial: split.partial,
                fraction: if split.partial {
                    (split.distance_m / 1000.0) as f32
                } else {
                    1.0
                },
                t,
            }
        })
        .collect()
}

/// One stripe per split, as wide as its span and in the colour of its view.
fn stripes(views: &[SplitView], spans: &[Span]) -> Vec<Stripe> {
    views
        .iter()
        .zip(spans)
        .map(|(view, span)| Stripe {
            width: span.width,
            color: view.color.clone(),
        })
        .collect()
}

/// A tick at the centre of every full kilometre divisible by [`TICK_EVERY_KM`].
fn ticks(splits: &[Split], spans: &[Span]) -> Vec<(f32, u32)> {
    splits
        .iter()
        .zip(spans)
        .filter(|(split, _)| !split.partial && split.index % TICK_EVERY_KM == 0)
        .map(|(split, span)| (span.centre(), split.index))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::TrackPoint;
    use crate::palette::{anchors, pace_position};
    use crate::variant::Variant;
    use time::macros::{date, datetime};

    /// Full stripe width of a 43-split run: 656 px minus 42 gaps of 2 px, shared.
    const MARATHON_STRIPE: f32 = (656.0 - 2.0 * 42.0) / 43.0;

    fn split(index: u32, distance_m: f64, seconds: f64) -> Split {
        Split {
            index,
            distance_m,
            seconds,
            partial: distance_m < 1000.0,
        }
    }

    /// A small loop south of the Tiergarten, closed at its start.
    fn route_points() -> Vec<TrackPoint> {
        [
            (52.51, 13.37),
            (52.51, 13.39),
            (52.52, 13.39),
            (52.52, 13.37),
            (52.51, 13.37),
        ]
        .into_iter()
        .map(|(lat, lon)| TrackPoint {
            lat,
            lon,
            time: None,
        })
        .collect()
    }

    fn analysis(splits: Vec<Split>, fastest: Option<u32>) -> Analysis {
        Analysis {
            distance_m: splits.iter().map(|s| s.distance_m).sum(),
            elapsed_s: splits.iter().map(|s| s.seconds).sum(),
            fastest,
            start: Some(datetime!(2026-09-27 09:00 UTC)),
            points: route_points(),
            splits,
        }
    }

    /// 42 kilometres between 5:30 and 5:54, km 30 in 4:30, then 195 m at 5:20.
    fn marathon() -> Analysis {
        let pace = |km: u32| {
            if km == 30 {
                270.0
            } else {
                330.0 + f64::from(km % 7) * 4.0
            }
        };
        let mut splits: Vec<Split> = (1..=42).map(|km| split(km, 1000.0, pace(km))).collect();
        splits.push(split(43, 195.0, 0.195 * 320.0));
        analysis(splits, Some(30))
    }

    /// 600 m at 5:00: a single partial split, no full kilometre.
    fn short_run() -> Analysis {
        analysis(vec![split(1, 600.0, 180.0)], None)
    }

    fn meta() -> Meta {
        Meta {
            runner: "Julia Brandt".into(),
            event: "Berlin Marathon".into(),
            city: Some("Berlin".into()),
            date: date!(2026 - 09 - 27),
        }
    }

    fn card(analysis: &Analysis) -> CardData {
        card_as(analysis, Variant::Classic)
    }

    fn card_as(analysis: &Analysis, variant: Variant) -> CardData {
        build(
            analysis,
            &meta(),
            &Palette::light(),
            &TextMeasure::new(),
            variant,
        )
    }

    /// The marathon card for `meta`.
    fn card_for(meta: Meta) -> CardData {
        build(
            &marathon(),
            &meta,
            &Palette::light(),
            &TextMeasure::new(),
            Variant::Classic,
        )
    }

    fn assert_near(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "{what} was {actual}, expected {expected}"
        );
    }

    #[test]
    fn every_split_gets_a_stripe() {
        assert_eq!(card(&marathon()).stripes.len(), 43);
    }

    #[test]
    fn the_full_kilometres_share_the_strip_evenly() {
        let stripes = card(&marathon()).stripes;
        let full = &stripes[..42];
        for stripe in full {
            assert_near(stripe.width, MARATHON_STRIPE, "full stripe width");
        }
        assert_near(
            full.iter().map(|s| s.width).sum::<f32>() / (656.0 - 42.0 * 2.0),
            42.0 / 43.0,
            "fraction of the strip covered by full stripes",
        );
        let total: f32 = stripes.iter().map(|s| s.width).sum::<f32>() + 42.0 * 2.0;
        assert!(total <= 656.0, "the strip is {total} px wide");
    }

    #[test]
    fn the_partial_kilometre_is_narrowed_to_its_distance() {
        let stripes = card(&marathon()).stripes;
        assert_near(
            stripes[42].width,
            0.195 * MARATHON_STRIPE,
            "partial stripe width",
        );
    }

    #[test]
    fn a_run_under_a_kilometre_is_one_narrowed_stripe() {
        let stripes = card(&short_run()).stripes;
        assert_eq!(stripes.len(), 1);
        assert_near(stripes[0].width, 0.6 * 656.0, "single partial stripe width");
    }

    /// Left and right edge of every stripe, laid out like the template does.
    fn extents(stripes: &[Stripe]) -> Vec<(f32, f32)> {
        let mut x = 32.0;
        stripes
            .iter()
            .map(|s| {
                let extent = (x, x + s.width);
                x += s.width + 2.0;
                extent
            })
            .collect()
    }

    #[test]
    fn the_fastest_kilometre_gets_the_fastest_colour_and_the_slowest_the_slowest() {
        let palette = Palette::light();
        let stripes = card(&marathon()).stripes;
        assert_eq!(stripes[29].color, palette.hex(1.0), "km 30");
        assert_eq!(stripes[5].color, palette.hex(0.0), "km 6 at 5:54");
    }

    #[test]
    fn a_few_metres_of_trailing_partial_do_not_move_the_anchors() {
        // 3 m in 0.6 s is 200 000 s/km: as an anchor it would leave no room
        // between 5:00 and 5:30.
        let analysis = analysis(
            vec![
                split(1, 1000.0, 300.0),
                split(2, 1000.0, 330.0),
                split(3, 3.0, 600.0),
            ],
            Some(1),
        );
        let palette = Palette::light();
        let stripes = card(&analysis).stripes;
        assert_eq!(stripes[0].color, palette.hex(1.0), "km 1 at 5:00");
        assert_eq!(stripes[1].color, palette.hex(0.0), "km 2 at 5:30");
        assert_eq!(stripes[2].color, palette.hex(0.0), "the last 3 m");
    }

    #[test]
    fn every_stripe_is_coloured_by_its_pace_between_the_full_kilometre_anchors() {
        let palette = Palette::light();
        let analysis = marathon();
        let full: Vec<f64> = analysis.splits[..42]
            .iter()
            .map(Split::pace_s_per_km)
            .collect();
        let (slow, fast) = anchors(&full);
        let stripes = card(&analysis).stripes;
        for (split, stripe) in analysis.splits.iter().zip(&stripes) {
            let expected = palette.hex(pace_position(split.pace_s_per_km(), slow, fast));
            assert_eq!(stripe.color, expected, "km {}", split.index);
        }
    }

    #[test]
    fn the_fastest_marker_sits_inside_the_fastest_stripe() {
        let card = card(&marathon());
        let (left, right) = extents(&card.stripes)[29];
        let x = card.fastest_x.expect("a fastest marker");
        assert!(left < x && x < right, "{x} is not inside {left}..{right}");
        assert_near(x, (left + right) / 2.0, "fastest marker");
    }

    #[test]
    fn ticks_mark_every_fifth_full_kilometre_at_its_stripe_centre() {
        let card = card(&marathon());
        let kms: Vec<u32> = card.ticks.iter().map(|&(_, km)| km).collect();
        assert_eq!(kms, vec![5, 10, 15, 20, 25, 30, 35, 40]);
        let extents = extents(&card.stripes);
        for &(x, km) in &card.ticks {
            let (left, right) = extents[km as usize - 1];
            assert_near(x, (left + right) / 2.0, &format!("tick {km}"));
        }
        assert!(
            card.ticks.windows(2).all(|w| w[0].0 < w[1].0),
            "tick positions do not increase: {:?}",
            card.ticks
        );
    }

    #[test]
    fn a_run_under_a_kilometre_has_no_fastest_marker_and_no_ticks() {
        let card = card(&short_run());
        assert_eq!(card.fastest_x, None);
        assert!(card.ticks.is_empty(), "{:?}", card.ticks);
    }

    #[test]
    fn the_texts_are_formatted_for_the_card() {
        let card = card(&marathon());
        assert_eq!(card.header, "RUN \u{b7} BERLIN");
        assert_eq!(card.date, "SEPTEMBER 27, 2026");
        assert_eq!(card.title, "Berlin Marathon");
        assert_eq!(card.runner, "Julia Brandt");
        assert_eq!(card.fastest_line, "FASTEST KM 30 \u{b7} 4:30 /KM");
        assert_eq!(card.distance, "42.195 km");
        // 14296 s for the full kilometres plus 62.4 s for the last 195 m.
        assert_eq!(card.time, "3:59:18");
        // The average over the whole distance: 14358.4 s / 42.195 km.
        assert_eq!(card.pace, "5:40 /km");
    }

    #[test]
    fn without_a_city_the_header_is_just_run() {
        let meta = Meta {
            city: None,
            ..meta()
        };
        let card = card_for(meta);
        assert_eq!(card.header, "RUN");
    }

    const LONG_RUNNER: &str = "Maximilian Oberhauser-Lindqvist";
    const LONG_EVENT: &str = "Schneider Electric Marathon de Paris";
    const LONG_CITY: &str = "Llanfairpwllgwyngyllgogerychwyrndrobwllllantysiliogogogoch";

    /// Width of the name as the template shows it: in capitals, at its size.
    fn runner_width(card: &CardData) -> f32 {
        TextMeasure::new().width(
            Font::Condensed,
            &card.runner.to_uppercase(),
            card.runner_size,
            0.0,
        )
    }

    fn title_width(card: &CardData) -> f32 {
        TextMeasure::new().width(Font::Serif, &card.title, card.title_size, 0.0)
    }

    fn assert_cut_with_one_ellipsis(cut: &str, original: &str) {
        let kept = cut
            .strip_suffix('\u{2026}')
            .unwrap_or_else(|| panic!("{cut:?} does not end with an ellipsis"));
        assert!(
            !kept.contains('\u{2026}'),
            "{cut:?} has more than one ellipsis"
        );
        assert!(
            original.starts_with(kept),
            "{cut:?} is not a prefix of {original:?}"
        );
        assert_eq!(
            kept,
            kept.trim_end(),
            "{cut:?} keeps a space before the ellipsis"
        );
    }

    #[test]
    fn a_normal_name_and_title_keep_their_full_size() {
        let card = card(&marathon());
        assert_eq!(
            (card.runner.as_str(), card.runner_size),
            ("Julia Brandt", 76.0)
        );
        assert_eq!(
            (card.title.as_str(), card.title_size),
            ("Berlin Marathon", 48.0)
        );
    }

    #[test]
    fn a_long_name_is_shrunk_just_enough_to_end_16_px_before_the_route_box() {
        let card = card_for(Meta {
            runner: LONG_RUNNER.into(),
            ..meta()
        });
        assert_eq!(card.runner, LONG_RUNNER);
        assert!(
            (40.0..76.0).contains(&card.runner_size),
            "size {}",
            card.runner_size
        );
        assert!(runner_width(&card) <= 516.0, "{} px", runner_width(&card));
        let larger = TextMeasure::new().width(
            Font::Condensed,
            &LONG_RUNNER.to_uppercase(),
            card.runner_size + 0.1,
            0.0,
        );
        assert!(larger > 516.0, "{} px would still fit", larger);
    }

    #[test]
    fn an_absurdly_long_name_is_cut_with_an_ellipsis_at_40_px() {
        let runner =
            "Maximilian Alexander Konstantin Oberhauser-Lindqvist zu Hohenzollern-Sigmaringen";
        assert_eq!(runner.chars().count(), 80);
        let card = card_for(Meta {
            runner: runner.into(),
            ..meta()
        });
        assert_eq!(card.runner_size, 40.0);
        assert_cut_with_one_ellipsis(&card.runner, runner);
        let width = runner_width(&card);
        assert!((516.0 - 30.0..=516.0).contains(&width), "{width} px");
    }

    #[test]
    fn a_long_title_is_shrunk_just_enough_to_fit_the_card() {
        let card = card_for(Meta {
            event: LONG_EVENT.into(),
            ..meta()
        });
        assert_eq!(card.title, LONG_EVENT);
        assert!(
            (28.0..48.0).contains(&card.title_size),
            "size {}",
            card.title_size
        );
        assert!(title_width(&card) <= 656.0, "{} px", title_width(&card));
        let larger = TextMeasure::new().width(Font::Serif, LONG_EVENT, card.title_size + 0.1, 0.0);
        assert!(larger > 656.0, "{} px would still fit", larger);
    }

    #[test]
    fn an_absurdly_long_title_is_cut_with_an_ellipsis_at_28_px() {
        let event =
            "Schneider Electric Marathon de Paris, presented by the City of Paris and Friends";
        assert_eq!(event.chars().count(), 80);
        let card = card_for(Meta {
            event: event.into(),
            ..meta()
        });
        assert_eq!(card.title_size, 28.0);
        assert_cut_with_one_ellipsis(&card.title, event);
        let width = title_width(&card);
        assert!((656.0 - 30.0..=656.0).contains(&width), "{width} px");
    }

    #[test]
    fn a_long_city_is_cut_so_the_header_ends_24_px_before_the_date() {
        let card = card_for(Meta {
            city: Some(LONG_CITY.into()),
            ..meta()
        });
        assert_cut_with_one_ellipsis(&card.header, &format::header(Some(LONG_CITY)));
        assert!(
            card.header.starts_with("RUN \u{b7} LLANFAIR"),
            "{}",
            card.header
        );
        let measure = TextMeasure::new();
        let header = measure.width(Font::Mono, &card.header, 13.0, 3.0);
        let date = measure.width(Font::Mono, &card.date, 13.0, 3.0);
        assert!(
            (656.0 - 24.0 - 20.0..=656.0 - 24.0).contains(&(header + date)),
            "header {header} px and date {date} px"
        );
    }

    #[test]
    fn a_run_under_a_kilometre_has_no_fastest_line() {
        assert_eq!(card(&short_run()).fastest_line, "");
    }

    #[test]
    fn the_route_is_fitted_into_the_box_beside_the_name_and_starts_at_its_first_point() {
        let card = card(&marathon());
        assert_eq!(card.route.len(), route_points().len());
        for &(x, y) in &card.route {
            assert!(
                (564.0 - 1e-3..=684.0 + 1e-3).contains(&x)
                    && (628.0 - 1e-3..=718.0 + 1e-3).contains(&y),
                "({x}, {y}) is outside the route box"
            );
        }
        assert_eq!(card.route_start, card.route[0]);
    }

    #[test]
    fn a_long_route_is_simplified_to_150_points() {
        let circle = (0..1000)
            .map(|i| {
                let a = f64::from(i) / 1000.0 * std::f64::consts::TAU;
                TrackPoint {
                    lat: 52.5 + 0.01 * a.sin(),
                    lon: 13.4 + 0.016 * a.cos(),
                    time: None,
                }
            })
            .collect();
        let analysis = Analysis {
            points: circle,
            ..marathon()
        };
        let route = card(&analysis).route;
        assert!((100..=150).contains(&route.len()), "{} points", route.len());
    }

    #[test]
    fn without_points_the_route_start_is_the_centre_of_the_box() {
        let analysis = Analysis {
            points: Vec::new(),
            ..marathon()
        };
        let card = card(&analysis);
        assert!(card.route.is_empty(), "{:?}", card.route);
        assert_eq!(card.route_start, (624.0, 673.0));
    }

    #[test]
    fn the_legend_has_five_swatches_from_slow_to_fast() {
        let legend = card(&marathon()).legend;
        assert_eq!(legend, Palette::light().legend(5));
        assert_eq!(legend.len(), 5);
    }

    #[test]
    fn every_split_gets_a_view_with_its_pace_colour_and_scale_position() {
        let card = card(&marathon());
        assert_eq!(card.splits.len(), 43);
        for (i, (view, stripe)) in card.splits.iter().zip(&card.stripes).enumerate() {
            assert_eq!(view.index, i as u32 + 1);
            assert_eq!(view.color, stripe.color, "km {}", view.index);
            assert_eq!(
                view.color,
                Palette::light().hex(view.t),
                "km {}",
                view.index
            );
        }
        let km30 = &card.splits[29];
        assert_eq!(
            (km30.pace.as_str(), km30.t, km30.partial, km30.fraction),
            ("4:30", 1.0, false, 1.0)
        );
        assert_eq!(card.splits[5].t, 0.0, "km 6 at 5:54");
    }

    #[test]
    fn the_partial_kilometre_view_has_its_fraction_and_its_scaled_pace() {
        let last = card(&marathon()).splits.pop().expect("a last split");
        assert_eq!(last.index, 43);
        assert!(last.partial);
        assert_near(last.fraction, 0.195, "fraction of the last split");
        assert_eq!(last.pace, "5:20");
    }

    #[test]
    fn the_average_pace_is_shown_bare_and_placed_on_the_scale() {
        let analysis = marathon();
        let full: Vec<f64> = analysis.splits[..42]
            .iter()
            .map(Split::pace_s_per_km)
            .collect();
        let (slow, fast) = anchors(&full);
        let average = analysis.elapsed_s / analysis.distance_m * 1000.0;
        let card = card(&analysis);
        assert_eq!(card.avg_pace, "5:40");
        assert_eq!(card.avg_t, pace_position(average, slow, fast));
        assert!((0.0..1.0).contains(&card.avg_t), "{}", card.avg_t);
    }

    #[test]
    fn the_card_names_its_variant_and_carries_its_theme() {
        for variant in Variant::ALL {
            let card = card_as(&marathon(), variant);
            assert_eq!(card.variant, variant.name());
            assert_eq!(card.theme, variant.theme(), "{variant}");
        }
    }

    #[test]
    fn only_the_shape_block_of_the_chosen_variant_is_built() {
        for variant in Variant::ALL {
            let card = card_as(&marathon(), variant);
            let built = [
                ("rings", card.rings.is_some()),
                ("route", card.route_hero.is_some()),
                ("bars", card.bars.is_some()),
                ("mosaic", card.mosaic.is_some()),
            ];
            for (name, is_built) in built {
                assert_eq!(
                    is_built,
                    name == variant.name(),
                    "{name} block for {variant}"
                );
            }
        }
    }

    #[test]
    fn the_classic_fields_are_the_same_for_every_variant() {
        let classic = card(&marathon());
        for variant in Variant::ALL {
            let other = card_as(&marathon(), variant);
            assert_eq!(
                (&other.stripes, &other.ticks, &other.route, &other.legend),
                (
                    &classic.stripes,
                    &classic.ticks,
                    &classic.route,
                    &classic.legend
                ),
                "{variant}"
            );
            assert_eq!(
                (
                    &other.runner,
                    &other.title,
                    &other.header,
                    &other.fastest_line
                ),
                (
                    &classic.runner,
                    &classic.title,
                    &classic.header,
                    &classic.fastest_line
                ),
                "{variant}"
            );
        }
    }
}
