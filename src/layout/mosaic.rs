//! Lays out the `mosaic` variant: one tile per kilometre in a grid.

use minijinja::{context, Value};
use serde::Serialize;

use crate::model::{Analysis, Meta, SplitView, Theme};
use crate::palette::Palette;
use crate::render::px;
use crate::route::{self, BBox};
use crate::text::TextMeasure;
use crate::variant::Variant;

/// Top-left corner of the grid.
const GRID_LEFT: f32 = 32.0;
const GRID_TOP: f32 = 180.0;
/// Width of the grid at full tile size (x = 32 to 688).
const GRID_WIDTH: f32 = 656.0;
/// The most height the grid may take, so it ends by y = 660.
const GRID_MAX_HEIGHT: f32 = 480.0;
/// Gap between neighbouring tiles, across and down.
const GAP: f32 = 10.0;
/// Inset of the fastest tile's ring from the tile's edge.
const RING_INSET: f32 = 4.0;
/// The box right of the runner's name that the route outline is fitted into:
/// classic's box, 40 px lower. Its left edge is classic's, so the name fits
/// the same 516 px and the template shows `CardData`'s fitted `runner`.
const ROUTE_BOX: BBox = BBox {
    x: 564.0,
    y: 668.0,
    w: 120.0,
    h: 90.0,
};
/// Point budget of the simplified route outline, as classic's.
const ROUTE_MAX_POINTS: usize = 150;
/// Offset of the kilometre number's baseline start from the tile's corner.
const KM_X: f32 = 10.0;
const KM_Y: f32 = 20.0;
/// Size of the pace and the height of its baseline, as fractions of the tile.
const PACE_SIZE: f32 = 0.42;
const PACE_BASELINE: f32 = 0.62;
/// Height of Bebas Neue's figures as a fraction of its font size.
const FIGURE_HEIGHT: f32 = 0.7;
/// Least room between the km number's baseline and the top of the pace.
const PACE_CLEARANCE: f32 = 4.0;
/// Height of the partial tile's distance baseline above the tile's bottom.
const DETAIL_ABOVE_BOTTOM: f32 = 10.0;

/// One kilometre as a square tile of the grid.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Tile {
    /// 1-based kilometre number.
    pub index: u32,
    /// Top-left corner.
    pub x: f32,
    pub y: f32,
    /// Width and height.
    pub size: f32,
    /// CSS colour of the kilometre's pace.
    pub color: String,
    /// True when the tile's text is set in the theme's ink, false for paper.
    pub ink_text: bool,
    /// Pace without the unit, e.g. `5:02`.
    pub pace: String,
    /// True for the trailing split that is shorter than a kilometre.
    pub partial: bool,
    /// The distance of a partial kilometre, e.g. `0.46 km`.
    pub detail: Option<String>,
}

/// What `templates/mosaic.svg` draws besides the shared card data.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Mosaic {
    /// One tile per split, row by row.
    pub tiles: Vec<Tile>,
    pub cols: u32,
    pub rows: u32,
    /// Width and height of every tile.
    pub size: f32,
    /// Top-left corner and size of the ring around the fastest full kilometre.
    pub fastest_ring: Option<(f32, f32, f32)>,
    /// Route outline fitted into the box right of the name, lower than classic's.
    pub route: Vec<(f32, f32)>,
    /// Start marker of the route.
    pub route_start: (f32, f32),
}

/// Lays out the tile grid in card pixels.
///
/// The tiles fill the width of the card in 5, 7 or 9 columns; a grid too tall
/// to end by y = 660 gets smaller tiles in the same columns, centred across.
/// A tile's text is the mosaic theme's ink or paper, whichever contrasts more
/// with the tile.
pub fn build(
    analysis: &Analysis,
    splits: &[SplitView],
    _palette: &Palette,
    _measure: &TextMeasure,
    _meta: &Meta,
) -> Mosaic {
    let n = splits.len();
    let cols = columns(n);
    let rows = n.div_ceil(cols as usize) as u32;
    let size = tile_size(cols, rows);
    // A shrunk grid is narrower than the card's width: centre it.
    let left = GRID_LEFT + (GRID_WIDTH - (cols as f32 * (size + GAP) - GAP)) / 2.0;
    let tiles: Vec<Tile> = splits
        .iter()
        .zip(&analysis.splits)
        .enumerate()
        .map(|(i, (view, split))| {
            let (col, row) = ((i as u32 % cols) as f32, (i as u32 / cols) as f32);
            Tile {
                index: view.index,
                x: left + col * (size + GAP),
                y: GRID_TOP + row * (size + GAP),
                size,
                color: view.color.clone(),
                ink_text: ink_text(&view.color, Variant::Mosaic.theme()),
                pace: view.pace.clone(),
                partial: view.partial,
                detail: split
                    .partial
                    .then(|| format!("{:.2} km", split.distance_m / 1000.0)),
            }
        })
        .collect();
    let fastest_ring = analysis
        .fastest
        .and_then(|index| tiles.iter().find(|tile| tile.index == index))
        .map(|tile| {
            (
                tile.x + RING_INSET,
                tile.y + RING_INSET,
                tile.size - 2.0 * RING_INSET,
            )
        });
    let route = route::outline(&analysis.points, ROUTE_BOX, ROUTE_MAX_POINTS);
    let route_start = route.first().copied().unwrap_or((
        ROUTE_BOX.x + ROUTE_BOX.w / 2.0,
        ROUTE_BOX.y + ROUTE_BOX.h / 2.0,
    ));
    Mosaic {
        tiles,
        cols,
        rows,
        size,
        fastest_ring,
        route,
        route_start,
    }
}

/// Columns of the grid for `n` tiles: 5 up to 25, 7 up to 42, else 9, so a
/// marathon's 43 tiles fill 5 rows of 9 rather than leave one alone in a
/// seventh row of 7.
fn columns(n: usize) -> u32 {
    match n {
        0..=25 => 5,
        26..=42 => 7,
        _ => 9,
    }
}

/// Tile size that fills the grid width with `cols` columns, smaller if `rows`
/// rows of it would not end by y = 660.
fn tile_size(cols: u32, rows: u32) -> f32 {
    let across = (GRID_WIDTH - GAP * (cols - 1) as f32) / cols as f32;
    if rows == 0 {
        return across;
    }
    let down = (GRID_MAX_HEIGHT + GAP) / rows as f32 - GAP;
    across.min(down)
}

/// Relative luminance (0 black to 1 white) of a `#rrggbb` colour: the sRGB
/// channels made linear and weighted as Rec. 709; black for anything else.
fn luminance(hex: &str) -> f64 {
    let channel = |i: usize| {
        let c = hex
            .get(i..i + 2)
            .and_then(|c| u8::from_str_radix(c, 16).ok())
            .map_or(0.0, |c| f64::from(c) / 255.0);
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(1) + 0.7152 * channel(3) + 0.0722 * channel(5)
}

/// Whether text on `fill` is set in the theme's ink rather than its paper:
/// whichever has the higher contrast ratio with the fill, ink on a tie.
fn ink_text(fill: &str, theme: Theme) -> bool {
    let contrast = |a: f64, b: f64| (a.max(b) + 0.05) / (a.min(b) + 0.05);
    let fill = luminance(fill);
    contrast(fill, luminance(theme.ink)) >= contrast(fill, luminance(theme.paper))
}

/// Font size of the pace on tiles of `size`: a fixed share of the tile, made
/// smaller on a small tile so its figures stay clear of the km number above.
fn pace_size(size: f32) -> f32 {
    let room = size * PACE_BASELINE - (KM_Y + PACE_CLEARANCE);
    (size * PACE_SIZE).min(room / FIGURE_HEIGHT)
}

impl Mosaic {
    /// The block as the template's `mosaic` value, numbers rounded to two
    /// decimals. Every tile carries the positions of its texts: the kilometre
    /// number (`km_x`, `km_y`), the pace centred at `cx` on `pace_y`, and the
    /// partial's distance on `detail_y`.
    pub fn to_value(&self) -> Value {
        let tiles: Vec<Value> = self
            .tiles
            .iter()
            .map(|tile| {
                context! {
                    index => tile.index,
                    x => px(tile.x),
                    y => px(tile.y),
                    size => px(tile.size),
                    color => &tile.color,
                    ink_text => tile.ink_text,
                    pace => &tile.pace,
                    partial => tile.partial,
                    detail => &tile.detail,
                    km_x => px(tile.x + KM_X),
                    km_y => px(tile.y + KM_Y),
                    cx => px(tile.x + tile.size / 2.0),
                    pace_y => px(tile.y + tile.size * PACE_BASELINE),
                    detail_y => px(tile.y + tile.size - DETAIL_ABOVE_BOTTOM),
                }
            })
            .collect();
        context! {
            tiles => tiles,
            cols => self.cols,
            rows => self.rows,
            size => px(self.size),
            pace_size => px(pace_size(self.size)),
            fastest_ring => self
                .fastest_ring
                .map(|(x, y, size)| Value::from(vec![px(x), px(y), px(size)])),
            route_path => route::to_path(&self.route),
            route_start => Value::from(vec![px(self.route_start.0), px(self.route_start.1)]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card;
    use crate::model::{Split, Theme, TrackPoint};
    use crate::variant::Variant;
    use time::macros::date;

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

    /// The fastest full split is the one with the smallest pace.
    fn analysis(splits: Vec<Split>) -> Analysis {
        let fastest = splits
            .iter()
            .filter(|s| !s.partial)
            .min_by(|a, b| a.pace_s_per_km().total_cmp(&b.pace_s_per_km()))
            .map(|s| s.index);
        Analysis {
            distance_m: splits.iter().map(|s| s.distance_m).sum(),
            elapsed_s: splits.iter().map(|s| s.seconds).sum(),
            fastest,
            start: None,
            points: route_points(),
            splits,
        }
    }

    /// `n` full kilometres between 5:00 and 5:48, the fourth in 4:40.
    fn run(n: u32) -> Analysis {
        let pace = |km: u32| {
            if km == 4 {
                280.0
            } else {
                300.0 + f64::from(km * 7 % 13) * 4.0
            }
        };
        analysis((1..=n).map(|km| split(km, 1000.0, pace(km))).collect())
    }

    /// `n` full kilometres and a 460 m partial.
    fn run_with_partial(n: u32) -> Analysis {
        let mut run = run(n);
        run.splits.push(split(n + 1, 460.0, 0.46 * 310.0));
        analysis(run.splits)
    }

    fn mosaic_of(analysis: &Analysis) -> Mosaic {
        let meta = Meta {
            runner: "Julia Brandt".into(),
            event: "Spreelauf".into(),
            city: None,
            date: date!(2026 - 09 - 27),
        };
        card::build(
            analysis,
            &meta,
            &Palette::light(),
            &TextMeasure::new(),
            Variant::Mosaic,
        )
        .mosaic
        .expect("the mosaic block")
    }

    fn assert_near(actual: f32, expected: f32, what: &str) {
        assert!(
            (actual - expected).abs() < 1e-3,
            "{what} was {actual}, expected {expected}"
        );
    }

    /// Bottom edge of the lowest row.
    fn grid_bottom(mosaic: &Mosaic) -> f32 {
        let last = mosaic.tiles.last().expect("a tile");
        last.y + last.size
    }

    #[test]
    fn up_to_25_tiles_take_5_columns_up_to_42_take_7_and_more_take_9() {
        let cases = [
            (1, 5),
            (21, 5),
            (25, 5),
            (26, 7),
            (42, 7),
            (43, 9),
            (49, 9),
            (60, 9),
        ];
        for (n, cols) in cases {
            assert_eq!(columns(n), cols, "{n} tiles");
        }
    }

    #[test]
    fn a_short_run_fills_rows_of_5_wide_tiles_from_the_top_left() {
        let mosaic = mosaic_of(&run(12));
        assert_eq!((mosaic.cols, mosaic.rows), (5, 3));
        let size = (656.0 - 4.0 * 10.0) / 5.0;
        assert_near(mosaic.size, size, "tile size");
        let corner = |i: usize| (mosaic.tiles[i].x, mosaic.tiles[i].y);
        assert_eq!(corner(0), (32.0, 180.0), "first tile");
        assert_near(
            corner(4).0,
            32.0 + 4.0 * (size + 10.0),
            "x of the last of a row",
        );
        assert_near(mosaic.tiles[4].x + size, 688.0, "right edge of the grid");
        assert_near(corner(4).1, 180.0, "y of the last of a row");
        assert_eq!(corner(5).0, 32.0, "x of the first of the next row");
        assert_near(corner(5).1, 180.0 + size + 10.0, "y of the next row");
        for tile in &mosaic.tiles {
            assert_eq!(tile.size, mosaic.size, "km {}", tile.index);
        }
        assert_eq!(mosaic.tiles.len(), 12);
    }

    #[test]
    fn a_grid_that_fits_keeps_the_full_width_tiles() {
        // 15 tiles in 3 rows of 123.2 px, 35 tiles in 5 rows of 85.14 px and
        // 43 tiles in 5 rows of 64 px all end above y = 660.
        for (n, cols, rows) in [(15, 5, 3), (35, 7, 5), (43, 9, 5)] {
            let mosaic = mosaic_of(&run(n));
            assert_eq!((mosaic.cols, mosaic.rows), (cols, rows), "{n} tiles");
            let full = (656.0 - 10.0 * (cols - 1) as f32) / cols as f32;
            assert_near(mosaic.size, full, &format!("size of {n} tiles"));
            assert!(
                grid_bottom(&mosaic) <= 660.0,
                "{n} tiles end at {}",
                grid_bottom(&mosaic)
            );
        }
    }

    #[test]
    fn a_grid_too_tall_for_the_card_shrinks_its_tiles_to_end_at_660() {
        // 21 tiles: 5 rows of 123.2 px would end at y = 836.
        let mosaic = mosaic_of(&run(21));
        assert_eq!((mosaic.cols, mosaic.rows), (5, 5));
        assert_near(mosaic.size, 88.0, "shrunk tile size");
        assert_near(grid_bottom(&mosaic), 660.0, "bottom of the grid");
        // 42 tiles: 6 rows of 85.14 px would end at y = 740.9.
        let mosaic = mosaic_of(&run(42));
        assert_eq!((mosaic.cols, mosaic.rows), (7, 6));
        assert_near(mosaic.size, 490.0 / 6.0 - 10.0, "shrunk tile size");
        assert_near(grid_bottom(&mosaic), 660.0, "bottom of the grid");
        // 60 tiles: 7 rows of 64 px would end at y = 688.
        let mosaic = mosaic_of(&run(60));
        assert_eq!((mosaic.cols, mosaic.rows), (9, 7));
        assert_near(grid_bottom(&mosaic), 660.0, "bottom of the grid");
    }

    #[test]
    fn a_marathon_is_a_grid_of_9_by_5_full_width_tiles() {
        let mosaic = mosaic_of(&run_with_partial(42));
        assert_eq!((mosaic.cols, mosaic.rows), (9, 5));
        assert_near(mosaic.size, 64.0, "tile size");
        assert_near(mosaic.tiles[0].x, 32.0, "left edge of the grid");
        assert_near(mosaic.tiles[8].x + 64.0, 688.0, "right edge of the grid");
        let last = mosaic.tiles.last().unwrap();
        assert_eq!(last.index, 43);
        assert_near(last.x, 32.0 + 6.0 * 74.0, "x of the partial");
        assert_near(last.y, 180.0 + 4.0 * 74.0, "y of the partial");
    }

    #[test]
    fn a_shrunk_grid_keeps_its_columns_and_square_tiles_with_10_px_gaps() {
        let mosaic = mosaic_of(&run_with_partial(54));
        assert_eq!((mosaic.cols, mosaic.rows), (9, 7));
        let size = mosaic.size;
        assert_near(size, 60.0, "tile size");
        let (first, next) = (&mosaic.tiles[0], &mosaic.tiles[1]);
        assert_near(next.x - first.x, size + 10.0, "column step");
        let below = &mosaic.tiles[9];
        assert_near(below.x, first.x, "x of the next row");
        assert_near(below.y - first.y, size + 10.0, "row step");
        assert_near(first.y, 180.0, "top of the grid");
        let last = mosaic.tiles.last().unwrap();
        assert_eq!(
            (last.index, last.x),
            (55, first.x),
            "the partial opens row 7"
        );
    }

    #[test]
    fn a_shrunk_grid_is_centred_across_the_card() {
        // 9 columns of 60 px and 8 gaps are 620 px wide: 18 px each side.
        let mosaic = mosaic_of(&run_with_partial(54));
        assert_near(mosaic.tiles[0].x, 50.0, "left edge of the grid");
        assert_near(
            mosaic.tiles[8].x + mosaic.size,
            670.0,
            "right edge of the grid",
        );
        // A grid of full-width tiles spans x = 32 to 688.
        let mosaic = mosaic_of(&run(35));
        assert_near(mosaic.tiles[0].x, 32.0, "left edge of a full grid");
        assert_near(
            mosaic.tiles[6].x + mosaic.size,
            688.0,
            "right edge of a full grid",
        );
    }

    #[test]
    fn every_tile_has_its_kilometre_colour_and_pace() {
        let analysis = run_with_partial(9);
        let mosaic = mosaic_of(&analysis);
        let card = card::build(
            &analysis,
            &Meta {
                runner: "Julia Brandt".into(),
                event: "Spreelauf".into(),
                city: None,
                date: date!(2026 - 09 - 27),
            },
            &Palette::light(),
            &TextMeasure::new(),
            Variant::Classic,
        );
        assert_eq!(mosaic.tiles.len(), 10);
        for (tile, view) in mosaic.tiles.iter().zip(&card.splits) {
            assert_eq!(
                (tile.index, &tile.color, &tile.pace, tile.partial),
                (view.index, &view.color, &view.pace, view.partial)
            );
        }
        assert_eq!(mosaic.tiles[3].pace, "4:40");
    }

    #[test]
    fn the_luminance_of_the_palette_stops_runs_from_dark_slate_to_light_cream() {
        let stops = ["#2c4a5e", "#9fb6c3", "#f3e3c8", "#f6a43a", "#e8601c"];
        let expected = [0.0624, 0.4477, 0.7816, 0.4645, 0.2561];
        for (stop, l) in stops.iter().zip(expected) {
            assert!(
                (luminance(stop) - l).abs() < 5e-4,
                "{stop}: {}",
                luminance(stop)
            );
        }
        assert_eq!(luminance("#000000"), 0.0);
        assert!((luminance("#ffffff") - 1.0).abs() < 1e-12);
    }

    #[test]
    fn text_on_a_tile_takes_the_theme_colour_that_contrasts_more_with_it() {
        let theme = Theme::light();
        let stops = ["#2c4a5e", "#9fb6c3", "#f3e3c8", "#f6a43a", "#e8601c"];
        let ink: Vec<bool> = stops.iter().map(|stop| ink_text(stop, theme)).collect();
        // Paper on the light slate #9fb6c3 would be 1.8:1, on the orange 3:1;
        // ink is 8.2:1 and 5:1.
        assert_eq!(ink, [false, true, true, true, true]);
    }

    #[test]
    fn ink_takes_over_where_both_text_colours_contrast_equally() {
        // Ink and paper of the light theme contrast equally at a luminance of
        // 0.1864: #777777 is 0.1845, #787878 0.1878.
        let theme = Theme::light();
        assert!(!ink_text("#777777", theme), "just darker than the edge");
        assert!(ink_text("#787878", theme), "just lighter than the edge");
        assert!(!ink_text("#000000", theme) && ink_text("#ffffff", theme));
    }

    #[test]
    fn the_mosaic_sets_its_text_by_contrast_with_each_tile() {
        let mosaic = mosaic_of(&run_with_partial(9));
        for tile in &mosaic.tiles {
            assert_eq!(
                tile.ink_text,
                ink_text(&tile.color, Theme::light()),
                "km {}",
                tile.index
            );
        }
        let slow = mosaic.tiles.iter().find(|t| t.color == "#2c4a5e");
        assert!(!slow.expect("a slowest tile").ink_text);
    }

    #[test]
    fn small_tiles_set_the_pace_smaller_so_it_clears_the_km_number() {
        // From 88 px down to about 71 px the pace is 0.42 of the tile.
        assert_near(pace_size(88.0), 36.96, "pace of an 88 px tile");
        assert_near(pace_size(85.0), 35.7, "pace of an 85 px tile");
        // Below, its figures (0.7 em tall) end 4 px under the km number's
        // baseline at y + 20, with the baseline staying at 0.62 of the tile.
        assert_near(pace_size(60.0), (37.2 - 24.0) / 0.7, "pace of a 60 px tile");
        assert_near(
            pace_size(64.0),
            (39.68 - 24.0) / 0.7,
            "pace of a 64 px tile",
        );
    }

    #[test]
    fn the_partial_tile_shows_its_distance_and_full_tiles_none() {
        let mosaic = mosaic_of(&run_with_partial(5));
        let last = mosaic.tiles.last().unwrap();
        assert!(last.partial);
        assert_eq!(last.detail.as_deref(), Some("0.46 km"));
        assert!(mosaic.tiles[..5].iter().all(|t| t.detail.is_none()));
    }

    #[test]
    fn the_fastest_full_kilometre_gets_a_ring_inset_4_px() {
        let mosaic = mosaic_of(&run_with_partial(9));
        let tile = &mosaic.tiles[3];
        let (x, y, size) = mosaic.fastest_ring.expect("a ring");
        assert_near(x, tile.x + 4.0, "ring x");
        assert_near(y, tile.y + 4.0, "ring y");
        assert_near(size, tile.size - 8.0, "ring size");
    }

    #[test]
    fn a_run_under_a_kilometre_is_one_tile_without_a_ring() {
        let mosaic = mosaic_of(&analysis(vec![split(1, 600.0, 180.0)]));
        assert_eq!(mosaic.tiles.len(), 1);
        assert_eq!(mosaic.fastest_ring, None);
        assert_eq!(mosaic.tiles[0].detail.as_deref(), Some("0.60 km"));
    }

    #[test]
    fn the_route_is_fitted_into_the_box_right_of_the_name_and_starts_at_its_first_point() {
        let mosaic = mosaic_of(&run(10));
        assert_eq!(mosaic.route.len(), route_points().len());
        for &(x, y) in &mosaic.route {
            assert!(
                (564.0 - 1e-3..=684.0 + 1e-3).contains(&x)
                    && (668.0 - 1e-3..=758.0 + 1e-3).contains(&y),
                "({x}, {y}) is outside the route box"
            );
        }
        assert_eq!(mosaic.route_start, mosaic.route[0]);
        let without_points = Analysis {
            points: Vec::new(),
            ..run(10)
        };
        let mosaic = mosaic_of(&without_points);
        assert!(mosaic.route.is_empty());
        assert_eq!(mosaic.route_start, (624.0, 713.0));
    }

    #[test]
    fn the_name_fits_the_classic_line_because_the_route_box_keeps_its_x() {
        assert_eq!(ROUTE_BOX.x, card::ROUTE_BOX.x);
    }

    #[test]
    fn the_value_places_the_tile_texts_and_rounds_to_two_decimals() {
        let mosaic = Mosaic {
            tiles: vec![Tile {
                index: 43,
                x: 32.0,
                y: 600.004,
                size: 60.006,
                color: "#e8601c".into(),
                ink_text: false,
                pace: "5:10".into(),
                partial: true,
                detail: Some("0.20 km".into()),
            }],
            cols: 7,
            rows: 7,
            size: 60.006,
            fastest_ring: Some((36.0, 184.004, 52.006)),
            route: vec![(564.123, 700.0), (600.0, 668.0)],
            route_start: (564.123, 700.0),
        };
        let value = mosaic.to_value();
        let attr = |value: &Value, name: &str| value.get_attr(name).unwrap();
        let tile = attr(&value, "tiles").get_item(&Value::from(0)).unwrap();
        let text = |name: &str| attr(&tile, name).to_string();
        assert_eq!(
            [text("index"), text("x"), text("y"), text("size")],
            ["43", "32.0", "600.0", "60.01"]
        );
        assert_eq!(attr(&tile, "color").as_str(), Some("#e8601c"));
        assert!(!attr(&tile, "ink_text").is_true());
        assert!(attr(&tile, "partial").is_true());
        assert_eq!(attr(&tile, "pace").as_str(), Some("5:10"));
        assert_eq!(attr(&tile, "detail").as_str(), Some("0.20 km"));
        // Km number at (x + 10, y + 20), pace centred at y + 0.62 size, the
        // detail 10 px above the bottom.
        assert_eq!([text("km_x"), text("km_y")], ["42.0", "620.0"]);
        assert_eq!([text("cx"), text("pace_y")], ["62.0", "637.21"]);
        assert_eq!(text("detail_y"), "650.01");
        assert_eq!(attr(&value, "pace_size").to_string(), "18.86");
        let ring = attr(&value, "fastest_ring");
        let ring: Vec<String> = (0..3)
            .map(|i| ring.get_item(&Value::from(i)).unwrap().to_string())
            .collect();
        assert_eq!(ring, ["36.0", "184.0", "52.01"]);
        assert_eq!(
            attr(&value, "route_path").as_str(),
            Some("M 564.1 700.0 L 600.0 668.0")
        );
        let start = attr(&value, "route_start");
        assert_eq!(
            start.get_item(&Value::from(0)).unwrap().to_string(),
            "564.12"
        );
    }
}
