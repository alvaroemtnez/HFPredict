//! The map as tiles, the way map apps draw: the disc's bounding square is
//! cut into a pyramid (level `z` has 2^z by 2^z tiles), each tile rendered
//! once at [`TILE_PX`] and kept by the app as a texture. A pan or a pinch
//! only moves textures; tiles the new view lacks are rendered one by one,
//! centre first, while a coarser tile already held stands in.
//!
//! A tile is the ordinary render with a view that frames it, so a tiled map
//! is pixel for pixel the map the single-image renderer drew.

use crate::projection::{MAX_SCALE, MIN_SCALE, MapView};

/// Side of a rendered tile, pixels.
pub const TILE_PX: u32 = 512;

/// One tile of the pyramid.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Tile {
    pub z: u32,
    pub x: u32,
    pub y: u32,
}

/// Where a tile goes on screen, pixels from the map area's top-left corner.
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct TilePlace {
    pub tile: Tile,
    pub left: f64,
    pub top: f64,
    pub size: f64,
}

/// The map on screen: the disc's square is `side` pixels at the whole-earth
/// view and its corner sits at (`x0`, `y0`) in a `width` by `height` area
/// (the square can be larger than the area, cut off at the sides).
#[derive(Clone, Copy, Debug, PartialEq)]
#[cfg_attr(feature = "ffi", derive(uniffi::Record))]
pub struct Screen {
    pub side: f64,
    pub x0: f64,
    pub y0: f64,
    pub width: f64,
    pub height: f64,
}

impl Tile {
    pub const WORLD: Tile = Tile { z: 0, x: 0, y: 0 };

    fn count(self) -> f64 {
        f64::from(1u32 << self.z)
    }

    /// The view that frames exactly this tile.
    pub fn view(self) -> MapView {
        let n = self.count();
        MapView {
            scale: n,
            cx: (f64::from(self.x) + 0.5) / n,
            cy: (f64::from(self.y) + 0.5) / n,
        }
    }

    /// The tile one level up that contains this one.
    pub fn parent(self) -> Option<Tile> {
        (self.z > 0).then(|| Tile {
            z: self.z - 1,
            x: self.x / 2,
            y: self.y / 2,
        })
    }

    /// Where the tile is drawn for a view.
    pub fn place(self, view: MapView, screen: &Screen) -> TilePlace {
        let n = self.count();
        let k = screen.side * view.scale;
        let origin = (view.cx - 0.5 / view.scale, view.cy - 0.5 / view.scale);
        TilePlace {
            tile: self,
            left: screen.x0 + (f64::from(self.x) / n - origin.0) * k,
            top: screen.y0 + (f64::from(self.y) / n - origin.1) * k,
            size: k / n,
        }
    }
}

/// The view kept so the screen never leaves the disc's square: the same as
/// `MapView::contained` when the screen is the square, but a screen cut
/// from a larger square (full screen on a phone) can pan across it even at
/// the whole-earth view.
pub fn contain(view: MapView, screen: &Screen) -> MapView {
    let scale = view.scale.clamp(MIN_SCALE, MAX_SCALE);
    let k = screen.side * scale;
    let clamp = |c: f64, extent: f64| {
        let half = extent / (2.0 * k);
        if half >= 0.5 {
            0.5
        } else {
            c.clamp(half, 1.0 - half)
        }
    };
    MapView {
        scale,
        cx: clamp(view.cx, screen.width.min(screen.side)),
        cy: clamp(view.cy, screen.height.min(screen.side)),
    }
}

/// A drag of (`dx`, `dy`) screen pixels.
pub fn panned(view: MapView, dx: f64, dy: f64, screen: &Screen) -> MapView {
    let k = screen.side * view.scale;
    contain(
        MapView {
            cx: view.cx - dx / k,
            cy: view.cy - dy / k,
            ..view
        },
        screen,
    )
}

/// A zoom by `factor` about the screen pixel (`x`, `y`), which stays still.
pub fn zoomed(view: MapView, factor: f64, x: f64, y: f64, screen: &Screen) -> MapView {
    let scale = (view.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
    let (k0, k1) = (screen.side * view.scale, screen.side * scale);
    // The square's point under (x, y) before and after.
    let origin = |cx: f64, k: f64, x0: f64| cx - screen.side / (2.0 * k) - x0 / k;
    let ux = origin(view.cx, k0, screen.x0) + x / k0;
    let uy = origin(view.cy, k0, screen.y0) + y / k0;
    let cx = ux - x / k1 + screen.side / (2.0 * k1) + screen.x0 / k1;
    let cy = uy - y / k1 + screen.side / (2.0 * k1) + screen.y0 / k1;
    contain(MapView { scale, cx, cy }, screen)
}

/// The deepest level the map uses: tiles at the closest zoom.
pub fn max_level(screen: &Screen) -> u32 {
    level_for(
        MapView {
            scale: MAX_SCALE,
            cx: 0.5,
            cy: 0.5,
        },
        screen,
    )
}

/// The level whose tiles match the screen: drawn between about 0.7 and 1.4
/// times their own size, so they stay sharp without wasting pixels.
pub fn level_for(view: MapView, screen: &Screen) -> u32 {
    let shown = screen.side * view.scale / f64::from(TILE_PX);
    shown.max(1.0).log2().round().clamp(0.0, 12.0) as u32
}

/// Every tile of level `z` that shows in the view, nearest the middle of
/// the screen first (the order to render missing ones in).
pub fn visible(view: MapView, screen: &Screen, z: u32) -> Vec<TilePlace> {
    let n = 1u32 << z;
    let k = screen.side * view.scale;
    let origin = (view.cx - 0.5 / view.scale, view.cy - 0.5 / view.scale);
    // The screen area in the square's units (0..1 across the square).
    let to_unit = |px: f64, o: f64, start: f64| start + (px - o) / k;
    let (u0, u1) = (
        to_unit(0.0, screen.x0, origin.0),
        to_unit(screen.width, screen.x0, origin.0),
    );
    let (v0, v1) = (
        to_unit(0.0, screen.y0, origin.1),
        to_unit(screen.height, screen.y0, origin.1),
    );
    let index = |u: f64| (u * f64::from(n)).floor().clamp(0.0, f64::from(n - 1)) as u32;
    let mut out = vec![];
    for y in index(v0)..=index(v1) {
        for x in index(u0)..=index(u1) {
            out.push(Tile { z, x, y }.place(view, screen));
        }
    }
    let (mx, my) = (screen.width / 2.0, screen.height / 2.0);
    out.sort_by(|a, b| {
        let d = |t: &TilePlace| (t.left + t.size / 2.0 - mx).hypot(t.top + t.size / 2.0 - my);
        d(a).total_cmp(&d(b))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(side: f64) -> Screen {
        Screen {
            side,
            x0: 0.0,
            y0: 0.0,
            width: side,
            height: side,
        }
    }

    #[test]
    fn whole_earth_is_one_tile_on_a_small_screen() {
        let s = screen(500.0);
        let v = MapView::default();
        assert_eq!(level_for(v, &s), 0);
        let t = visible(v, &s, 0);
        assert_eq!(t.len(), 1);
        assert_eq!((t[0].left, t[0].top, t[0].size), (0.0, 0.0, 500.0));
    }

    #[test]
    fn zooming_picks_finer_tiles_that_cover_the_screen() {
        let s = screen(1000.0);
        let v = MapView {
            scale: 4.0,
            cx: 0.3,
            cy: 0.6,
        };
        let z = level_for(v, &s);
        assert_eq!(z, 3);
        let tiles = visible(v, &s, z);
        // Every corner and the middle of the screen fall in some tile.
        for (px, py) in [
            (0.0, 0.0),
            (999.0, 0.0),
            (0.0, 999.0),
            (999.0, 999.0),
            (500.0, 500.0),
        ] {
            assert!(
                tiles.iter().any(|t| px >= t.left
                    && px < t.left + t.size
                    && py >= t.top
                    && py < t.top + t.size),
                "({px}, {py}) uncovered"
            );
        }
        // The first is the one under the middle.
        let m = tiles[0];
        assert!(m.left <= 500.0 && m.left + m.size > 500.0);
    }

    #[test]
    fn a_tile_view_frames_the_tile() {
        let t = Tile { z: 2, x: 3, y: 1 };
        let v = t.view();
        assert_eq!(v.scale, 4.0);
        let s = screen(512.0);
        let p = t.place(v, &s);
        assert!((p.left).abs() < 1e-9 && (p.top).abs() < 1e-9 && (p.size - 512.0).abs() < 1e-9);
        assert_eq!(t.parent(), Some(Tile { z: 1, x: 1, y: 0 }));
    }

    #[test]
    fn a_cut_screen_pans_and_zooms_about_the_finger() {
        let s = Screen {
            side: 2400.0,
            x0: -660.0,
            y0: 0.0,
            width: 1080.0,
            height: 2400.0,
        };
        // The whole earth: across, but not up and down.
        let v = panned(MapView::default(), -300.0, -300.0, &s);
        assert!(v.cx > 0.5 && (v.cy - 0.5).abs() < 1e-12, "{v:?}");
        // Never past the square's edge.
        let far = panned(MapView::default(), -1e6, 0.0, &s);
        assert!((far.cx - (1.0 - 1080.0 / 4800.0)).abs() < 1e-9);
        // The point under the finger stays put.
        let z = zoomed(MapView::default(), 2.0, 540.0, 1200.0, &s);
        assert!((z.cx - 0.5).abs() < 1e-9 && (z.cy - 0.5).abs() < 1e-9);
        // In a square screen it is the old arithmetic.
        let sq = screen(800.0);
        let a = zoomed(MapView::default(), 3.0, 200.0, 300.0, &sq);
        let b = MapView::default().zoomed_at(3.0, 200.0, 300.0, 800.0);
        assert!((a.cx - b.cx).abs() < 1e-9 && (a.cy - b.cy).abs() < 1e-9);
    }

    #[test]
    fn a_wide_screen_cuts_the_square() {
        // Full screen on a phone: the square is as tall as the screen.
        let s = Screen {
            side: 2400.0,
            x0: -660.0,
            y0: 0.0,
            width: 1080.0,
            height: 2400.0,
        };
        let v = MapView::default();
        let z = level_for(v, &s);
        let tiles = visible(v, &s, z);
        assert!(
            tiles
                .iter()
                .all(|t| t.left + t.size > 0.0 && t.left < 1080.0)
        );
    }
}
