//! Builds the template context (`CardData`) from an `Analysis` and the run
//! `Meta`: the shared frame (title block, legend, route preview, footer) and
//! the shape block of the chosen variant.

use crate::format::{self, DOT};
use crate::layout;
use crate::model::{
    Analysis, CardData, Fastest, Measure, Meta, RoutePreview, Split, SplitView, Stats, TitleView,
    TrackPoint,
};
use crate::palette::{self, Palette};
use crate::route::{self, BBox};
use crate::text::{self, Role, TextMeasure};
use crate::theme::Theme;
use crate::variant::Variant;

/// Left edge of the content box inside the 48 px margins.
const LEFT: f32 = 48.0;
/// Width of the content box inside the 48 px margins (x = 48 to 672).
pub const CONTENT_WIDTH: f32 = 624.0;
/// Number of 5 px rects in the 160 px legend bar.
pub const LEGEND_STEPS: usize = 32;
/// The box right of the legend, flush with the content's right edge, that
/// the route preview is fitted into.
const PREVIEW_BOX: BBox = BBox {
    x: 472.0,
    y: 750.0,
    w: 200.0,
    h: 56.0,
};
/// Most points the route preview is drawn with.
const PREVIEW_POINTS: usize = 120;
/// The event title: Display at 76 px, its last word in DisplayItalic,
/// shrunk down to 44 px, then cut.
const TITLE_LINE: Line = Line {
    role: Role::Display,
    size: 76.0,
    min_size: 44.0,
    max_width: CONTENT_WIDTH,
    letter_spacing: 0.0,
};
/// The byline, the runner's name: DisplayItalic at 26 px, cut but never shrunk.
const BYLINE: Line = Line {
    role: Role::DisplayItalic,
    size: 26.0,
    min_size: 26.0,
    max_width: CONTENT_WIDTH,
    letter_spacing: 0.0,
};
/// The line under the byline: Label at 11 px in capitals, tracked by 2.2 px.
const SUBLINE: Line = Line {
    role: Role::Label,
    size: 11.0,
    min_size: 11.0,
    max_width: CONTENT_WIDTH,
    letter_spacing: 2.2,
};
/// The finish time: Numeral at 132 px tracked by -3 px, shrunk down to
/// 80 px to fit the left 424 px, never cut.
const TIME_LINE: Line = Line {
    role: Role::Numeral,
    size: 132.0,
    min_size: 80.0,
    max_width: 424.0,
    letter_spacing: -3.0,
};
/// Size of the stats line: Text, the fastest pace in Strong.
const STATS_SIZE: f32 = 17.0;
/// Room between the distance and the average pace on the stats line.
const STATS_GAP: f32 = 44.0;

/// Lays out and formats everything the frame and the template of `variant`
/// show, in the colours of `theme` with `palette` as its colour scale.
///
/// A long title is set smaller, down to 44 px, and cut with an ellipsis if
/// it does not fit even then; a long name is cut; a long city is cut before
/// the date; a long time is set smaller, never cut. Only the shape block of
/// `variant` is built, and every variant but `trail`, whose picture is the
/// route, gets the route preview.
pub fn build(
    analysis: &Analysis,
    meta: &Meta,
    palette: &Palette,
    measure: &TextMeasure,
    variant: Variant,
    theme: Theme,
) -> CardData {
    let splits = &analysis.splits;
    let (slow, fast) = anchors(splits);
    let views = split_views(splits, slow, fast, palette);
    let average = analysis.elapsed_s / analysis.distance_m * 1000.0;
    let (title, title_size) = title(measure, &meta.event);
    let time = format::duration(analysis.elapsed_s);
    let block = |shown: Variant| variant == shown;
    CardData {
        variant: variant.name(),
        theme: theme.tokens(),
        title,
        title_size,
        runner: BYLINE.fit(measure, &meta.runner).0,
        subline: subline(meta, measure),
        legend: palette.legend(LEGEND_STEPS),
        route: match variant {
            Variant::Trail => None,
            _ => route_preview(&analysis.points),
        },
        time_size: TIME_LINE.shrink(measure, &time),
        time,
        stats: stats(analysis, average, measure),
        avg_t: palette::pace_position(average, slow, fast),
        avg_pace: format::pace_short(average),
        ribbon: block(Variant::Ribbon)
            .then(|| layout::ribbon::build(analysis, &views, palette, measure, meta)),
        spiral: block(Variant::Spiral)
            .then(|| layout::spiral::build(analysis, &views, palette, measure, meta)),
        trail: block(Variant::Trail)
            .then(|| layout::trail::build(analysis, &views, palette, measure, meta)),
        dots: block(Variant::Dots)
            .then(|| layout::dots::build(analysis, &views, palette, measure, meta)),
        wave: block(Variant::Wave)
            .then(|| layout::wave::build(analysis, &views, palette, measure, meta)),
        ledger: block(Variant::Ledger)
            .then(|| layout::ledger::build(analysis, &views, palette, measure, meta)),
        splits: views,
    }
}

/// A line of text that is shrunk, then cut, to stay within its room.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Line {
    pub(crate) role: Role,
    /// The size the card is designed for, in px.
    pub(crate) size: f32,
    /// The smallest size; a text too wide even at this size gets cut. Equal
    /// to `size` for a line that is only cut.
    pub(crate) min_size: f32,
    /// Room from the start of the text to where it must end, in px.
    pub(crate) max_width: f32,
    /// The `letter-spacing` the line is set with, in px.
    pub(crate) letter_spacing: f32,
}

impl Line {
    /// Whether `text` fits at `size`.
    pub(crate) fn fits(&self, measure: &TextMeasure, text: &str, size: f32) -> bool {
        measure.width_spaced(self.role, text, size, self.letter_spacing) <= self.max_width
    }

    /// The largest size from the minimum to the design size, in steps of
    /// 0.1 px, that `fits` accepts; `None` if not even the minimum.
    fn largest_size(&self, fits: impl Fn(f32) -> bool) -> Option<f32> {
        if fits(self.size) {
            return Some(self.size);
        }
        if !fits(self.min_size) {
            return None;
        }
        // Bisect in tenths of a pixel: `fitting` fits, `too_large` does not.
        let tenths = |px: f32| (px * 10.0).round() as u32;
        let (mut fitting, mut too_large) = (tenths(self.min_size), tenths(self.size));
        while too_large - fitting > 1 {
            let mid = (fitting + too_large) / 2;
            if fits(mid as f32 / 10.0) {
                fitting = mid;
            } else {
                too_large = mid;
            }
        }
        Some(fitting as f32 / 10.0)
    }

    /// `text` and the largest size at which it fits; at the minimum size and
    /// cut with an ellipsis if it does not fit even then.
    pub(crate) fn fit(&self, measure: &TextMeasure, text: &str) -> (String, f32) {
        match self.largest_size(|size| self.fits(measure, text, size)) {
            Some(size) => (text.to_owned(), size),
            None => (self.cut(measure, text), self.min_size),
        }
    }

    /// The largest size at which `text` fits, the minimum size if it does
    /// not fit even then: for a text that is never cut.
    pub(crate) fn shrink(&self, measure: &TextMeasure, text: &str) -> f32 {
        self.largest_size(|size| self.fits(measure, text, size))
            .unwrap_or(self.min_size)
    }

    /// `text` cut with an ellipsis to fit at the minimum size.
    fn cut(&self, measure: &TextMeasure, text: &str) -> String {
        text::truncate_to_fit(text, |t| self.fits(measure, t, self.min_size))
    }
}

/// The title split before its last word and its size: both parts shrunk
/// together until they fit; unsplit and cut at the minimum size if they do
/// not fit even then.
fn title(measure: &TextMeasure, event: &str) -> (TitleView, f32) {
    let split = split_title(event);
    let width = |size: f32| {
        measure.width(Role::Display, &split.head, size)
            + measure.width(Role::DisplayItalic, &split.tail, size)
    };
    match TITLE_LINE.largest_size(|size| width(size) <= TITLE_LINE.max_width) {
        Some(size) => (split, size),
        None => (
            TitleView {
                head: TITLE_LINE.cut(measure, event),
                tail: String::new(),
            },
            TITLE_LINE.min_size,
        ),
    }
}

/// The last whitespace-separated word of `event` as the tail, everything
/// before it, spaces included, as the head; a one-word title is all head.
/// Trailing whitespace is dropped.
fn split_title(event: &str) -> TitleView {
    let trimmed = event.trim_end();
    match trimmed.rfind(char::is_whitespace) {
        Some(space) if !trimmed[..space].trim().is_empty() => {
            let tail_start = space + trimmed[space..].chars().next().map_or(1, char::len_utf8);
            TitleView {
                head: trimmed[..tail_start].to_owned(),
                tail: trimmed[tail_start..].to_owned(),
            }
        }
        _ => TitleView {
            head: trimmed.to_owned(),
            tail: String::new(),
        },
    }
}

/// `<CITY> · <DATE>` in capitals, or the date alone without a city. A city
/// too long for the tracked line is cut with an ellipsis so the date stays
/// whole.
fn subline(meta: &Meta, measure: &TextMeasure) -> String {
    let date = format::date(meta.date).to_uppercase();
    let Some(city) = meta
        .city
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    else {
        return date;
    };
    let line = |city: &str| format!("{city}{DOT}{date}");
    let fits = |city: &str| SUBLINE.fits(measure, &line(city), SUBLINE.size);
    line(&text::truncate_to_fit(&city.to_uppercase(), fits))
}

/// The distance and the average pace, the pace 44 px after the distance,
/// and the fastest full kilometre if there is one.
fn stats(analysis: &Analysis, average: f64, measure: &TextMeasure) -> Stats {
    let distance = Measure {
        value: format::kilometres(analysis.distance_m),
        unit: "km".to_owned(),
    };
    let distance_width = measure.width(
        Role::Text,
        &format!("{} {}", distance.value, distance.unit),
        STATS_SIZE,
    );
    let fastest = analysis
        .fastest
        .and_then(|index| analysis.splits.iter().find(|s| s.index == index))
        .map(|split| {
            let pace = format::pace_short(split.pace_s_per_km());
            Fastest {
                label: format::fastest_km(split.index),
                width: measure.width(Role::Strong, &pace, STATS_SIZE),
                pace,
            }
        });
    Stats {
        distance,
        pace: Measure {
            value: format::pace_short(average),
            unit: "/km".to_owned(),
        },
        pace_x: LEFT + distance_width + STATS_GAP,
        fastest,
    }
}

/// The route fitted into [`PREVIEW_BOX`] with its aspect ratio kept,
/// simplified to [`PREVIEW_POINTS`], then moved right until its right-most
/// point sits on the box's right edge; vertically it stays centred. `None`
/// for a track with fewer than two distinct points.
fn route_preview(points: &[TrackPoint]) -> Option<RoutePreview> {
    let outline = route::outline(points, PREVIEW_BOX, PREVIEW_POINTS);
    let &first = outline.first()?;
    if outline.iter().all(|&point| point == first) {
        return None;
    }
    let right = outline.iter().map(|&(x, _)| x).fold(f32::MIN, f32::max);
    let shift = PREVIEW_BOX.x + PREVIEW_BOX.w - right;
    let shifted: Vec<(f32, f32)> = outline.iter().map(|&(x, y)| (x + shift, y)).collect();
    Some(RoutePreview {
        path: route::to_path(&shifted),
        start: shifted[0],
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Measure, TitleView};
    use crate::palette::{anchors, pace_position};
    use time::macros::{date, datetime};

    fn split(index: u32, distance_m: f64, seconds: f64) -> Split {
        Split {
            index,
            distance_m,
            seconds,
            partial: distance_m < 1000.0,
        }
    }

    fn analysis(splits: Vec<Split>, fastest: Option<u32>) -> Analysis {
        Analysis {
            distance_m: splits.iter().map(|s| s.distance_m).sum(),
            elapsed_s: splits.iter().map(|s| s.seconds).sum(),
            fastest,
            start: Some(datetime!(2026-04-19 07:30 UTC)),
            points: vec![
                TrackPoint {
                    lat: 38.70,
                    lon: -9.15,
                    time: None,
                },
                TrackPoint {
                    lat: 38.71,
                    lon: -9.14,
                    time: None,
                },
            ],
            splits,
        }
    }

    /// 21 kilometres at 5:30, km 11 in 5:02 and km 14 in 5:52, then 100 m at 5:00.
    fn half() -> Analysis {
        let pace = |km: u32| match km {
            11 => 302.0,
            14 => 352.0,
            _ => 330.0,
        };
        let mut splits: Vec<Split> = (1..=21).map(|km| split(km, 1000.0, pace(km))).collect();
        splits.push(split(22, 100.0, 30.0));
        analysis(splits, Some(11))
    }

    /// 600 m at 5:00: a single partial split, no full kilometre.
    fn short_run() -> Analysis {
        analysis(vec![split(1, 600.0, 180.0)], None)
    }

    fn meta() -> Meta {
        Meta {
            runner: "Mara Lindqvist".into(),
            event: "Harbour Half".into(),
            city: Some("Lisbon".into()),
            date: date!(2026 - 04 - 19),
        }
    }

    fn palette() -> Palette {
        Palette::for_theme(Theme::Light)
    }

    fn card_with(analysis: &Analysis, meta: &Meta, variant: Variant, theme: Theme) -> CardData {
        build(
            analysis,
            meta,
            &Palette::for_theme(theme),
            &TextMeasure::new(),
            variant,
            theme,
        )
    }

    fn card(analysis: &Analysis) -> CardData {
        card_with(analysis, &meta(), Variant::Ribbon, Theme::Light)
    }

    /// The half-marathon card for `meta`.
    fn card_for(meta: Meta) -> CardData {
        card_with(&half(), &meta, Variant::Ribbon, Theme::Light)
    }

    fn assert_near(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "{what} was {actual}, expected {expected}"
        );
    }

    fn width(role: Role, text: &str, size: f32) -> f32 {
        TextMeasure::new().width(role, text, size)
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

    fn title(head: &str, tail: &str) -> TitleView {
        TitleView {
            head: head.into(),
            tail: tail.into(),
        }
    }

    fn measure(value: &str, unit: &str) -> Measure {
        Measure {
            value: value.into(),
            unit: unit.into(),
        }
    }

    #[test]
    fn the_frame_texts_are_formatted_for_the_card() {
        let card = card(&half());
        assert_eq!(card.title, title("Harbour ", "Half"));
        assert_eq!(card.runner, "Mara Lindqvist");
        assert_eq!(card.subline, "LISBON \u{b7} 19 APRIL 2026");
        // 19 kilometres at 5:30, 5:02, 5:52 and 30 s for the last 100 m.
        assert_eq!(card.time, "1:55:54");
        assert_eq!(card.stats.distance, measure("21.10", "km"));
        // 6954 s over 21.1 km.
        assert_eq!(card.stats.pace, measure("5:30", "/km"));
        let fastest = card.stats.fastest.expect("a fastest kilometre");
        assert_eq!(
            (fastest.label.as_str(), fastest.pace.as_str()),
            ("fastest km 11", "5:02")
        );
    }

    #[test]
    fn a_run_without_a_full_kilometre_has_no_fastest_block() {
        let card = card(&short_run());
        assert_eq!(card.stats.fastest, None);
        assert_eq!(card.stats.distance, measure("0.60", "km"));
        assert_eq!(card.time, "3:00");
    }

    #[test]
    fn the_pace_follows_the_distance_after_a_44_px_gap() {
        let card = card(&half());
        let distance = width(Role::Text, "21.10 km", 17.0);
        assert_near(card.stats.pace_x, 48.0 + distance + 44.0, "pace_x");
    }

    #[test]
    fn the_fastest_block_carries_the_width_of_its_pace_in_strong() {
        let fastest = card(&half()).stats.fastest.expect("a fastest kilometre");
        assert_near(
            fastest.width,
            width(Role::Strong, "5:02", 17.0),
            "fastest.width",
        );
        assert!(
            fastest.width > width(Role::Text, "5:02", 17.0),
            "Strong is not wider than Text"
        );
    }

    #[test]
    fn without_a_city_the_subline_is_just_the_date_in_capitals() {
        for city in [None, Some(String::new()), Some("  ".to_owned())] {
            let card = card_for(Meta {
                city: city.clone(),
                ..meta()
            });
            assert_eq!(card.subline, "19 APRIL 2026", "{city:?}");
        }
    }

    #[test]
    fn the_subline_is_set_in_capitals() {
        let card = card_for(Meta {
            city: Some("Zürich-Höngg".into()),
            date: date!(2026 - 03 - 08),
            ..meta()
        });
        assert_eq!(card.subline, "ZÜRICH-HÖNGG \u{b7} 8 MARCH 2026");
    }

    const LONG_CITY: &str = "Llanfairpwllgwyngyllgogerychwyrndrobwllllantysiliogogogoch, \
        Isle of Anglesey, Wales, United Kingdom of Great Britain and Northern Ireland";

    /// The width of the tracked subline at 11 px.
    fn subline_width(text: &str) -> f32 {
        TextMeasure::new().width_spaced(Role::Label, text, 11.0, 2.2)
    }

    #[test]
    fn a_long_city_is_cut_first_so_the_tracked_subline_keeps_the_date() {
        assert!(subline_width(&LONG_CITY.to_uppercase()) > 624.0);
        let card = card_for(Meta {
            city: Some(LONG_CITY.into()),
            ..meta()
        });
        let city = card
            .subline
            .strip_suffix(" \u{b7} 19 APRIL 2026")
            .unwrap_or_else(|| panic!("{:?} lost the date", card.subline));
        assert_cut_with_one_ellipsis(city, &LONG_CITY.to_uppercase());
        let line = subline_width(&card.subline);
        assert!((624.0 - 12.0..=624.0).contains(&line), "{line} px");
        // Without the tracking the cut would keep more of the city.
        let untracked = width(Role::Label, &card.subline, 11.0);
        assert!(untracked < 624.0 - 60.0, "{untracked} px untracked");
    }

    /// The width of a split title at `size`: the head in Display, the tail
    /// in DisplayItalic.
    fn title_width(title: &TitleView, size: f32) -> f32 {
        width(Role::Display, &title.head, size) + width(Role::DisplayItalic, &title.tail, size)
    }

    #[test]
    fn a_two_word_title_sets_its_last_word_apart_at_76_px() {
        let card = card(&half());
        assert_eq!(card.title, title("Harbour ", "Half"));
        assert_eq!(card.title_size, 76.0);
        assert!(title_width(&card.title, 76.0) <= 624.0);
    }

    #[test]
    fn the_head_keeps_every_word_but_the_last_and_its_space() {
        let card = card_for(Meta {
            event: "Lauf  am Main ".into(),
            ..meta()
        });
        assert_eq!(card.title, title("Lauf  am ", "Main"));
    }

    #[test]
    fn a_one_word_title_is_all_head() {
        for event in ["Spreelauf", "Spreelauf  "] {
            let card = card_for(Meta {
                event: event.into(),
                ..meta()
            });
            assert_eq!(
                (card.title, card.title_size),
                (title("Spreelauf", ""), 76.0),
                "{event:?}"
            );
        }
    }

    const LONG_EVENT: &str = "Maratona Internacional de Lisboa";

    #[test]
    fn a_long_title_is_shrunk_just_enough_for_head_and_tail_to_fit_624_px() {
        let split = title("Maratona Internacional de ", "Lisboa");
        assert!(title_width(&split, 76.0) > 624.0);
        let card = card_for(Meta {
            event: LONG_EVENT.into(),
            ..meta()
        });
        assert_eq!(card.title, split);
        assert!(
            (44.0..76.0).contains(&card.title_size),
            "size {}",
            card.title_size
        );
        let fitted = title_width(&split, card.title_size);
        assert!(fitted <= 624.0, "{fitted} px");
        let larger = title_width(&split, card.title_size + 0.1);
        assert!(larger > 624.0, "{larger} px would still fit");
    }

    #[test]
    fn a_title_too_long_even_at_44_px_drops_the_split_and_is_cut_with_an_ellipsis() {
        let event =
            "Schneider Electric Marathon de Paris, presented by the City of Paris and Friends";
        let card = card_for(Meta {
            event: event.into(),
            ..meta()
        });
        assert_eq!(card.title_size, 44.0);
        assert_eq!(card.title.tail, "");
        assert_cut_with_one_ellipsis(&card.title.head, event);
        let fitted = width(Role::Display, &card.title.head, 44.0);
        assert!((624.0 - 44.0..=624.0).contains(&fitted), "{fitted} px");
    }

    #[test]
    fn a_long_name_is_cut_with_an_ellipsis_in_the_italic_at_26_px() {
        let runner =
            "Maximilian Alexander Konstantin Oberhauser-Lindqvist zu Hohenzollern-Sigmaringen und Waldburg";
        assert!(width(Role::DisplayItalic, runner, 26.0) > 624.0);
        let card = card_for(Meta {
            runner: runner.into(),
            ..meta()
        });
        assert_cut_with_one_ellipsis(&card.runner, runner);
        let fitted = width(Role::DisplayItalic, &card.runner, 26.0);
        assert!((624.0 - 26.0..=624.0).contains(&fitted), "{fitted} px");
    }

    #[test]
    fn the_name_is_kept_as_typed() {
        let card = card_for(Meta {
            runner: "mara de la Cruz-Lindqvist".into(),
            ..meta()
        });
        assert_eq!(card.runner, "mara de la Cruz-Lindqvist");
    }

    /// The width of the time tracked by -3 at `size`.
    fn time_width(time: &str, size: f32) -> f32 {
        TextMeasure::new().width_spaced(Role::Numeral, time, size, -3.0)
    }

    #[test]
    fn the_time_is_set_at_132_px() {
        let card = card(&half());
        assert_eq!(card.time_size, 132.0);
        assert!(time_width(&card.time, 132.0) <= 424.0);
    }

    #[test]
    fn a_long_time_is_shrunk_just_enough_to_fit_424_px() {
        let analysis = Analysis {
            elapsed_s: 100_000.0 * 3600.0 - 1.0,
            ..half()
        };
        let card = card(&analysis);
        assert_eq!(card.time, "99999:59:59");
        assert!(time_width(&card.time, 132.0) > 424.0);
        assert!(
            (80.0..132.0).contains(&card.time_size),
            "size {}",
            card.time_size
        );
        let fitted = time_width(&card.time, card.time_size);
        assert!(fitted <= 424.0, "{fitted} px");
        let larger = time_width(&card.time, card.time_size + 0.1);
        assert!(larger > 424.0, "{larger} px would still fit");
    }

    #[test]
    fn a_time_too_long_even_at_80_px_is_set_whole_at_80_px() {
        let analysis = Analysis {
            elapsed_s: 1e15,
            ..half()
        };
        let card = card(&analysis);
        assert_eq!(card.time, "277777777777:46:40");
        assert!(time_width(&card.time, 80.0) > 424.0);
        assert_eq!(card.time_size, 80.0);
    }

    #[test]
    fn the_legend_has_32_colours_walking_the_gradient() {
        let card = card(&half());
        assert_eq!(card.legend.len(), 32);
        assert_eq!(card.legend, palette().legend(32));
        assert_eq!(card.legend[0], palette().hex(0.0));
        assert_eq!(card.legend[31], palette().hex(1.0));
    }

    #[test]
    fn the_card_carries_the_tokens_of_its_theme_and_the_name_of_its_variant() {
        for theme in Theme::ALL {
            for variant in Variant::ALL {
                let card = card_with(&half(), &meta(), variant, theme);
                assert_eq!(card.theme, theme.tokens(), "{theme} {variant}");
                assert_eq!(card.variant, variant.name());
            }
        }
    }

    #[test]
    fn the_dark_card_takes_its_colours_from_the_palette_it_is_given() {
        let card = card_with(&half(), &meta(), Variant::Ribbon, Theme::Dark);
        let dark = Palette::for_theme(Theme::Dark);
        assert_eq!(card.legend, dark.legend(32));
        assert_eq!(card.splits[13].color, dark.hex(0.0), "km 14");
    }

    #[test]
    fn card_build_fills_exactly_the_shape_block_of_its_variant() {
        for variant in Variant::ALL {
            let card = card_with(&half(), &meta(), variant, Theme::Light);
            let built = [
                ("ribbon", card.ribbon.is_some()),
                ("spiral", card.spiral.is_some()),
                ("trail", card.trail.is_some()),
                ("dots", card.dots.is_some()),
                ("wave", card.wave.is_some()),
                ("ledger", card.ledger.is_some()),
            ];
            let names: Vec<&str> = built
                .iter()
                .filter(|(_, is_built)| *is_built)
                .map(|(name, _)| *name)
                .collect();
            assert_eq!(names, [variant.name()], "{variant}");
        }
    }

    #[test]
    fn the_frame_is_the_same_for_every_variant() {
        let ribbon = card(&half());
        for variant in Variant::ALL {
            let other = card_with(&half(), &meta(), variant, Theme::Light);
            assert_eq!(
                (
                    &other.title,
                    other.title_size,
                    &other.runner,
                    &other.subline,
                    &other.legend
                ),
                (
                    &ribbon.title,
                    ribbon.title_size,
                    &ribbon.runner,
                    &ribbon.subline,
                    &ribbon.legend
                ),
                "{variant}"
            );
            assert_eq!(
                (&other.time, other.time_size, &other.stats),
                (&ribbon.time, ribbon.time_size, &ribbon.stats),
                "{variant}"
            );
            assert_eq!(other.splits, ribbon.splits, "{variant}");
        }
    }

    #[test]
    fn every_split_gets_a_view_with_its_pace_colour_and_scale_position() {
        let card = card(&half());
        assert_eq!(card.splits.len(), 22);
        for (i, view) in card.splits.iter().enumerate() {
            assert_eq!(view.index, i as u32 + 1);
            assert_eq!(view.color, palette().hex(view.t), "km {}", view.index);
        }
        let km11 = &card.splits[10];
        assert_eq!(
            (km11.pace.as_str(), km11.t, km11.partial, km11.fraction),
            ("5:02", 1.0, false, 1.0)
        );
        assert_eq!(card.splits[13].t, 0.0, "km 14 at 5:52");
        assert_eq!(card.splits[13].color, palette().hex(0.0), "km 14 at 5:52");
    }

    #[test]
    fn every_view_is_coloured_by_its_pace_between_the_full_kilometre_anchors() {
        let analysis = half();
        let full: Vec<f64> = analysis.splits[..21]
            .iter()
            .map(Split::pace_s_per_km)
            .collect();
        let (slow, fast) = anchors(&full);
        let card = card(&analysis);
        for (split, view) in analysis.splits.iter().zip(&card.splits) {
            let t = pace_position(split.pace_s_per_km(), slow, fast);
            assert_eq!(view.t, t, "km {}", split.index);
            assert_eq!(view.color, palette().hex(t), "km {}", split.index);
        }
    }

    #[test]
    fn the_partial_kilometre_view_has_its_fraction_and_its_scaled_pace() {
        let last = card(&half()).splits.pop().expect("a last split");
        assert_eq!(last.index, 22);
        assert!(last.partial);
        assert_near(last.fraction, 0.1, "fraction of the last split");
        assert_eq!(last.pace, "5:00");
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
        let views = card(&analysis).splits;
        let colours: Vec<&str> = views.iter().map(|v| v.color.as_str()).collect();
        assert_eq!(
            colours,
            [palette().hex(1.0), palette().hex(0.0), palette().hex(0.0)]
        );
    }

    #[test]
    fn the_average_pace_is_shown_bare_and_placed_on_the_scale() {
        let analysis = half();
        let full: Vec<f64> = analysis.splits[..21]
            .iter()
            .map(Split::pace_s_per_km)
            .collect();
        let (slow, fast) = anchors(&full);
        let average = analysis.elapsed_s / analysis.distance_m * 1000.0;
        let card = card(&analysis);
        assert_eq!(card.avg_pace, "5:30");
        assert_eq!(card.avg_t, pace_position(average, slow, fast));
        assert!((0.0..1.0).contains(&card.avg_t), "{}", card.avg_t);
    }

    /// A closed loop of `n` steps around Lisbon, about 3.5 km east to west
    /// and 2.2 km south to north, starting at its eastern end.
    fn lisbon_loop(n: usize) -> Vec<TrackPoint> {
        (0..=n)
            .map(|i| {
                let a = i as f64 / n as f64 * std::f64::consts::TAU;
                TrackPoint {
                    lat: 38.70 + 0.01 * a.sin(),
                    lon: -9.15 + 0.02 * a.cos(),
                    time: None,
                }
            })
            .collect()
    }

    /// The route preview of the half marathon run along `points`, as `variant`.
    fn preview_of(points: Vec<TrackPoint>, variant: Variant) -> Option<RoutePreview> {
        let analysis = Analysis { points, ..half() };
        card_with(&analysis, &meta(), variant, Theme::Light).route
    }

    /// The points of `M x y L x y ...` path data.
    fn path_points(path: &str) -> Vec<(f32, f32)> {
        path.split(['M', 'L'])
            .filter(|step| !step.trim().is_empty())
            .map(|step| {
                let numbers: Vec<f32> = step
                    .split_whitespace()
                    .map(|n| n.parse().expect("a number"))
                    .collect();
                assert_eq!(numbers.len(), 2, "{step:?} in {path}");
                (numbers[0], numbers[1])
            })
            .collect()
    }

    #[test]
    fn a_loop_is_previewed_inside_the_box_right_of_the_legend_flush_right() {
        let preview = preview_of(lisbon_loop(400), Variant::Ribbon).expect("a preview");
        let points = path_points(&preview.path);
        for &(x, y) in &points {
            assert!(
                (472.0..=672.0).contains(&x) && (750.0..=806.0).contains(&y),
                "({x}, {y}) lies outside x 472..672, y 750..806"
            );
        }
        let (left, right) = points
            .iter()
            .fold((f32::MAX, f32::MIN), |(l, r), &(x, _)| (l.min(x), r.max(x)));
        assert!(
            (right - 672.0).abs() <= 0.01,
            "the right-most point is at x {right}"
        );
        // Narrower than the box, so it is moved right rather than centred.
        assert!(left > 472.0 + 50.0, "the left-most point is at x {left}");
        let start = preview.start;
        assert!(
            preview
                .path
                .starts_with(&format!("M {:.1} {:.1} L ", start.0, start.1)),
            "the start {start:?} is not the first point of {}",
            preview.path
        );
    }

    #[test]
    fn a_route_wider_than_the_box_spans_it_at_mid_height() {
        let line: Vec<TrackPoint> = (0..=10)
            .map(|i| TrackPoint {
                lat: 38.70,
                lon: -9.15 + 0.001 * f64::from(i),
                time: None,
            })
            .collect();
        let preview = preview_of(line, Variant::Ribbon).expect("a preview");
        let points = path_points(&preview.path);
        assert_eq!(points.first(), Some(&(472.0, 778.0)), "{}", preview.path);
        assert_eq!(points.last(), Some(&(672.0, 778.0)), "{}", preview.path);
        assert_eq!(preview.start, (472.0, 778.0));
    }

    #[test]
    fn the_preview_is_simplified_to_at_most_120_points() {
        let preview = preview_of(lisbon_loop(1000), Variant::Ribbon).expect("a preview");
        let points = preview.path.matches('L').count() + 1;
        assert!(points <= 120, "{points} points");
        assert_eq!(points, path_points(&preview.path).len());
    }

    #[test]
    fn every_variant_but_trail_carries_the_same_preview() {
        let ribbon = preview_of(lisbon_loop(400), Variant::Ribbon);
        assert!(ribbon.is_some());
        for variant in Variant::ALL {
            let preview = preview_of(lisbon_loop(400), variant);
            if variant == Variant::Trail {
                assert_eq!(preview, None, "the trail already draws the route");
            } else {
                assert_eq!(preview, ribbon, "{variant}");
            }
        }
    }

    #[test]
    fn a_track_without_two_distinct_points_has_no_preview() {
        let here = TrackPoint {
            lat: 38.70,
            lon: -9.15,
            time: None,
        };
        assert_eq!(
            preview_of(vec![here.clone(), here.clone()], Variant::Ribbon),
            None
        );
        assert_eq!(preview_of(vec![here], Variant::Ribbon), None);
    }
}
