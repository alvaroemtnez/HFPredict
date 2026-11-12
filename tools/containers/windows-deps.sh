#!/bin/sh
# windows-deps.sh - Sets up a Fedora system for packaging/windows/build.sh:
# Fedora's MinGW GTK 4 and AppStream, libadwaita 1.7 built here (Fedora
# packages no MinGW libadwaita), and NSIS for the installer. Run as root by
# the Containerfile's "windows" stage (local builds only; releases use MSYS2).
set -eu

dnf -y install --setopt=install_weak_deps=False \
    mingw64-gcc mingw64-gtk4 mingw64-appstream mingw64-adwaita-icon-theme \
    mingw64-librsvg2 mingw64-pkg-config mingw64-hicolor-icon-theme \
    mingw32-nsis gcc meson ninja-build sassc gettext glib2-devel python3 curl xz findutils patch
dnf clean all

ADW_VERSION=1.7.12
ADW_SHA256=94c41ab41c2d2188af8290ac0dbb906896cf75e76e69722c2ea2572531de3308
curl -fsSL "https://download.gnome.org/sources/libadwaita/1.7/libadwaita-$ADW_VERSION.tar.xz" -o /tmp/adw.tar.xz
echo "$ADW_SHA256  /tmp/adw.tar.xz" | sha256sum -c -
tar -C /tmp -xf /tmp/adw.tar.xz
cd "/tmp/libadwaita-$ADW_VERSION"
meson setup build --cross-file /usr/share/mingw/toolchain-mingw64.meson \
    --prefix /usr/x86_64-w64-mingw32/sys-root/mingw --buildtype release \
    -Dintrospection=disabled -Dvapi=false -Dgtk_doc=false -Dtests=false -Dexamples=false
ninja -C build install
cd /
rm -rf /tmp/adw.tar.xz "/tmp/libadwaita-$ADW_VERSION"
