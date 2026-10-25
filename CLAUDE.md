# HFPredict

HF propagation forecasts for amateur radio, for Android and GNOME. A rewrite of
HFcast (Jonathan McSweet) on his HFcast engine (Rust VOACAP + Truecast).
App id `io.alvaroedo.HFPredict` everywhere. Owner: Álvaro Edo Martínez (licensed
ham, Spain). GPL-3.0-or-later. Read `docs/architecture.md` first, then
`docs/roadmap.md` for what is open.

## Layout

- `core/` — Rust: everything that decides a number (engine requests, the
  WSPR-fitted correction, survey, coverage lattices, places, online readings,
  saved state, map cache, the map renderer). `core/src/app.rs` `Core` is the API.
- `ffi/` — the core as `libhfpredict_ffi.so` + `uniffi-bindgen` (Kotlin bindings).
- `gnome/` — GTK 4.18 / libadwaita 1.7 app in Rust; `gnome/build.sh` (podman).
- `android/` — Kotlin + Compose Material 3; `android/build.sh` (no Android Studio).
- `i18n/<lang>.json` — every UI string, one table per language (en, es, ca, de, ja, ar).
- `packaging/` — Flatpak (GNOME 51) and AppImage scripts. `.forgejo/` — CI and release.
- `tools/` — `rust.sh` (pinned toolchain in `.tools/`), `gen-strings.py`,
  `gen-icons.py`, `update-ssn.py`, release helpers.

## Commands

```sh
tools/rust.sh cargo test --workspace     # core tests (engine runs included)
gnome/build.sh test                      # fmt + clippy -D warnings + tests (container)
gnome/build.sh run | screenshot [names]  # run on the desktop / headless PNGs in target/screenshots
android/build.sh debug | test | release  # APK / unit tests + lint (zero warnings) / signed APKs
android/build.sh emulator; android/build.sh install; android/build.sh shots
packaging/flatpak/build.sh; packaging/appimage/build.sh   # -> dist/
python3 tools/gen-strings.py             # after editing i18n/*.json (Android XML)
python3 tools/gen-icons.py               # after editing the icon design
```

## Rules

- **Commit freely, never push.** Small coherent commits; the owner pushes.
- **Nothing outside this folder.** Toolchains download into `.tools/`,
  `android/.tools/`, `.cache/`; containers via podman (`flatpak-spawn --host`
  from the VSCodium sandbox). Never install system packages.
- **100% FOSS, no Google.** No Play Services, no Firebase, no closed SDKs, no
  telemetry. Location through the platform `LocationManager`. Only the
  `INTERNET` and `ACCESS_COARSE_LOCATION` permissions; ask before adding one.
- **One implementation.** Logic lives in `core/`; the apps lay out and draw.
  Do not reimplement core logic in Kotlin or in the GNOME code; export a helper
  from `core/src/exports.rs` instead.
- **The engine is not ours.** Pinned by commit in the root `Cargo.toml`
  (mirror of hfcast-engine, tag v2.14.0). Don't patch it; see
  `docs/decisions/0002-engine.md` before moving the pin.
- **Strings**: edit `i18n/*.json` (all six languages; English is the
  fallback), placeholders `{name}`, then `tools/gen-strings.py`. Code, comments
  and docs in English.
- **Colours** come from the platform theme (Material You / GNOME accent);
  map colours are derived in `ui/Theme.kt` and `gnome/src/palette.rs`.
- **Phones stay portrait**; tablets (sw ≥ 600 dp) and desktop windows get two
  columns when wide (≥ 840 dp Android, ≥ 960 sp GNOME).
- **Version** lives only in the root `Cargo.toml`; releases follow
  `docs/releasing.md` (CHANGELOG entry required).
- Credit upstream (NOTICE, About screens) whenever something new is ported.
