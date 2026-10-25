#!/usr/bin/env bash
# build.sh - Builds dist/HFPredict-<version>-x86_64.AppImage.
#
# Runs in the GNOME build container (Debian 13: GTK 4.18, libadwaita 1.7),
# bundling GTK and libadwaita with linuxdeploy and its GTK plugin, so the
# AppImage runs on distributions from 2025 on (glibc 2.41) without either
# installed. The tools are downloaded into .cache/appimage/.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n1)
cd "$ROOT"

if [ "${IN_CONTAINER:-0}" != 1 ]; then
    # Through the GNOME container, with the in-repo toolchain.
    exec env EXTRA_ENV=1 "$ROOT/gnome/build.sh" shell-cmd "IN_CONTAINER=1 bash packaging/appimage/build.sh"
fi

TOOLS=.cache/appimage
mkdir -p "$TOOLS" dist
fetch() { [ -f "$TOOLS/$2" ] || curl -fsSL --retry 3 "$1" -o "$TOOLS/$2"; chmod +x "$TOOLS/$2"; }
fetch https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage linuxdeploy-x86_64.AppImage
fetch https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/master/linuxdeploy-plugin-gtk.sh linuxdeploy-plugin-gtk.sh

cargo build --release --locked -p hfpredict
BIN="${CARGO_TARGET_DIR:-target}/release/hfpredict"

APPDIR=$TOOLS/AppDir
rm -rf "$APPDIR"
install -Dm755 "$BIN" "$APPDIR/usr/bin/hfpredict"
install -Dm644 gnome/data/io.alvaroedo.HFPredict.metainfo.xml "$APPDIR/usr/share/metainfo/io.alvaroedo.HFPredict.appdata.xml"
install -Dm644 gnome/data/icons/hicolor/symbolic/apps/io.alvaroedo.HFPredict-symbolic.svg "$APPDIR/usr/share/icons/hicolor/symbolic/apps/io.alvaroedo.HFPredict-symbolic.svg"
# The symbolic icons the UI uses come from the Adwaita theme.
mkdir -p "$APPDIR/usr/share/icons"
mkdir -p "$APPDIR/usr/share/icons/Adwaita"
cp -r /usr/share/icons/Adwaita/symbolic /usr/share/icons/Adwaita/index.theme "$APPDIR/usr/share/icons/Adwaita/"

# Screenshot URLs only resolve once pushed; metadata is validated offline
# by gnome/build.sh test.
export NO_APPSTREAM=1 APPIMAGE_EXTRACT_AND_RUN=1 DEPLOY_GTK_VERSION=4 ARCH=x86_64 LINUXDEPLOY_OUTPUT_VERSION="$VERSION"
export PATH="$PWD/$TOOLS:$PATH"
(cd "$TOOLS" && ./linuxdeploy-x86_64.AppImage --appdir AppDir \
    --desktop-file "$ROOT/gnome/data/io.alvaroedo.HFPredict.desktop" \
    --icon-file "$ROOT/gnome/data/icons/hicolor/scalable/apps/io.alvaroedo.HFPredict.svg" \
    --plugin gtk)
# The GTK plugin forces X11; GTK picks Wayland or X11 well by itself.
# It also forces GTK_THEME, which overrides libadwaita's own stylesheet;
# libadwaita follows the dark/light preference through the portal itself.
sed -i -e '/GDK_BACKEND/d' -e '/GTK_THEME/d' "$APPDIR"/apprun-hooks/*gtk*.sh
(cd "$TOOLS" && ./linuxdeploy-x86_64.AppImage --appdir AppDir --output appimage)
mv "$TOOLS"/HFPredict-*.AppImage "dist/HFPredict-$VERSION-x86_64.AppImage"
echo "==> dist/HFPredict-$VERSION-x86_64.AppImage"
