#!/usr/bin/env bash
# build.sh - Builds dist/HFPredict-<version>-macos-<arch>.dmg: the GNOME app
# for macOS, as HFPredict.app with GTK 4, libadwaita and everything they need
# inside it. Runs on a Mac (Apple silicon or Intel) with Homebrew; the GitHub
# mirror runs it in .github/workflows/macos.yml.
#
# Linux comes first: this is a by-product of the same code. The app is not
# signed with an Apple Developer ID nor notarised (ad-hoc signature only), so
# macOS asks once: right-click the app, Open, then Open again.
#
# Homebrew's libraries are built for the macOS they were installed on, so the
# app needs that macOS version or newer.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"
VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' Cargo.toml | head -n1)
ARCH=$(uname -m)   # arm64 or x86_64
APP_ID=io.alvaroedo.HFPredict

if [ "$(uname -s)" != Darwin ]; then
    echo "packaging/macos/build.sh runs on macOS (see .github/workflows/macos.yml)" >&2
    exit 1
fi
if [ "${SKIP_BREW:-0}" != 1 ]; then
    brew install --quiet gtk4 libadwaita librsvg adwaita-icon-theme hicolor-icon-theme pkgconf dylibbundler
fi
BREW="$(brew --prefix)"

# Rust: the toolchain pinned in rust-toolchain.toml (rustup picks it up).
command -v cargo >/dev/null || { echo "needs rustup (https://rustup.rs)" >&2; exit 1; }
cargo build --release --locked -p hfpredict

OUT=target/macos
APP="$OUT/HFPredict.app"
C="$APP/Contents"
rm -rf "$OUT"
mkdir -p "$C/MacOS" "$C/Frameworks" "$C/Resources/share/icons" "$C/Resources/share/glib-2.0/schemas" "$C/Resources/lib"
# Not "hfpredict": macOS file systems are case-insensitive by default, and
# the launcher below is "HFPredict" (it would overwrite the program).
cp target/release/hfpredict "$C/MacOS/hfpredict-bin"

# The icon: the desktop SVG rendered at every size iconutil wants.
ICONSET="$OUT/HFPredict.iconset"
mkdir -p "$ICONSET"
SVG=desktop/data/icons/hicolor/scalable/apps/$APP_ID.svg
for s in 16 32 128 256 512; do
    rsvg-convert -w $s -h $s "$SVG" -o "$ICONSET/icon_${s}x${s}.png"
    rsvg-convert -w $((s * 2)) -h $((s * 2)) "$SVG" -o "$ICONSET/icon_${s}x${s}@2x.png"
done
iconutil -c icns "$ICONSET" -o "$C/Resources/HFPredict.icns"

# GTK's settings schemas, the icon themes (Adwaita for the UI's symbolic
# icons, hicolor for the app's own) and the SVG image loader.
cp "$BREW"/share/glib-2.0/schemas/*.xml "$C/Resources/share/glib-2.0/schemas/"
glib-compile-schemas "$C/Resources/share/glib-2.0/schemas"
cp -RL "$BREW/share/icons/Adwaita" "$C/Resources/share/icons/"
cp -RL "$BREW/share/icons/hicolor" "$C/Resources/share/icons/"
install -m644 "$SVG" "$C/Resources/share/icons/hicolor/scalable/apps/$APP_ID.svg"
mkdir -p "$C/Resources/share/icons/hicolor/symbolic/apps"
install -m644 desktop/data/icons/hicolor/symbolic/apps/$APP_ID-symbolic.svg "$C/Resources/share/icons/hicolor/symbolic/apps/"
gtk4-update-icon-cache -q -f -t "$C/Resources/share/icons/hicolor" || true
gtk4-update-icon-cache -q -f -t "$C/Resources/share/icons/Adwaita" || true

PIXBUF="$BREW/lib/gdk-pixbuf-2.0/2.10.0"
LOADERS=()
if [ -d "$PIXBUF/loaders" ]; then
    mkdir -p "$C/Resources/lib/gdk-pixbuf-2.0/2.10.0/loaders"
    for f in "$PIXBUF"/loaders/*.so "$PIXBUF"/loaders/*.dylib; do
        [ -e "$f" ] || continue
        cp -L "$f" "$C/Resources/lib/gdk-pixbuf-2.0/2.10.0/loaders/"
        LOADERS+=(-x "$C/Resources/lib/gdk-pixbuf-2.0/2.10.0/loaders/$(basename "$f")")
    done
    # The loader list with a placeholder for the app's path; the launcher
    # fills it in, since gdk-pixbuf wants absolute paths here.
    sed "s|$PIXBUF|@RESOURCES@/lib/gdk-pixbuf-2.0/2.10.0|g" "$PIXBUF/loaders.cache" \
        >"$C/Resources/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache.in"
fi

# Every Homebrew library the program and the loaders need, copied into
# Frameworks with their references rewritten.
dylibbundler -of -b -cd -x "$C/MacOS/hfpredict-bin" "${LOADERS[@]}" -s "$BREW/lib" \
    -d "$C/Frameworks" -p @executable_path/../Frameworks/ </dev/null >/dev/null

# The launcher: points GTK at the bundled data, follows the system language.
cat >"$C/MacOS/HFPredict" <<'EOF'
#!/bin/sh
HERE="$(cd "$(dirname "$0")" && pwd)"
RES="$(cd "$HERE/../Resources" && pwd)"
export XDG_DATA_DIRS="$RES/share"
export GSETTINGS_SCHEMA_DIR="$RES/share/glib-2.0/schemas"
if [ -f "$RES/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache.in" ]; then
    CACHE="$HOME/Library/Caches/io.alvaroedo.HFPredict"
    mkdir -p "$CACHE"
    sed "s|@RESOURCES@|$RES|g" "$RES/lib/gdk-pixbuf-2.0/2.10.0/loaders.cache.in" >"$CACHE/loaders.cache"
    export GDK_PIXBUF_MODULE_FILE="$CACHE/loaders.cache"
fi
# Apps opened from Finder get no LANG: take the language from the system.
if [ -z "${LANG:-}" ]; then
    LOCALE="$(defaults read -g AppleLocale 2>/dev/null | sed 's/@.*//')"
    [ -n "$LOCALE" ] && export LANG="$LOCALE.UTF-8"
fi
exec "$HERE/hfpredict-bin" "$@"
EOF
chmod +x "$C/MacOS/HFPredict"
file "$C/MacOS/hfpredict-bin" | grep -q Mach-O || { echo "the program is missing from the bundle" >&2; exit 1; }

cat >"$C/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>HFPredict</string>
    <key>CFBundleDisplayName</key><string>HFPredict</string>
    <key>CFBundleIdentifier</key><string>$APP_ID</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleExecutable</key><string>HFPredict</string>
    <key>CFBundleIconFile</key><string>HFPredict</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
    <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSLocationUsageDescription</key><string>HFPredict sets where you transmit from to your location. It stays on this Mac.</string>
    <key>NSLocationWhenInUseUsageDescription</key><string>HFPredict sets where you transmit from to your location. It stays on this Mac.</string>
    <key>NSHumanReadableCopyright</key><string>© 2026 Álvaro Edo Martínez · GPL-3.0-or-later</string>
</dict>
</plist>
EOF
cp LICENSE NOTICE "$C/Resources/"

# Ad-hoc signature: Apple silicon runs nothing unsigned, and rewriting the
# libraries' references above voided Homebrew's signatures.
codesign --force --deep --sign - "$APP"

mkdir -p dist
DMG="dist/HFPredict-$VERSION-macos-$ARCH.dmg"
rm -f "$DMG"
STAGE="$OUT/dmg"
mkdir -p "$STAGE"
cp -R "$APP" "$STAGE/"
ln -s /Applications "$STAGE/Applications"
hdiutil create -quiet -volname "HFPredict $VERSION" -srcfolder "$STAGE" -ov -format UDZO "$DMG"
ls -la "$DMG"
