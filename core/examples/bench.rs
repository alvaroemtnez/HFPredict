//! Times each step of a map: `cargo run --release --example bench`.

use std::time::Instant;

use hfpredict_core::Core;
use hfpredict_core::app::MapOptions;
use hfpredict_core::bands::Band;
use hfpredict_core::places::Place;
use hfpredict_core::projection::MapView;
use hfpredict_core::render::Palette;

fn main() {
    let dir = std::env::temp_dir().join("hfp-bench");
    let _ = std::fs::remove_dir_all(&dir);
    let core = Core::open(dir.join("data"), dir.join("cache"));
    core.set_from(Place::at("Valencia", "Spain", 39.47, -0.38))
        .unwrap();
    let now = hfpredict_core::geo::days_from_civil(2026, 10, 25) * 86_400 + 12 * 3600;
    let t = |what: &str, f: &mut dyn FnMut()| {
        let t0 = Instant::now();
        f();
        println!("{what:<28} {:>7.1} ms", t0.elapsed().as_secs_f64() * 1000.0);
    };
    t("local time, first lookup", &mut || {
        hfpredict_core::localtime::clock(39.47, -0.38, now);
    });
    t("local time, 24 more", &mut || {
        for h in 0..24 {
            hfpredict_core::localtime::clock(39.47, -0.38, now + h * 3600);
        }
    });
    t("forecast (survey)", &mut || {
        core.forecast(now).unwrap();
    });
    for h in [12, 13] {
        t(&format!("coarse maps h{h}"), &mut || {
            core.coarse_maps(h, now).unwrap();
        });
        t(&format!("fine map 20m h{h}"), &mut || {
            core.fine_map(Band::M20, h, now).unwrap();
        });
    }
    let zoom = MapView {
        scale: 4.0,
        cx: 0.5,
        cy: 0.5,
    };
    t("patch (zoom 4)", &mut || {
        core.patch_map(12, now, zoom).unwrap();
    });
    let palette = Palette {
        disc: 0xFF101418,
        reliable: 0xFFD0BCFF,
        patchy: 0xFF9A82DB,
        weak: 0xFF4F378B,
        night: 0x66000000,
        coastline: 0xCCFFFFFF,
        grid_lines: 0x55FFFFFF,
        path: 0xFFF6D32D,
        path_halo: 0xE0000000,
        station: 0xFFFFC107,
        halo: 0xFF000000,
        nvis: 0xFFFFFFFF,
    };
    let opts = MapOptions {
        show_night: true,
        show_nvis: true,
        show_path: true,
        highlight: None,
    };
    for size in [540u32, 1080, 1440] {
        for (name, view) in [("globe", MapView::default()), ("zoom", zoom)] {
            t(&format!("render {size}px {name}"), &mut || {
                core.render_map(Band::M20, 12, now, view, size, 2.6, palette, opts.clone());
            });
            t("  same view, next hour", &mut || {
                core.render_map(Band::M20, 13, now, view, size, 2.6, palette, opts.clone());
            });
        }
    }
    println!(
        "threads: {}",
        std::thread::available_parallelism().map_or(1, |n| n.get())
    );
}
