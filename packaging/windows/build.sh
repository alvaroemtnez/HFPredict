#!/usr/bin/env bash
# build.sh - Builds dist/HFPredict-<version>-windows-<arch>.zip: the GNOME app
# for Windows, with GTK 4, libadwaita and everything they need beside it.
# Unzip and run HFPredict\bin\hfpredict.exe.
#
# Linux comes first: this is a by-product of the same code. Two ways:
#
# - x86_64, on Linux: cross-compiled in its own container
#   (tools/containers/Containerfile, target "windows": Fedora's MinGW GTK 4
#   and AppStream, libadwaita 1.7 built there, by windows-deps.sh) with the
#   in-repo Rust toolchain plus its Windows target. The release workflow's
#   Fedora job runs it with IN_CONTAINER=1. Nothing here changes a Linux build.
# - arm64, on Windows on ARM, in an MSYS2 CLANGARM64 shell, with MSYS2's
#   GTK 4 and libadwaita (no Linux distribution packages them for Windows on
#   ARM); the pinned Rust toolchain goes into .tools/ as on Linux.
#   .github/workflows/windows.yml runs it on the GitHub mirror. A CLANG64
#   shell (x86_64) works the same way, though releases use the cross-build.
#
# The engine links files with Unix symlinks, which Windows lacks: this build
# alone uses a private copy patched with hfcast-engine-windows.patch (a
# copy instead of a link where there are no Unix symlinks). The engine the
# Linux and Android builds use is never touched, and Cargo.lock is restored.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# At most four cores (HFP_CPUS to change): a laptop running a long build
# on every core can overheat.
CPUS="${HFP_CPUS:-$(( $(nproc) < 4 ? $(nproc) : 4 ))}"
VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n1)
IMAGE=localhost/hfpredict-windows:latest
cd "$ROOT"

if [ -n "${MSYSTEM:-}" ]; then
    # Native, in MSYS2. The toolchains are LLVM's: rustc's *-windows-gnullvm
    # targets and MSYS2's clang environments agree on the C runtime (UCRT).
    case "$MSYSTEM" in
        CLANG64) TARGET=x86_64-pc-windows-gnullvm ARCH=x86_64 ;;
        CLANGARM64) TARGET=aarch64-pc-windows-gnullvm ARCH=arm64 ;;
        *) echo "packaging/windows/build.sh: run it from an MSYS2 CLANG64 or CLANGARM64 shell" >&2; exit 1 ;;
    esac
    SYSROOT="$MINGW_PREFIX"
    OBJDUMP=llvm-objdump
    # Windows programs read these, so in C:/... form.
    RUSTUP_HOME="$(cygpath -m "$ROOT/.tools/rustup")"
    CARGO_HOME="$(cygpath -m "$ROOT/.tools/cargo")"
    export RUSTUP_HOME CARGO_HOME CARGO_TARGET_DIR="$(cygpath -m "$ROOT/target/windows")"
    export PATH="$ROOT/.tools/cargo/bin:$PATH"
    if [ ! -x "$ROOT/.tools/cargo/bin/rustup.exe" ]; then
        echo "==> Installing rustup into .tools/ (no system changes)" >&2
        mkdir -p "$ROOT/.tools"
        # The installer for this machine's processor (it needs no MSVC to run);
        # the toolchain it installs is the gnullvm one, set as the host.
        curl --proto '=https' --tlsv1.2 -fsSL --retry 3 \
            "https://static.rust-lang.org/rustup/dist/$MSYSTEM_CARCH-pc-windows-msvc/rustup-init.exe" \
            -o "$ROOT/.tools/rustup-init.exe"
        "$ROOT/.tools/rustup-init.exe" -y --no-modify-path --profile minimal \
            --default-toolchain none --default-host "$TARGET"
        rm -f "$ROOT/.tools/rustup-init.exe"
    fi
    T="${TARGET^^}"; T="${T//-/_}"
    export "CARGO_TARGET_${T}_LINKER=clang" "CC_${TARGET//-/_}=clang" "AR_${TARGET//-/_}=llvm-ar"
elif [ "${IN_CONTAINER:-0}" != 1 ]; then
    if [ -f /.flatpak-info ]; then PODMAN=(flatpak-spawn --host podman); else PODMAN=(podman); fi
    if ! "${PODMAN[@]}" image exists "$IMAGE"; then
        "${PODMAN[@]}" build --cpu-period 100000 --cpu-quota "$((CPUS * 100000))" --target windows -t "$IMAGE" -f "$ROOT/tools/containers/Containerfile" "$ROOT/tools/containers"
    fi
    "$ROOT/tools/rust.sh" true
    exec "${PODMAN[@]}" run --rm -i --userns=keep-id --cpus="$CPUS" -v "$ROOT:$ROOT:z" -w "$ROOT" \
        -e RUSTUP_HOME="$ROOT/.tools/rustup" -e CARGO_HOME="$ROOT/.tools/cargo" \
        -e CARGO_TARGET_DIR="$ROOT/target/windows" -e HOME=/tmp -e IN_CONTAINER=1 \
        -e PATH="$ROOT/.tools/cargo/bin:/usr/local/bin:/usr/bin:/bin" \
        "$IMAGE" bash packaging/windows/build.sh
else
    # In the container (or the release workflow's Fedora job): cross-compiled
    # with Fedora's MinGW, using the toolchain in .tools/.
    eval "$("$ROOT/tools/rust.sh" env)"
    export CARGO_TARGET_DIR="$ROOT/target/windows"
    TARGET=x86_64-pc-windows-gnu ARCH=x86_64
    SYSROOT=/usr/x86_64-w64-mingw32/sys-root/mingw
    OBJDUMP=x86_64-w64-mingw32-objdump
    rustup target add "$TARGET" >/dev/null
    export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
    export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
    export PKG_CONFIG_ALLOW_CROSS=1
    export PKG_CONFIG_x86_64_pc_windows_gnu=x86_64-w64-mingw32-pkg-config
fi

# The pinned engine, copied out of cargo's checkout and patched.
mkdir -p target/windows
cargo fetch --locked >/dev/null
ENGINE_REV=$(sed -n 's/.*hfcast-engine.git", rev = "\([0-9a-f]*\)".*/\1/p' Cargo.toml)
ENGINE_URL=$(sed -n 's/^hfcast = { git = "\([^"]*\)".*/\1/p' Cargo.toml)
CHECKOUT=$(ls -d "$CARGO_HOME"/git/checkouts/hfcast-engine-*/"${ENGINE_REV:0:7}" | head -n1)
ENGINE=target/windows/hfcast-engine
rm -rf "$ENGINE"
cp -r "$CHECKOUT" "$ENGINE"
patch -d "$ENGINE" -p1 --quiet <packaging/windows/hfcast-engine-windows.patch
cp Cargo.lock target/windows/Cargo.lock.saved
trap 'cp target/windows/Cargo.lock.saved Cargo.lock' EXIT
cargo build --release --target "$TARGET" -p hfpredict \
    --config "patch.\"$ENGINE_URL\".hfcast.path = \"$ENGINE\""

OUT="target/windows/HFPredict"
rm -rf "$OUT"
mkdir -p "$OUT/bin" "$OUT/share/icons" "$OUT/share/glib-2.0/schemas" "$OUT/lib"
cp "target/windows/$TARGET/release/hfpredict.exe" "$OUT/bin/"

# Every DLL the program needs, followed from its imports, from the sysroot.
declare -A seen=()
queue=("$OUT/bin/hfpredict.exe")
while [ ${#queue[@]} -gt 0 ]; do
    file="${queue[0]}"; queue=("${queue[@]:1}")
    for dll in $("$OBJDUMP" -p "$file" | sed -n 's/.*DLL Name: //p'); do
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
        for dll in $("$OBJDUMP" -p "$loader" | sed -n 's/.*DLL Name: //p'); do
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
rm -f "dist/HFPredict-$VERSION-windows-$ARCH.zip"
(cd target/windows && zip -qr "$ROOT/dist/HFPredict-$VERSION-windows-$ARCH.zip" HFPredict)
ls -la "dist/HFPredict-$VERSION-windows-$ARCH.zip"
