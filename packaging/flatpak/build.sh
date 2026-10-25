#!/usr/bin/env bash
# build.sh - Builds the Flatpak bundle: dist/HFPredict-<version>-x86_64.flatpak
#
# 1. vendors every crate into .cache/flatpak-vendor (with the toolchain in
#    .tools/), so the Flatpak build itself is offline;
# 2. runs flatpak-builder in Flathub's own CI image (GNOME 51 SDK inside),
#    privileged (rootless) because flatpak-builder sandboxes with bubblewrap.
#
# In CI (FLATPAK_IN_CONTAINER=1) step 2 runs directly in the job's container.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# At most six cores (HFP_CPUS to change): a laptop running a long build
# on every core can overheat.
CPUS="${HFP_CPUS:-$(( $(nproc) < 6 ? $(nproc) : 6 ))}"
IMAGE=ghcr.io/flathub-infra/flatpak-github-actions:gnome-51
APP=io.alvaroedo.HFPredict
VERSION=$(sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -n1)
OUT="dist/HFPredict-$VERSION-x86_64.flatpak"
cd "$ROOT"

vendor() {
    echo "==> Vendoring crates"
    rm -rf .cache/flatpak-vendor
    mkdir -p .cache
    tools/rust.sh cargo vendor --locked --versioned-dirs .cache/flatpak-vendor > .cache/flatpak-vendor-config.toml
    # Vendoring packages the engine as crates.io would, which leaves out its
    # coefficient maps (see the engine's NOTICE). Put them back from the
    # pinned git checkout; cargo only verifies the files it listed.
    local rev checkout crate
    rev=$(sed -n 's/.*hfcast-engine.git?rev=\([0-9a-f]*\).*/\1/p' Cargo.lock | head -n1)
    checkout=$(ls -d "$ROOT/.tools/cargo"/git/checkouts/hfcast-engine-*/"${rev:0:7}" | head -n1)
    crate=$(ls -d .cache/flatpak-vendor/hfcast-* | head -n1)
    cp -r "$checkout/embedded/coeffs" "$crate/embedded/"
    # Inside the build the vendor directory sits next to the sources.
    sed -i "s|directory = \".*\"|directory = \"/run/build/hfpredict/vendor\"|" .cache/flatpak-vendor-config.toml
}

build() {
    flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
    flatpak-builder --user --install-deps-from=flathub --force-clean --disable-rofiles-fuse \
        --repo=packaging/flatpak/repo --state-dir=packaging/flatpak/.flatpak-builder \
        packaging/flatpak/build-dir packaging/flatpak/$APP.yml
    mkdir -p dist
    flatpak build-bundle --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
        packaging/flatpak/repo "$OUT" "$APP"
    echo "==> $OUT"
}

case "${1:-all}" in
    vendor) vendor ;;
    inner) build ;;
    all)
        vendor
        if [ "${FLATPAK_IN_CONTAINER:-0}" = 1 ]; then
            build
        else
            PODMAN=(podman)
            [ -f /.flatpak-info ] && PODMAN=(flatpak-spawn --host podman)
            mkdir -p .cache/flatpak-user
            "${PODMAN[@]}" run --rm --privileged --cpus="$CPUS" -v "$ROOT:$ROOT:z" -w "$ROOT" \
                -v "$ROOT/.cache/flatpak-user:/root/.local/share/flatpak:z" \
                "$IMAGE" bash packaging/flatpak/build.sh inner
        fi
        ;;
    *) echo "usage: $0 [vendor|all]" >&2; exit 2 ;;
esac
