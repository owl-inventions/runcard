//! Maps split paces to stripe colours with a colour gradient.

use colorgrad::{BlendMode, Gradient, GradientBuilder, LinearGradient};

/// Gradient stops of the light scale from the slowest to the fastest pace:
/// slate blue through cream to orange.
const LIGHT_STOPS: [&str; 5] = ["#2c4a5e", "#9fb6c3", "#f3e3c8", "#f6a43a", "#e8601c"];
/// Gradient stops of the dark scale: violet, blue, teal, yellow, red-orange.
const DARK_STOPS: [&str; 5] = ["#3b2a6b", "#2f6fb3", "#3fc1b0", "#f6c445", "#ff5a36"];

/// The slow-to-fast colour scale of the stripes and the legend, interpolated
/// in Oklab so the steps look even.
pub struct Palette {
    gradient: LinearGradient,
}

impl Palette {
    /// The scale for the light variants.
    pub fn light() -> Self {
        Self::with_stops(&LIGHT_STOPS)
    }

    /// The scale for the dark variant, bright enough to stand out on black.
    pub fn dark() -> Self {
        Self::with_stops(&DARK_STOPS)
    }

    fn with_stops(stops: &[&str]) -> Self {
        let gradient = GradientBuilder::new()
            .html_colors(stops)
            .mode(BlendMode::Oklab)
            .build::<LinearGradient>()
            .expect("the palette stops are valid colours");
        Self { gradient }
    }

    /// Colour at position `t` (0 = slowest, 1 = fastest) as lowercase `#rrggbb`.
    pub fn hex(&self, t: f32) -> String {
        let [r, g, b, _] = self.gradient.at(t).to_rgba8();
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    /// `n` colours evenly spaced over the whole scale, slowest first.
    pub fn legend(&self, n: usize) -> Vec<String> {
        let last = n.saturating_sub(1).max(1) as f32;
        (0..n).map(|i| self.hex(i as f32 / last)).collect()
    }
}

/// The light scale.
impl Default for Palette {
    fn default() -> Self {
        Self::light()
    }
}

/// Position of a pace on the colour scale: the slow anchor maps to 0, the fast
/// anchor to 1, paces beyond the anchors are clamped. Equal anchors give 0.5.
pub fn pace_position(pace_s_per_km: f64, slow_anchor: f64, fast_anchor: f64) -> f32 {
    if slow_anchor == fast_anchor {
        return 0.5;
    }
    let t = (slow_anchor - pace_s_per_km) / (slow_anchor - fast_anchor);
    t.clamp(0.0, 1.0) as f32
}

/// Paces from this many on lose their single slowest and fastest one before
/// the anchors are taken.
const TRIM_FROM: usize = 4;

/// Anchors closer than this (s/km) are moved apart to it, so a second of
/// noise on an even pace does not span the whole scale.
const MIN_SPREAD_S_PER_KM: f64 = 20.0;

/// The colour scale anchors of a run, (slow, fast) in s/km: the 95th and the
/// 5th percentile of the paces, so one very slow or very fast kilometre does
/// not flatten the others. From four paces on, the slowest and the fastest one
/// are left out first; anchors closer than 20 s/km are widened to 20 s/km
/// around their midpoint. Fewer than two paces give that pace for both (0 for
/// none).
pub fn anchors(paces: &[f64]) -> (f64, f64) {
    if paces.len() < 2 {
        let pace = paces.first().copied().unwrap_or(0.0);
        return (pace, pace);
    }
    let mut sorted = paces.to_vec();
    sorted.sort_by(f64::total_cmp);
    let kept = if sorted.len() >= TRIM_FROM {
        &sorted[1..sorted.len() - 1]
    } else {
        &sorted[..]
    };
    let (slow, fast) = (percentile(kept, 0.95), percentile(kept, 0.05));
    if slow - fast < MIN_SPREAD_S_PER_KM {
        let middle = (slow + fast) / 2.0;
        let half = MIN_SPREAD_S_PER_KM / 2.0;
        (middle + half, middle - half)
    } else {
        (slow, fast)
    }
}

/// Percentile `q` (0..=1) of sorted values, interpolating between neighbours.
fn percentile(sorted: &[f64], q: f64) -> f64 {
    let rank = q * (sorted.len() - 1) as f64;
    let below = rank.floor() as usize;
    let above = rank.ceil() as usize;
    let fraction = rank - below as f64;
    sorted[below] + (sorted[above] - sorted[below]) * fraction
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOWEST: &str = "#2c4a5e";
    const FASTEST: &str = "#e8601c";
    const DARK_SLOWEST: &str = "#3b2a6b";
    const DARK_FASTEST: &str = "#ff5a36";

    fn assert_hex(color: &str) {
        assert_eq!(color.len(), 7, "{color:?} is not #rrggbb");
        assert!(color.starts_with('#'), "{color:?} does not start with #");
        assert!(
            u32::from_str_radix(&color[1..], 16).is_ok(),
            "{color:?} is not hexadecimal"
        );
        assert_eq!(color, color.to_lowercase(), "{color:?} is not lowercase");
    }

    #[test]
    fn the_ends_of_the_scale_are_the_first_and_last_stops() {
        let palette = Palette::light();
        assert_eq!(palette.hex(0.0), SLOWEST);
        assert_eq!(palette.hex(1.0), FASTEST);
    }

    #[test]
    fn the_inner_stops_sit_at_the_quarters_and_blend_in_oklab_between() {
        let palette = Palette::light();
        assert_eq!(palette.hex(0.25), "#9fb6c3");
        assert_eq!(palette.hex(0.5), "#f3e3c8");
        assert_eq!(palette.hex(0.75), "#f6a43a");
        // Halfway between the first two stops; blending in RGB gives #668091.
        assert_eq!(palette.hex(0.125), "#637e8f");
    }

    #[test]
    fn the_dark_scale_runs_from_violet_through_its_stops_to_red() {
        let palette = Palette::dark();
        assert_eq!(palette.hex(0.0), DARK_SLOWEST);
        assert_eq!(palette.hex(0.25), "#2f6fb3");
        assert_eq!(palette.hex(0.5), "#3fc1b0");
        assert_eq!(palette.hex(0.75), "#f6c445");
        assert_eq!(palette.hex(1.0), DARK_FASTEST);
        // Halfway between the teal and the yellow stop, blended in Oklab.
        assert_eq!(palette.hex(0.625), "#acc586");
    }

    /// Hue angle in Oklab, in degrees from 0 to 360, at position `t`.
    fn oklab_hue(palette: &Palette, t: f32) -> f32 {
        let [_, a, b, _] = palette.gradient.at(t).to_oklaba();
        b.atan2(a).to_degrees().rem_euclid(360.0)
    }

    #[test]
    fn both_scales_walk_the_hue_one_way_from_slow_to_fast() {
        for (name, palette) in [("light", Palette::light()), ("dark", Palette::dark())] {
            let hues: Vec<f32> = (0..=40)
                .map(|i| oklab_hue(&palette, i as f32 / 40.0))
                .collect();
            assert!(
                hues.windows(2).all(|w| w[1] < w[0]),
                "the {name} hue does not fall steadily: {hues:?}"
            );
        }
    }

    #[test]
    fn every_sample_is_a_lowercase_rrggbb_colour() {
        for palette in [Palette::light(), Palette::dark()] {
            for i in 0..=20 {
                assert_hex(&palette.hex(i as f32 / 20.0));
            }
        }
    }

    #[test]
    fn the_legend_spans_the_whole_scale() {
        let legend = Palette::default().legend(5);
        assert_eq!(legend.len(), 5);
        assert_eq!(legend[0], SLOWEST);
        assert_eq!(legend[4], FASTEST);
        legend.iter().for_each(|c| assert_hex(c));
    }

    #[test]
    fn the_dark_legend_spans_the_dark_scale() {
        let legend = Palette::dark().legend(5);
        assert_eq!(
            legend,
            [DARK_SLOWEST, "#2f6fb3", "#3fc1b0", "#f6c445", DARK_FASTEST]
        );
    }

    #[test]
    fn the_default_palette_is_the_light_one() {
        assert_eq!(Palette::default().legend(9), Palette::light().legend(9));
    }

    #[test]
    fn the_slow_anchor_maps_to_zero_and_the_fast_anchor_to_one() {
        assert_eq!(pace_position(400.0, 400.0, 300.0), 0.0);
        assert_eq!(pace_position(300.0, 400.0, 300.0), 1.0);
        assert!((pace_position(350.0, 400.0, 300.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn paces_beyond_the_anchors_are_clamped() {
        assert_eq!(pace_position(600.0, 400.0, 300.0), 0.0);
        assert_eq!(pace_position(200.0, 400.0, 300.0), 1.0);
    }

    #[test]
    fn equal_anchors_put_every_pace_in_the_middle() {
        assert_eq!(pace_position(320.0, 300.0, 300.0), 0.5);
    }

    fn assert_anchors(paces: &[f64], expected: (f64, f64)) {
        let (slow, fast) = anchors(paces);
        assert!(
            (slow - expected.0).abs() < 1e-9 && (fast - expected.1).abs() < 1e-9,
            "anchors of {paces:?} were ({slow}, {fast}), expected {expected:?}"
        );
    }

    #[test]
    fn anchors_are_the_95th_and_5th_percentiles_without_the_extremes() {
        // 250 and 500 are dropped; the percentiles of 300..=350 remain.
        assert_anchors(
            &[330.0, 250.0, 300.0, 350.0, 310.0, 500.0, 340.0, 320.0],
            (347.5, 302.5),
        );
    }

    #[test]
    fn fewer_than_four_paces_keep_their_extremes() {
        assert_anchors(&[340.0, 300.0, 380.0], (376.0, 304.0));
    }

    #[test]
    fn anchors_closer_than_20_s_per_km_are_widened_around_their_midpoint() {
        // The percentiles of 302, 304, 306 are 305.8 and 302.2, midpoint 304.
        assert_anchors(&[300.0, 302.0, 304.0, 306.0, 308.0], (314.0, 294.0));
    }

    /// Positions on the scale of `paces` between their own anchors.
    fn positions(paces: &[f64]) -> Vec<f32> {
        let (slow, fast) = anchors(paces);
        paces
            .iter()
            .map(|&p| pace_position(p, slow, fast))
            .collect()
    }

    #[test]
    fn a_walked_kilometre_alone_takes_the_slow_end_while_the_others_spread() {
        let mut paces: Vec<f64> = (297..=305).map(f64::from).collect();
        paces.push(600.0);
        let positions = positions(&paces);
        let (running, walk) = positions.split_at(9);
        assert_eq!(walk, [0.0]);
        assert!(
            running.iter().all(|&t| (0.2..=0.8).contains(&t)),
            "{running:?}"
        );
        assert!(
            running.windows(2).all(|w| w[0] > w[1]),
            "slower kilometres are not further from the fast end: {running:?}"
        );
        assert!(running[0] - running[8] >= 0.3, "{running:?}");
    }

    #[test]
    fn a_second_of_noise_on_an_even_pace_stays_in_the_middle_of_the_scale() {
        let paces = [
            300.0, 301.0, 299.0, 300.0, 300.0, 301.0, 299.0, 300.0, 301.0, 299.0,
        ];
        let positions = positions(&paces);
        assert!(
            positions.iter().all(|&t| (0.4..=0.6).contains(&t)),
            "{positions:?}"
        );
    }

    #[test]
    fn a_single_pace_is_both_anchors() {
        assert_eq!(anchors(&[312.0]), (312.0, 312.0));
    }
}
