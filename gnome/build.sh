#!/usr/bin/env bash
# build.sh - Build, run and test the GNOME app. Everything runs in a podman
# container (tools/containers/Containerfile); the host only needs podman.
#
# Usage: gnome/build.sh [command]   (default: build)
#
#   build         debug build              -> target/gnome/debug/hfpredict
#   release       release build            -> target/gnome/release/hfpredict
#   run [args]    build and run it on this desktop (Wayland)
#   test          core + GNOME unit tests, clippy and rustfmt check
#   screenshot    headless screenshots of the UI into target/screenshots/
#   flatpak       build the Flatpak bundle  -> dist/
#   appimage      build the AppImage        -> dist/
#   shell         a shell in the build container
#   image         (re)build the container image
#
# The Rust toolchain is the one in .tools/ (tools/rust.sh); cargo's
# registry and the build output stay inside the repository.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
IMAGE=localhost/hfpredict-gnome:latest
TARGET="$ROOT/target/gnome"

# VSCodium and other IDEs on Silverblue run as a Flatpak: reach the host's
# podman through flatpak-spawn then.
if [ -f /.flatpak-info ]; then
    PODMAN=(flatpak-spawn --host podman)
else
    PODMAN=(podman)
fi

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }

ensure_image() {
    if ! "${PODMAN[@]}" image exists "$IMAGE"; then
        info "Building container image $IMAGE"
        "${PODMAN[@]}" build --target gnome -t "$IMAGE" -f "$ROOT/tools/containers/Containerfile" "$ROOT/tools/containers"
    fi
}

# Runs a command in the container with the repository at the same path, so
# cargo's fingerprints and the toolchain's paths stay valid.
in_container() {
    ensure_image
    "$ROOT/tools/rust.sh" true
    local tty=()
    [ -t 0 ] && tty=(-t)
    "${PODMAN[@]}" run --rm -i "${tty[@]}" --userns=keep-id \
        -v "$ROOT:$ROOT:z" -w "$ROOT" \
        -e RUSTUP_HOME="$ROOT/.tools/rustup" -e CARGO_HOME="$ROOT/.tools/cargo" \
        -e CARGO_TARGET_DIR="$TARGET" -e PATH="$ROOT/.tools/cargo/bin:/usr/local/bin:/usr/bin:/bin" \
        -e HOME=/tmp -e CARGO_TERM_COLOR="${CARGO_TERM_COLOR:-auto}" \
        "${EXTRA[@]}" "$IMAGE" bash -c "$*"
}
EXTRA=()

case "${1:-build}" in
    build) in_container "cargo build -p hfpredict" ;;
    release) in_container "cargo build --release -p hfpredict" ;;
    run)
        shift
        EXTRA=(-e WAYLAND_DISPLAY="${WAYLAND_DISPLAY:-wayland-0}" -e XDG_RUNTIME_DIR=/run/user/host
               -v "${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/${WAYLAND_DISPLAY:-wayland-0}:/run/user/host/${WAYLAND_DISPLAY:-wayland-0}:z"
               -e HFPREDICT_DATA_DIR="$ROOT/target/gnome/home/data" -e HFPREDICT_CACHE_DIR="$ROOT/target/gnome/home/cache")
        in_container "cargo build -p hfpredict && $TARGET/debug/hfpredict $*"
        ;;
    test)
        in_container "cargo fmt --all --check && cargo clippy -p hfpredict-core -p hfpredict --all-targets -- -D warnings && cargo test -p hfpredict-core -p hfpredict"
        ;;
    screenshot)
        shift
        in_container "cargo build --release -p hfpredict && bash gnome/tools/screenshots.sh $*"
        ;;
    flatpak) exec "$ROOT/packaging/flatpak/build.sh" ;;
    appimage) exec "$ROOT/packaging/appimage/build.sh" ;;
    shell) in_container bash ;;
    image)
        "${PODMAN[@]}" build --target gnome -t "$IMAGE" -f "$ROOT/tools/containers/Containerfile" "$ROOT/tools/containers"
        ;;
    -h|--help|help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//' ;;
    *) echo "Unknown command '$1'. Run gnome/build.sh help." >&2; exit 2 ;;
esac
