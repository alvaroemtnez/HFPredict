//! Colours for the map and the band grid, taken from the desktop: the
//! reach shades are the user's accent colour at three lightnesses, and the
//! rest follows light, dark or low-light.

use gtk::gdk;
use hfpredict_core::quality::Quality;
use hfpredict_core::render::Palette;

#[derive(Clone, Copy, Debug)]
pub struct Colors {
    pub map: Palette,
    /// Closed, weak, patchy, reliable: the band grid's cells.
    pub cells: [gdk::RGBA; 4],
    pub fg: gdk::RGBA,
    pub dim: gdk::RGBA,
    /// The "now" marker.
    pub now: gdk::RGBA,
    pub dark: bool,
    pub low_light: bool,
}

impl Colors {
    pub fn cell(&self, q: Quality) -> gdk::RGBA {
        self.cells[q as usize]
    }

    /// A quality as a traffic light (green to red, grey unknown).
    pub fn semaphore(&self, q: Option<Quality>) -> gdk::RGBA {
        rgba_of(hfpredict_core::render::semaphore(
            q,
            self.dark,
            self.low_light,
        ))
    }
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h / 60.0).rem_euclid(6.0);
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    (r + m, g + m, b + m)
}

fn hue_sat(c: &gdk::RGBA) -> (f32, f32) {
    let (r, g, b) = (c.red(), c.green(), c.blue());
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let l = (max + min) / 2.0;
    if d < 1e-6 {
        return (0.0, 0.0);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(1e-6);
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s)
}

fn rgba(r: f32, g: f32, b: f32, a: f32) -> gdk::RGBA {
    gdk::RGBA::new(r, g, b, a)
}

fn shade(h: f32, s: f32, l: f32) -> gdk::RGBA {
    let (r, g, b) = hsl_to_rgb(h, s, l);
    rgba(r, g, b, 1.0)
}

fn argb(c: &gdk::RGBA) -> u32 {
    let ch = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (ch(c.alpha()) << 24) | (ch(c.red()) << 16) | (ch(c.green()) << 8) | ch(c.blue())
}

pub fn colors(accent: gdk::RGBA, dark: bool, low_light: bool) -> Colors {
    if low_light {
        // Dim reds on black: the eye's rods barely see red, so night
        // vision survives a glance at the screen.
        let red = |l: f32| shade(4.0, 0.85, l);
        let cells = [
            rgba(0.35, 0.05, 0.03, 0.18),
            red(0.16),
            red(0.30),
            red(0.48),
        ];
        let fg = rgba(0.85, 0.25, 0.18, 1.0);
        let dim = rgba(0.6, 0.16, 0.12, 1.0);
        return Colors {
            map: Palette {
                disc: 0xFF0A0101,
                weak: argb(&cells[1]),
                patchy: argb(&cells[2]),
                reliable: argb(&cells[3]),
                night: 0x66000000,
                coastline: 0xCC9A2A1E,
                grid_lines: 0x664A120C,
                path: 0xFFFF6A50,
                path_halo: 0xFF000000,
                station: 0xFFFF8A65,
                halo: 0xFF000000,
                nvis: 0xFFFF8A65,
            },
            cells,
            fg,
            dim,
            now: rgba(1.0, 0.45, 0.3, 1.0),
            dark: true,
            low_light: true,
        };
    }
    let highlight = hfpredict_core::render::highlight(argb(&accent), dark, false);
    let (h, s) = hue_sat(&accent);
    // A grey accent still needs three readable steps.
    let s = s.clamp(0.35, 0.85);
    if dark {
        let cells = [
            rgba(1.0, 1.0, 1.0, 0.06),
            shade(h, s * 0.75, 0.36),
            shade(h, s, 0.58),
            shade(h, s, 0.82),
        ];
        Colors {
            map: Palette {
                disc: 0xFF18181B,
                weak: argb(&cells[1]),
                patchy: argb(&cells[2]),
                reliable: argb(&cells[3]),
                night: 0x5C000000,
                coastline: 0xC8E8E8EE,
                grid_lines: 0x50FFFFFF,
                path: highlight.line,
                path_halo: highlight.halo,
                station: 0xFFF6D32D,
                halo: 0xFF18181B,
                nvis: 0xFFFFFFFF,
            },
            cells,
            fg: rgba(1.0, 1.0, 1.0, 1.0),
            dim: rgba(1.0, 1.0, 1.0, 0.55),
            now: rgba(0.96, 0.83, 0.18, 1.0),
            dark: true,
            low_light: false,
        }
    } else {
        let cells = [
            rgba(0.0, 0.0, 0.1, 0.06),
            shade(h, s * 0.8, 0.80),
            shade(h, s, 0.56),
            shade(h, s, 0.32),
        ];
        Colors {
            map: Palette {
                disc: 0xFFF2F2F5,
                weak: argb(&cells[1]),
                patchy: argb(&cells[2]),
                reliable: argb(&cells[3]),
                night: 0x24000020,
                coastline: 0xB0303038,
                grid_lines: 0x50000010,
                path: highlight.line,
                path_halo: highlight.halo,
                station: 0xFFE66100,
                halo: 0xFFFFFFFF,
                nvis: 0xFF000000,
            },
            cells,
            fg: rgba(0.0, 0.0, 0.0, 0.8),
            dim: rgba(0.0, 0.0, 0.0, 0.55),
            now: rgba(0.90, 0.38, 0.0, 1.0),
            dark: false,
            low_light: false,
        }
    }
}

/// The colours for the window's current style.
pub fn current(low_light: bool) -> Colors {
    let sm = adw::StyleManager::default();
    colors(sm.accent_color_rgba(), sm.is_dark(), low_light)
}

pub fn css(c: &gdk::RGBA) -> String {
    format!(
        "rgba({},{},{},{})",
        (c.red() * 255.0).round(),
        (c.green() * 255.0).round(),
        (c.blue() * 255.0).round(),
        c.alpha()
    )
}

/// An 0xAARRGGBB colour as GDK's.
pub fn rgba_of(c: u32) -> gdk::RGBA {
    let ch = |shift: u32| ((c >> shift) & 255) as f32 / 255.0;
    gdk::RGBA::new(ch(16), ch(8), ch(0), ch(24))
}
