# Architecture

```
            ┌──────────────────────────── core/ (Rust) ────────────────────────────┐
            │  hfcast engine (git dep, pinned)  ←  engine.rs: one request builder  │
            │  forecast.rs  path + survey      maps.rs  coarse / fine / patch      │
            │  correct.rs   WSPR-fitted swing and spread correction (from HFcast)  │
            │  render.rs    the map, drawn to RGBA with tiny-skia                  │
            │  places.rs  online.rs  store.rs  cache.rs                            │
            │  app.rs  `Core`: the one object both apps call, thread-safe          │
            └───────────────┬───────────────────────────────────┬──────────────────┘
                            │ Rust                              │ UniFFI (ffi/)
                 ┌──────────┴──────────┐             ┌──────────┴──────────┐
                 │ gnome/  GTK 4 +     │             │ android/  Kotlin +  │
                 │ libadwaita (Rust)   │             │ Compose Material 3  │
                 └─────────────────────┘             └─────────────────────┘
```

## One core, two thin apps

Everything that decides a number lives in `core/`: requests to the engine, the
correction HFcast fitted against WSPR reports, the survey of directions, the
coverage lattices, the patch arithmetic, place search, the online readings, the
saved state and the map image itself. The two apps only lay out widgets, handle
gestures and pick colours from the platform theme. A feature appears on both
platforms or neither, and a number can never differ between them.

`Core` (`core/src/app.rs`) is the API. Its methods are blocking and safe to call
from any thread; the apps call them off the UI thread (coroutines on Android,
`gio::spawn_blocking` on GNOME) and apply results on the UI thread, discarding
any that a newer request has overtaken.

## The engine

`hfcast` comes from the owner's mirror of
[hfcast-engine](https://github.com/jonathanmcsweet/hfcast-engine), pinned to
the commit of tag v2.14.0 in the workspace `Cargo.toml`. It is used unmodified,
through its JSON entry point `hfcast::service::run`, the same one HFcast uses.
The `embedded-coefficients` feature compiles the ionospheric maps in; they are
only in the repository (not the crates.io package), which is why the dependency
is a git one. See [decisions/0002-engine.md](decisions/0002-engine.md).

The engine is extensively tested by its author (300+ unit tests, and a parity
check against the Fortran over 23,040 cells), so HFPredict does not retest it.
`core/` tests what HFPredict adds: the ported correction, lattices, parsers,
rendering, persistence and the request plumbing (`tools/rust.sh cargo test`).

## How a map is made

1. **Coarse** (15° by 22.5°, 192 points, every band in one engine run): drawn at
   once, and the source of every band's dot on the band chips.
2. **Daily middles**: a whole-day run on a lattice (coarse for the coarse map,
   5° by 7.5° for one band) that the correction shrinks each hour toward.
3. **Fine** (1.25° by 1.5°, 34,560 points, one band): Truecast threads inside
   the engine; VOACAP's area driver is serial, so the core cuts the world into
   latitude strips that produce exactly the same cells (tested), one per thread.
4. **Patch**: when zoomed in, up to 2,000 points around the view at a finer step.
5. **Render**: per pixel, the inverse projection finds the cell; the finest grid
   that covers a pixel wins. Night wash, coastlines, distance rings and the path
   are drawn on top. Gestures move the previous image at once; the new one
   follows.

Fine grids are kept in memory and (Preferences → Keep computed maps) on disk,
quantised to 69 KB each, within 128 MB.

## Strings

`i18n/<lang>.json` is the single source for every language. The GNOME app reads
it directly (`gnome/src/i18n.rs`); `tools/gen-strings.py` generates Android's
`res/values*/strings.xml` and checks that placeholders agree. Placeholders are
named (`{band}`), so translations can reorder them.

## Builds

| What | Where | Notes |
| --- | --- | --- |
| Rust toolchain | `.tools/` via `tools/rust.sh` | pinned in `rust-toolchain.toml` |
| Android | `android/build.sh` | JDK, SDK, NDK, cargo-ndk into `android/.tools/`; Rust for 3 ABIs, then Gradle |
| GNOME dev | `gnome/build.sh` | podman, Debian 13 (GTK 4.18, libadwaita 1.7) |
| Flatpak | `packaging/flatpak/build.sh` | vendored crates, Flathub's GNOME 51 image |
| AppImage | `packaging/appimage/build.sh` | linuxdeploy + GTK plugin in the Debian 13 container |

GTK 4.18 and libadwaita 1.7 are the oldest supported (Cargo features `v4_18`,
`v1_7`), so the AppImage can bundle what Debian 13 ships.

## UI tests

- GNOME: `gnome/build.sh screenshot [scenario…]` runs the app in a headless
  Weston and saves PNGs (the app's `HFPREDICT_SCREENSHOT` hook) for phone-sized,
  wide, light, dark, low-light, each dialog and Spanish.
- Android: `android/build.sh emulator` starts a headless AOSP emulator (no
  Google APIs), `android/build.sh install`, then `android/build.sh shots`.
