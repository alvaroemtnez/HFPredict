# 0004. Script builds, toolchains inside the repository

Status: accepted (2026-10-25)

As in Skypost and upe-tools: `tools/rust.sh` installs the pinned Rust toolchain
into `.tools/`; `android/build.sh` downloads JDK, SDK, NDK and cargo-ndk into
`android/.tools/`; the GNOME app, Flatpak and AppImage build in podman
containers that mount the repository at its own path and reuse `.tools/`.
Nothing is installed system-wide. CI runs the same scripts.

GTK 4.18 and libadwaita 1.7 (Debian 13) are the minimum, so the AppImage can
bundle them; the Flatpak uses the GNOME 51 runtime.
