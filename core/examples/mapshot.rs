//! Renders a real coverage map to a PNG, for looking at the renderer.
//! cargo run --release -p hfpredict-core --example mapshot -- out.png [band-mhz] [hour]
use hfpredict_core::bands::Band;
use hfpredict_core::coverage;
use hfpredict_core::engine::{Engine, Model, RunContext};
use hfpredict_core::geo::{LatLon, UtcDate, days_from_civil};
use hfpredict_core::maps;
use hfpredict_core::projection::MapView;
use hfpredict_core::render::{self, Palette, Scene};
use hfpredict_core::station::Station;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = args.get(1).cloned().unwrap_or("map.png".into());
    let band = args
        .get(2)
        .and_then(|s| s.parse().ok())
        .and_then(Band::from_mhz)
        .unwrap_or(Band::M20);
    let hour: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(12);
    let from = LatLon {
        lat: 39.47,
        lon: -0.38,
    };
    let engine = Engine::new(std::env::temp_dir().join("hfp-mapshot"));
    let ctx = RunContext {
        station: Station::default(),
        model: Model::Truecast,
        date: UtcDate {
            year: 2026,
            month: 10,
            day: 25,
            hour,
        },
        nowcast: None,
    };
    let threads = hfpredict_core::parallel::cores();
    let t = Instant::now();
    let coarse = maps::coarse(&engine, &ctx, from, hour).unwrap();
    let centres = maps::centres(
        &engine,
        &ctx,
        from,
        coverage::FINE_CENTRE_LAT_STEP,
        coverage::FINE_CENTRE_LON_STEP,
        Some(band),
        threads,
    )
    .unwrap();
    let fine = maps::fine(&engine, &ctx, from, hour, band, threads).unwrap();
    let grid = fine.corrected(centres[0].1.as_ref(), 24.0, None);
    eprintln!(
        "compute {} ms, reach {:.0}%",
        t.elapsed().as_millis(),
        grid.reach() * 100.0
    );
    let coarse_grid = coarse
        .iter()
        .find(|(b, _)| *b == band)
        .unwrap()
        .1
        .grid
        .clone();
    let palette = Palette {
        disc: 0xFF1B1D24,
        reliable: 0xFFD0BCFF,
        patchy: 0xFF9A82DB,
        weak: 0xFF5B3F99,
        night: 0x70000000,
        coastline: 0xDDE6E1E5,
        grid_lines: 0x66E6E1E5,
        path: 0xFF4DD0E1,
        station: 0xFFFFC107,
        halo: 0xFF1B1D24,
        nvis: 0xFFFFFFFF,
    };
    let unix = days_from_civil(2026, 10, 25) * 86_400 + i64::from(hour) * 3600;
    let scene = Scene {
        centre: from,
        destination: Some(LatLon {
            lat: 35.7,
            lon: 139.7,
        }),
        unix_secs: unix,
        layers: vec![&coarse_grid, &grid],
        show_night: true,
        show_nvis: true,
    };
    let size = 900;
    let t = Instant::now();
    let view = if args.iter().any(|a| a == "--fit") {
        render::fit_reach(from, &grid)
    } else {
        MapView::default()
    };
    let px = render::render(&scene, view, size, 2.0, &palette, threads);
    eprintln!("render {} ms at {size}px", t.elapsed().as_millis());
    let pm =
        tiny_skia::Pixmap::from_vec(px, tiny_skia::IntSize::from_wh(size, size).unwrap()).unwrap();
    pm.save_png(&out).unwrap();
}
