#!/usr/bin/env bash
# build.sh - Builds dist/HFPredict-<version>-windows-x86_64.zip: the GNOME app
# cross-compiled for 64-bit Windows, with GTK 4, libadwaita and everything
# they need beside it. Unzip and run HFPredict\bin\hfpredict.exe.
#
# Linux comes first: this is a by-product of the same code, built in its own
# container (tools/containers/Containerfile, target "windows": Fedora's MinGW
# GTK 4 and AppStream, libadwaita 1.7 built there) with the in-repo Rust
# toolchain plus its Windows target. Nothing here changes a Linux build.
#
# Waiting on the engine: hfcast-engine v2.14.0 links its antenna files with
# std::os::unix::fs::symlink (src/runner.rs), which Windows lacks, so the
# build stops there. Everything before it (GTK 4, libadwaita, AppStream)
# cross-builds; see docs/roadmap.md, "Windows and macOS".
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# At most four cores (HFP_CPUS to change): a laptop running a long build
# on every core can overheat.
CPUS="${HFP_CPUS:-$(( $(nproc) < 4 ? $(nproc) : 4 ))}"
VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n1)
IMAGE=localhost/hfpredict-windows:latest
cd "$ROOT"

if [ "${IN_CONTAINER:-0}" != 1 ]; then
    if [ -f /.flatpak-info ]; then PODMAN=(flatpak-spawn --host podman); else PODMAN=(podman); fi
    if ! "${PODMAN[@]}" image exists "$IMAGE"; then
        "${PODMAN[@]}" build --target windows -t "$IMAGE" -f "$ROOT/tools/containers/Containerfile" "$ROOT/tools/containers"
    fi
    "$ROOT/tools/rust.sh" true
    exec "${PODMAN[@]}" run --rm -i --userns=keep-id --cpus="$CPUS" -v "$ROOT:$ROOT:z" -w "$ROOT" \
        -e RUSTUP_HOME="$ROOT/.tools/rustup" -e CARGO_HOME="$ROOT/.tools/cargo" \
        -e CARGO_TARGET_DIR="$ROOT/target/windows" -e HOME=/tmp -e IN_CONTAINER=1 \
        -e PATH="$ROOT/.tools/cargo/bin:/usr/local/bin:/usr/bin:/bin" \
        "$IMAGE" bash packaging/windows/build.sh
fi

TARGET=x86_64-pc-windows-gnu
SYSROOT=/usr/x86_64-w64-mingw32/sys-root/mingw
rustup target add "$TARGET" >/dev/null

export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_x86_64_pc_windows_gnu=x86_64-w64-mingw32-pkg-config
cargo build --release --locked --target "$TARGET" -p hfpredict

OUT="target/windows/HFPredict"
rm -rf "$OUT"
mkdir -p "$OUT/bin" "$OUT/share/icons" "$OUT/share/glib-2.0/schemas" "$OUT/lib"
cp "target/windows/$TARGET/release/hfpredict.exe" "$OUT/bin/"

# Every DLL the program needs, followed from its imports, from the sysroot.
declare -A seen=()
queue=("$OUT/bin/hfpredict.exe")
while [ ${#queue[@]} -gt 0 ]; do
    file="${queue[0]}"; queue=("${queue[@]:1}")
    for dll in $(x86_64-w64-mingw32-objdump -p "$file" | sed -n 's/.*DLL Name: //p'); do
        key="${dll,,}"
        [ -n "${seen[$key]:-}" ] && continue
        seen[$key]=1
        src="$(find "$SYSROOT/bin" -maxdepth 1 -iname "$dll" | head -n1)"
        # Not in the sysroot: a Windows system DLL.
        [ -z "$src" ] && continue
        cp "$src" "$OUT/bin/"
        queue+=("$src")
    done
done

# GTK's settings schemas, the icon themes and the SVG image loader.
cp "$SYSROOT"/share/glib-2.0/schemas/*.xml "$OUT/share/glib-2.0/schemas/"
glib-compile-schemas "$OUT/share/glib-2.0/schemas"
cp -r "$SYSROOT/share/icons/Adwaita" "$OUT/share/icons/"
cp -r "$SYSROOT/share/icons/hicolor" "$OUT/share/icons/"
install -Dm644 gnome/data/icons/hicolor/scalable/apps/io.alvaroedo.HFPredict.svg \
    "$OUT/share/icons/hicolor/scalable/apps/io.alvaroedo.HFPredict.svg"
install -Dm644 gnome/data/icons/hicolor/symbolic/apps/io.alvaroedo.HFPredict-symbolic.svg \
    "$OUT/share/icons/hicolor/symbolic/apps/io.alvaroedo.HFPredict-symbolic.svg"
if [ -d "$SYSROOT/lib/gdk-pixbuf-2.0" ]; then
    cp -r "$SYSROOT/lib/gdk-pixbuf-2.0" "$OUT/lib/"
    for loader in "$OUT"/lib/gdk-pixbuf-2.0/2.10.0/loaders/*.dll; do
        for dll in $(x86_64-w64-mingw32-objdump -p "$loader" | sed -n 's/.*DLL Name: //p'); do
            src="$(find "$SYSROOT/bin" -maxdepth 1 -iname "$dll" | head -n1)"
            [ -n "$src" ] && [ ! -f "$OUT/bin/$(basename "$src")" ] && cp "$src" "$OUT/bin/"
        done
    done
    # The loader list, with paths relative to the program as gdk-pixbuf
    # reads them on Windows.
    {
        for loader in "$OUT"/lib/gdk-pixbuf-2.0/2.10.0/loaders/*.dll; do
            name="$(basename "$loader")"
            grep -A20 "\"[^\"]*$name\"" "$SYSROOT/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache" 2>/dev/null \
                | sed -e "1s|^\"[^\"]*\"|\"lib\\\\\\\\gdk-pixbuf-2.0\\\\\\\\2.10.0\\\\\\\\loaders\\\\\\\\$name\"|" -e '/^$/q'
        done
    } >"$OUT/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache" || true
fi
cp LICENSE NOTICE "$OUT/"

mkdir -p dist
rm -f "dist/HFPredict-$VERSION-windows-x86_64.zip"
(cd target/windows && zip -qr "$ROOT/dist/HFPredict-$VERSION-windows-x86_64.zip" HFPredict)
ls -la "dist/HFPredict-$VERSION-windows-x86_64.zip"
