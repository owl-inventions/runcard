//! Builds the template context (`CardData`) from an `Analysis` and the run
//! `Meta`: the shared frame (title block, legend, footer) and the shape block
//! of the chosen variant.

use crate::format::{self, DOT};
use crate::layout;
use crate::model::{Analysis, CardData, Meta, Split, SplitView};
use crate::palette::{self, Palette};
use crate::text::{self, Role, TextMeasure};
use crate::theme::Theme;
use crate::variant::Variant;

/// Width of the content box inside the 48 px margins (x = 48 to 672).
pub const CONTENT_WIDTH: f32 = 624.0;
/// Number of 5 px rects in the 160 px legend bar.
pub const LEGEND_STEPS: usize = 32;
/// The event title: Display at 40 px, shrunk down to 28 px, then cut.
const TITLE_LINE: Line = Line {
    role: Role::Display,
    size: 40.0,
    min_size: 28.0,
    max_width: CONTENT_WIDTH,
};
/// The runner's name: Label at 14 px, cut but never shrunk.
const RUNNER_LINE: Line = Line {
    role: Role::Label,
    size: 14.0,
    min_size: 14.0,
    max_width: CONTENT_WIDTH,
};
/// The line under the title: Label at 12 px.
const SUBLINE: Line = Line {
    role: Role::Label,
    size: 12.0,
    min_size: 12.0,
    max_width: CONTENT_WIDTH,
};

/// Lays out and formats everything the frame and the template of `variant`
/// show, in the colours of `theme` with `palette` as its colour scale.
///
/// A long title is set smaller, down to 28 px, and cut with an ellipsis if
/// it does not fit even then; a long name is cut; a long city is cut before
/// the date. Only the shape block of `variant` is built.
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
    let fastest = analysis
        .fastest
        .and_then(|index| splits.iter().find(|s| s.index == index))
        .map(|split| format::fastest(split.index, split.pace_s_per_km()))
        .unwrap_or_default();
    let (title, title_size) = TITLE_LINE.fit(measure, &meta.event);
    let (runner, _) = RUNNER_LINE.fit(measure, &meta.runner);
    let block = |shown: Variant| variant == shown;
    CardData {
        variant: variant.name(),
        theme: theme.tokens(),
        title,
        title_size,
        subline: subline(meta, measure),
        legend: palette.legend(LEGEND_STEPS),
        time: format::duration(analysis.elapsed_s),
        distance: format::distance(analysis.distance_m),
        pace: format::pace(average),
        fastest,
        runner,
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
}

impl Line {
    /// Whether `text` fits at `size`.
    pub(crate) fn fits(&self, measure: &TextMeasure, text: &str, size: f32) -> bool {
        measure.width(self.role, text, size) <= self.max_width
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

/// `<city> · <date>`, or the date alone without a city. A city too long for
/// the line is cut with an ellipsis so the date stays whole.
fn subline(meta: &Meta, measure: &TextMeasure) -> String {
    let date = format::date(meta.date);
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
    line(&text::truncate_to_fit(city, fits))
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
    use crate::model::TrackPoint;
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

    #[test]
    fn the_frame_texts_are_formatted_for_the_card() {
        let card = card(&half());
        assert_eq!(card.title, "Harbour Half");
        assert_eq!(card.subline, "Lisbon \u{b7} 19 April 2026");
        assert_eq!(card.runner, "Mara Lindqvist");
        // 19 kilometres at 5:30, 5:02, 5:52 and 30 s for the last 100 m.
        assert_eq!(card.time, "1:55:54");
        assert_eq!(card.distance, "21.10 km");
        // 6954 s over 21.1 km.
        assert_eq!(card.pace, "5:30 /km");
        assert_eq!(card.fastest, "km 11 \u{b7} 5:02");
    }

    #[test]
    fn a_run_without_a_full_kilometre_names_no_fastest_one() {
        let card = card(&short_run());
        assert_eq!(card.fastest, "");
        assert_eq!(card.distance, "0.60 km");
        assert_eq!(card.time, "3:00");
    }

    #[test]
    fn without_a_city_the_subline_is_just_the_date() {
        for city in [None, Some(String::new()), Some("  ".to_owned())] {
            let card = card_for(Meta {
                city: city.clone(),
                ..meta()
            });
            assert_eq!(card.subline, "19 April 2026", "{city:?}");
        }
    }

    const LONG_CITY: &str = "Llanfairpwllgwyngyllgogerychwyrndrobwllllantysiliogogogoch, \
        Isle of Anglesey, Wales, United Kingdom of Great Britain and Northern Ireland";

    #[test]
    fn a_long_city_is_cut_first_so_the_subline_keeps_the_date() {
        assert!(width(Role::Label, LONG_CITY, 12.0) > 624.0);
        let card = card_for(Meta {
            city: Some(LONG_CITY.into()),
            ..meta()
        });
        let city = card
            .subline
            .strip_suffix(" \u{b7} 19 April 2026")
            .unwrap_or_else(|| panic!("{:?} lost the date", card.subline));
        assert_cut_with_one_ellipsis(city, LONG_CITY);
        let line = width(Role::Label, &card.subline, 12.0);
        assert!((624.0 - 12.0..=624.0).contains(&line), "{line} px");
    }

    #[test]
    fn a_normal_title_keeps_its_full_size() {
        let card = card(&half());
        assert_eq!(
            (card.title.as_str(), card.title_size),
            ("Harbour Half", 40.0)
        );
    }

    const LONG_EVENT: &str = "Maratona Internacional de Lisboa";

    #[test]
    fn a_long_title_is_shrunk_just_enough_to_fit_624_px() {
        assert!(width(Role::Display, LONG_EVENT, 40.0) > 624.0);
        let card = card_for(Meta {
            event: LONG_EVENT.into(),
            ..meta()
        });
        assert_eq!(card.title, LONG_EVENT);
        assert!(
            (28.0..40.0).contains(&card.title_size),
            "size {}",
            card.title_size
        );
        let fitted = width(Role::Display, LONG_EVENT, card.title_size);
        assert!(fitted <= 624.0, "{fitted} px");
        let larger = width(Role::Display, LONG_EVENT, card.title_size + 0.1);
        assert!(larger > 624.0, "{larger} px would still fit");
    }

    #[test]
    fn an_absurdly_long_title_is_cut_with_an_ellipsis_at_28_px() {
        let event =
            "Schneider Electric Marathon de Paris, presented by the City of Paris and Friends";
        let card = card_for(Meta {
            event: event.into(),
            ..meta()
        });
        assert_eq!(card.title_size, 28.0);
        assert_cut_with_one_ellipsis(&card.title, event);
        let fitted = width(Role::Display, &card.title, 28.0);
        assert!((624.0 - 30.0..=624.0).contains(&fitted), "{fitted} px");
    }

    #[test]
    fn a_long_name_is_cut_with_an_ellipsis_at_its_size() {
        let runner =
            "Maximilian Alexander Konstantin Oberhauser-Lindqvist zu Hohenzollern-Sigmaringen und Waldburg";
        assert!(width(Role::Label, runner, 14.0) > 624.0);
        let card = card_for(Meta {
            runner: runner.into(),
            ..meta()
        });
        assert_cut_with_one_ellipsis(&card.runner, runner);
        let fitted = width(Role::Label, &card.runner, 14.0);
        assert!((624.0 - 16.0..=624.0).contains(&fitted), "{fitted} px");
    }

    #[test]
    fn the_name_is_kept_as_typed() {
        let card = card_for(Meta {
            runner: "mara de la Cruz-Lindqvist".into(),
            ..meta()
        });
        assert_eq!(card.runner, "mara de la Cruz-Lindqvist");
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
                    &other.subline,
                    &other.legend
                ),
                (
                    &ribbon.title,
                    ribbon.title_size,
                    &ribbon.subline,
                    &ribbon.legend
                ),
                "{variant}"
            );
            assert_eq!(
                (
                    &other.time,
                    &other.distance,
                    &other.pace,
                    &other.fastest,
                    &other.runner
                ),
                (
                    &ribbon.time,
                    &ribbon.distance,
                    &ribbon.pace,
                    &ribbon.fastest,
                    &ribbon.runner
                ),
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
}
