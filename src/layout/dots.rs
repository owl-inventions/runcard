//! Lays out the `dots` variant: a grid of circles, one per kilometre, larger for faster paces.

use minijinja::{context, Value};
use serde::Serialize;

use crate::model::{Analysis, Meta, SplitView};
use crate::palette::Palette;
use crate::render::px;
use crate::text::TextMeasure;

/// The visualisation zone.
const ZONE_X: f32 = 48.0;
const ZONE_Y: f32 = 160.0;
const ZONE_W: f32 = 624.0;
const ZONE_H: f32 = 560.0;
/// Up to this many splits the grid has [`NARROW_COLS`] columns, beyond it
/// [`WIDE_COLS`].
const MAX_NARROW: usize = 49;
const NARROW_COLS: usize = 7;
const WIDE_COLS: usize = 9;
/// Radius of the slowest dot as a share of the cell, and what the fastest adds.
const MIN_RADIUS_SHARE: f32 = 0.16;
const RADIUS_SHARE_RANGE: f32 = 0.26;
/// Cells this large number every kilometre; smaller ones every fifth and the last.
const MIN_LABELLED_CELL: f32 = 56.0;
const LABEL_EVERY: u32 = 5;
/// The kilometre number's inset from the top-left corner of its cell.
const LABEL_INSET: f32 = 4.0;
/// Height of the figures of JetBrains Mono at 11 px (cap height 0.73 em):
/// the baseline lies this far below the top of the number.
const LABEL_FIGURE_HEIGHT: f32 = 8.0;
/// Paper between the fastest dot and its ring.
const RING_CLEARANCE: f32 = 5.0;

/// One kilometre of the grid.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Dot {
    /// 1-based kilometre number.
    pub index: u32,
    /// Centre and radius of the circle.
    pub x: f32,
    pub y: f32,
    pub r: f32,
    /// CSS colour of the kilometre's pace.
    pub color: String,
    /// True for the trailing split shorter than a kilometre, drawn lighter.
    pub partial: bool,
    /// Baseline start of the kilometre number, if it is shown.
    pub label: Option<(f32, f32)>,
}

/// A circle drawn as an outline.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Ring {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

/// What `templates/dots.svg` draws inside the visualisation zone.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Dots {
    /// One dot per split, row by row from the top left.
    pub dots: Vec<Dot>,
    /// The ring around the fastest full kilometre.
    pub fastest: Option<Ring>,
}

/// Lays out the dots in card pixels: one per split in a grid of square
/// cells centred in the zone, each dot centred in its cell, sized by its
/// pace and numbered in the corner.
pub fn build(
    analysis: &Analysis,
    splits: &[SplitView],
    _palette: &Palette,
    _measure: &TextMeasure,
    _meta: &Meta,
) -> Dots {
    let grid = grid(splits.len());
    let last = splits.last().map_or(0, |s| s.index);
    let dots: Vec<Dot> = splits
        .iter()
        .enumerate()
        .map(|(i, view)| {
            let (left, top) = grid.cell_corner(i);
            Dot {
                index: view.index,
                x: left + grid.cell / 2.0,
                y: top + grid.cell / 2.0,
                r: radius(grid.cell, view.t),
                color: view.color.clone(),
                partial: view.partial,
                label: labelled(view.index, last, grid.cell)
                    .then_some((left + LABEL_INSET, top + LABEL_INSET + LABEL_FIGURE_HEIGHT)),
            }
        })
        .collect();
    let fastest = analysis
        .fastest
        .and_then(|index| dots.iter().find(|d| d.index == index))
        .map(|dot| Ring {
            x: dot.x,
            y: dot.y,
            r: dot.r + RING_CLEARANCE,
        });
    Dots { dots, fastest }
}

/// The cells of the grid.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Grid {
    cols: usize,
    rows: usize,
    /// Side of a square cell in px.
    cell: f32,
    /// Top-left corner of the grid.
    left: f32,
    top: f32,
}

impl Grid {
    /// Top-left corner of the cell of the `i`-th split, row by row.
    fn cell_corner(&self, i: usize) -> (f32, f32) {
        let (col, row) = ((i % self.cols) as f32, (i / self.cols) as f32);
        (self.left + col * self.cell, self.top + row * self.cell)
    }
}

/// The grid for `n` splits: 7 columns up to 49 splits, else 9, as many rows
/// as they fill, the largest square cells both fit the zone with, centred.
fn grid(n: usize) -> Grid {
    let cols = if n <= MAX_NARROW {
        NARROW_COLS
    } else {
        WIDE_COLS
    };
    let rows = n.div_ceil(cols).max(1);
    let cell = (ZONE_W / cols as f32).min(ZONE_H / rows as f32);
    Grid {
        cols,
        rows,
        cell,
        left: ZONE_X + (ZONE_W - cols as f32 * cell) / 2.0,
        top: ZONE_Y + (ZONE_H - rows as f32 * cell) / 2.0,
    }
}

/// Radius of the dot of a pace at `t` (0 slowest, 1 fastest) in a `cell`.
fn radius(cell: f32, t: f32) -> f32 {
    cell * (MIN_RADIUS_SHARE + RADIUS_SHARE_RANGE * t)
}

/// Whether kilometre `index` of a run whose last split is `last` shows its
/// number in a cell of this size.
fn labelled(index: u32, last: u32, cell: f32) -> bool {
    cell >= MIN_LABELLED_CELL || index.is_multiple_of(LABEL_EVERY) || index == last
}

impl Dots {
    /// The block as the template sees it, under the key `dots`, every number
    /// rounded to two decimals.
    pub fn to_value(&self) -> Value {
        let dots: Vec<Value> = self
            .dots
            .iter()
            .map(|d| {
                context! {
                    index => d.index,
                    x => px(d.x),
                    y => px(d.y),
                    r => px(d.r),
                    color => &d.color,
                    partial => d.partial,
                    label => d.label.map(|(x, y)| context! { x => px(x), y => px(y) }),
                }
            })
            .collect();
        context! {
            dots => dots,
            fastest => self
                .fastest
                .map(|ring| context! { x => px(ring.x), y => px(ring.y), r => px(ring.r) }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Split;
    use crate::theme::Theme;

    fn assert_near(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "{what} was {actual}, expected {expected}"
        );
    }

    #[test]
    fn the_grid_has_7_columns_up_to_49_splits_then_9() {
        // n, columns, rows, cell
        let cases = [
            (10, 7, 2, 624.0 / 7.0),
            (21, 7, 3, 624.0 / 7.0),
            (43, 7, 7, 80.0),
            (49, 7, 7, 80.0),
            (50, 9, 6, 624.0 / 9.0),
            (60, 9, 7, 624.0 / 9.0),
        ];
        for (n, cols, rows, cell) in cases {
            let grid = grid(n);
            assert_eq!((grid.cols, grid.rows), (cols, rows), "{n} splits");
            assert_near(grid.cell, cell, &format!("cell of {n} splits"));
            assert!(grid.rows as f32 * grid.cell <= 560.0 + 1e-3, "{n} splits");
            assert!(grid.cols as f32 * grid.cell <= 624.0 + 1e-3, "{n} splits");
        }
    }

    #[test]
    fn the_grid_is_centred_in_the_zone_both_ways() {
        for n in [1, 10, 21, 43, 49, 50, 60, 110] {
            let grid = grid(n);
            let (width, height) = (grid.cols as f32 * grid.cell, grid.rows as f32 * grid.cell);
            assert_near(
                grid.left - 48.0,
                672.0 - (grid.left + width),
                "side margins",
            );
            assert_near(
                grid.top - 160.0,
                720.0 - (grid.top + height),
                "top and bottom",
            );
            assert!(grid.left >= 48.0 - 1e-3 && grid.top >= 160.0 - 1e-3, "{n}");
        }
        let half = grid(21);
        assert_near(half.left, 48.0, "left of 21 splits");
        assert_near(
            half.top,
            160.0 + (560.0 - 3.0 * 624.0 / 7.0) / 2.0,
            "top of 21",
        );
        let full = grid(43);
        assert_near((full.left, full.top).0, 80.0, "left of 43 splits");
        assert_near(full.top, 160.0, "top of 43 splits");
    }

    #[test]
    fn the_radius_grows_from_16_to_42_percent_of_the_cell() {
        assert_near(radius(100.0, 0.0), 16.0, "slowest");
        assert_near(radius(100.0, 1.0), 42.0, "fastest");
        assert_near(radius(80.0, 0.5), 80.0 * 0.29, "middle");
    }

    #[test]
    fn large_cells_show_every_kilometre_number_small_ones_every_fifth_and_the_last() {
        assert!((1..=21).all(|i| labelled(i, 21, 89.0)));
        assert!(labelled(7, 49, 56.0), "56 px is large enough");
        let shown: Vec<u32> = (1..=110).filter(|&i| labelled(i, 110, 46.0)).collect();
        let expected: Vec<u32> = (1..=22).map(|i| i * 5).collect();
        assert_eq!(shown, expected);
        let shown: Vec<u32> = (1..=112).filter(|&i| labelled(i, 112, 46.0)).collect();
        assert_eq!(shown.last(), Some(&112));
        assert_eq!(shown.len(), 23);
    }

    // ----- build and to_value -----

    fn view(index: u32, fraction: f32, t: f32) -> SplitView {
        SplitView {
            index,
            pace: "5:00".into(),
            color: format!("#0000{index:02x}"),
            partial: fraction < 1.0,
            fraction,
            t,
        }
    }

    fn analysis(views: &[SplitView], fastest: Option<u32>) -> Analysis {
        let splits: Vec<Split> = views
            .iter()
            .map(|v| Split {
                index: v.index,
                distance_m: f64::from(v.fraction) * 1000.0,
                seconds: 300.0 * f64::from(v.fraction),
                partial: v.partial,
            })
            .collect();
        Analysis {
            distance_m: splits.iter().map(|s| s.distance_m).sum(),
            elapsed_s: splits.iter().map(|s| s.seconds).sum(),
            splits,
            fastest,
            start: None,
            points: Vec::new(),
        }
    }

    fn meta() -> Meta {
        Meta {
            runner: "Mara Lindqvist".into(),
            event: "Harbour Half".into(),
            city: None,
            date: time::macros::date!(2026 - 04 - 19),
        }
    }

    fn dots_of(views: &[SplitView], fastest: Option<u32>) -> Dots {
        build(
            &analysis(views, fastest),
            views,
            &Palette::for_theme(Theme::Light),
            &TextMeasure::new(),
            &meta(),
        )
    }

    /// 21 full kilometres from slow to fast and a 0.1 km partial.
    fn half() -> Vec<SplitView> {
        let mut views: Vec<SplitView> = (1..=21)
            .map(|i| view(i, 1.0, (i - 1) as f32 / 20.0))
            .collect();
        views.push(view(22, 0.1, 0.5));
        views
    }

    #[test]
    fn every_split_gets_a_dot_in_its_colour_in_its_cell_row_by_row() {
        let views = half();
        let dots = dots_of(&views, Some(21));
        let grid = grid(22);
        assert_eq!(dots.dots.len(), 22);
        for (i, (dot, view)) in dots.dots.iter().zip(&views).enumerate() {
            assert_eq!(
                (dot.index, &dot.color, dot.partial),
                (view.index, &view.color, view.partial)
            );
            let (col, row) = ((i % 7) as f32, (i / 7) as f32);
            assert_near(dot.x, grid.left + (col + 0.5) * grid.cell, "x");
            assert_near(dot.y, grid.top + (row + 0.5) * grid.cell, "y");
            assert_near(dot.r, radius(grid.cell, view.t), "r");
        }
        assert!(dots.dots[21].partial);
        assert!(dots.dots[..21].iter().all(|d| !d.partial));
    }

    #[test]
    fn the_kilometre_number_sits_4_px_inside_the_top_left_corner_of_its_cell() {
        let dots = dots_of(&half(), None);
        let grid = grid(22);
        for (i, dot) in dots.dots.iter().enumerate() {
            let (col, row) = ((i % 7) as f32, (i / 7) as f32);
            let (x, y) = dot.label.expect("large cells show every number");
            assert_near(x, grid.left + col * grid.cell + 4.0, "label x");
            // The baseline under the 8 px figures of 11 px JetBrains Mono.
            assert_near(y, grid.top + row * grid.cell + 12.0, "label baseline");
        }
    }

    #[test]
    fn a_small_grid_numbers_every_fifth_kilometre_and_the_last() {
        let views: Vec<SplitView> = (1..=103).map(|i| view(i, 1.0, 0.5)).collect();
        let dots = dots_of(&views, None);
        assert!(grid(103).cell < 56.0);
        let shown: Vec<u32> = dots
            .dots
            .iter()
            .filter(|d| d.label.is_some())
            .map(|d| d.index)
            .collect();
        let mut expected: Vec<u32> = (1..=20).map(|i| i * 5).collect();
        expected.push(103);
        assert_eq!(shown, expected);
    }

    #[test]
    fn the_fastest_kilometre_gets_a_ring_5_px_outside_its_dot() {
        let dots = dots_of(&half(), Some(21));
        let fastest = &dots.dots[20];
        let ring = dots.fastest.expect("a ring");
        assert_eq!((ring.x, ring.y), (fastest.x, fastest.y));
        assert_near(ring.r, fastest.r + 5.0, "ring radius");
        assert_eq!(dots_of(&half(), None).fastest, None);
    }

    #[test]
    fn every_dot_and_ring_stays_inside_its_cell_and_the_zone() {
        for n in [1, 10, 21, 43, 49, 50, 60, 110] {
            let views: Vec<SplitView> = (1..=n).map(|i| view(i, 1.0, 1.0)).collect();
            let dots = dots_of(&views, Some(n));
            let ring = dots.fastest.unwrap();
            let cell = grid(n as usize).cell;
            // Clear of the largest dot in the next cell.
            let neighbour = radius(cell, 1.0);
            assert!(
                ring.r + 0.75 + neighbour < cell,
                "{n}: ring {} in cell {cell}",
                ring.r
            );
            for dot in &dots.dots {
                assert!(dot.x - dot.r >= 48.0 && dot.x + dot.r <= 672.0, "{n}");
                assert!(dot.y - dot.r >= 160.0 && dot.y + dot.r <= 720.0, "{n}");
            }
        }
    }

    #[test]
    fn without_splits_there_are_no_dots() {
        let dots = dots_of(&[], None);
        assert!(dots.dots.is_empty());
        assert_eq!(dots.fastest, None);
    }

    #[test]
    fn the_template_value_has_every_number_rounded_to_two_decimals() {
        let dots = Dots {
            dots: vec![Dot {
                index: 7,
                x: 100.123_45,
                y: 200.987_65,
                r: 30.004,
                color: "#123456".into(),
                partial: true,
                label: Some((52.333_33, 172.666_66)),
            }],
            fastest: Some(Ring {
                x: 1.234_5,
                y: 2.345_6,
                r: 35.0,
            }),
        };
        let value = dots.to_value();
        let dot = value
            .get_attr("dots")
            .unwrap()
            .get_item(&Value::from(0))
            .unwrap();
        let get = |value: &Value, path: &str| {
            path.split('.')
                .try_fold(value.clone(), |v, key| v.get_attr(key).ok())
                .unwrap_or_else(|| panic!("no {path}"))
                .to_string()
        };
        assert_eq!(get(&dot, "index"), "7");
        assert_eq!(get(&dot, "x"), "100.12");
        assert_eq!(get(&dot, "y"), "200.99");
        assert_eq!(get(&dot, "r"), "30");
        assert_eq!(get(&dot, "color"), "#123456");
        assert!(dot.get_attr("partial").unwrap().is_true());
        assert_eq!(get(&dot, "label.x"), "52.33");
        assert_eq!(get(&dot, "label.y"), "172.67");
        assert_eq!(get(&value, "fastest.x"), "1.23");
        assert_eq!(get(&value, "fastest.y"), "2.35");
        assert_eq!(get(&value, "fastest.r"), "35");
        let unlabelled = Dots {
            dots: vec![Dot {
                label: None,
                ..dots.dots[0].clone()
            }],
            fastest: None,
        }
        .to_value();
        let dot = unlabelled
            .get_attr("dots")
            .unwrap()
            .get_item(&Value::from(0))
            .unwrap();
        assert!(dot.get_attr("label").unwrap().is_none());
        assert!(unlabelled.get_attr("fastest").unwrap().is_none());
    }
}
