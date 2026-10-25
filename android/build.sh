#!/usr/bin/env bash
# build.sh - Build the HFPredict Android app from the command line, no Android Studio needed.
#
# Usage: ./build.sh [command]   (default: debug)
#
#   debug       build the debug APK -> dist/hfpredict-debug.apk
#   release     build the release APKs, signed if HFPREDICT_KEYSTORE is set
#   install     build the debug APK and install it on the connected phone
#   run         install, then launch the app and follow its log
#   logcat      follow the log of the running app
#   test        run the JVM unit tests and Android lint (debug + release checks)
#   gradle ...  run any Gradle task with the bundled toolchain
#   env         print exports for using gradlew/adb directly: eval "$(./build.sh env)"
#   rust        only build the Rust core and its Kotlin bindings
#   emulator    start a headless AOSP emulator (no Google services) for testing
#   shots       screenshots of the installed debug app into ../target/screenshots/android/
#   setup       only download the toolchain into .tools/
#   clean       remove build outputs
#   distclean   also remove .tools/ (the downloaded toolchain and Gradle caches)
#
# Everything (JDK, Android SDK and NDK, Gradle and its caches, cargo-ndk) is
# downloaded into android/.tools/ and the Rust toolchain into ../.tools/;
# nothing is installed system-wide. See android/README.md.
#
# Rust ABIs: HFP_ABIS="arm64-v8a x86_64" limits a debug build to what the
# device (or emulator) needs; release builds always build all three.

set -euo pipefail

ANDROID_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT_DIR="$(cd "$ANDROID_DIR/.." && pwd)"
GENERATED="$ANDROID_DIR/app/build/generated/uniffi"
ALL_ABIS="arm64-v8a armeabi-v7a x86_64"
CARGO_NDK_VERSION=4.1.2
# The emulator image: AOSP, no Google APIs, so the app is tested as it runs
# on a de-Googled phone.
EMULATOR_API=36
EMULATOR_IMAGE="system-images;android-$EMULATOR_API;default;x86_64"
DIST_DIR="$ANDROID_DIR/dist"
TOOLS_DIR="$ANDROID_DIR/.tools"
APP_GRADLE="$ANDROID_DIR/app/build.gradle.kts"

# AGP 9 and Gradle 9 need JDK 17+; 21 is the LTS they are tested with.
JDK_VERSION=21
# https://developer.android.com/studio#command-line-tools-only
CMDLINE_TOOLS_VERSION=16111833
CMDLINE_TOOLS_SHA256=0877a1d048fe4a24efe2eff536ca4223f7adeb58648bb81909d33c446918cfa8

JDK_DIR="$TOOLS_DIR/jdk-$JDK_VERSION"
SDK_DIR="$TOOLS_DIR/android-sdk"
ADB="$SDK_DIR/platform-tools/adb"
ANDROID_CLI="$SDK_DIR/cmdline-tools/latest/bin/android"

# Make every tool use the copies in .tools/, regardless of what the system has.
export JAVA_HOME="$JDK_DIR"
export ANDROID_HOME="$SDK_DIR"
unset ANDROID_SDK_ROOT
export GRADLE_USER_HOME="$TOOLS_DIR/gradle"
# Instead of ~/.android: AGP's debug keystore, the Android CLI and its caches.
# (adb ignores it and keeps its key in ~/.android/adbkey.)
export ANDROID_USER_HOME="$TOOLS_DIR/android-user"
export PATH="$JDK_DIR/bin:$SDK_DIR/platform-tools:$PATH"

info() { printf '\033[1;34m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1;31mError:\033[0m %s\n' "$*" >&2; exit 1; }

require() {
    local cmd
    for cmd in "$@"; do
        command -v "$cmd" >/dev/null || die "'$cmd' not found, please install it."
    done
}

# Read a value from app/build.gradle.kts, e.g. gradle_value compileSdk -> 35.
gradle_value() {
    local value
    value="$(sed -n "s/^ *$1 *= *\"\{0,1\}\([0-9.]*\)\"\{0,1\}.*/\1/p" "$APP_GRADLE" | head -n 1)"
    [ -n "$value" ] || die "Could not read $1 from $APP_GRADLE."
    echo "$value"
}

# Download $1 to $2 and check it against the SHA-256 $3.
fetch_verified() {
    local url="$1" out="$2" sha="$3"
    curl -fL --retry 3 --progress-bar "$url" -o "$out"
    echo "$sha  $out" | sha256sum -c --quiet - || die "Checksum mismatch for $url"
}

setup_jdk() {
    [ -x "$JDK_DIR/bin/java" ] && return
    local tmp meta url sha
    tmp="$(mktemp -d)"
    # The assets API gives the download link together with its SHA-256.
    meta="$(curl -fsSL "https://api.adoptium.net/v3/assets/latest/$JDK_VERSION/hotspot?os=linux&architecture=x64&image_type=jdk&vendor=eclipse")"
    url="$(echo "$meta" | sed -n 's/.*"link": *"\([^"]*\.tar\.gz\)".*/\1/p' | head -n 1)"
    sha="$(echo "$meta" | sed -n 's/.*"checksum": *"\([0-9a-f]\{64\}\)".*/\1/p' | head -n 1)"
    [ -n "$url" ] && [ -n "$sha" ] || die "Could not find a Temurin JDK $JDK_VERSION download."
    info "Downloading Temurin JDK $JDK_VERSION ($(basename "$url"))"
    fetch_verified "$url" "$tmp/jdk.tar.gz" "$sha"
    mkdir -p "$tmp/out" "$TOOLS_DIR"
    tar -xf "$tmp/jdk.tar.gz" -C "$tmp/out" --strip-components=1
    mv "$tmp/out" "$JDK_DIR"
    rm -rf "$tmp"
}

# The Android CLI (`android sdk`), which replaces sdkmanager. Usage metrics are off.
android_cli() {
    "$ANDROID_CLI" --no-metrics --sdk="$SDK_DIR" "$@"
}

# Install SDK package $1 (e.g. "platform-tools") unless $SDK_DIR/$1 exists.
# Installing accepts its licence (the Android SDK terms, developer.android.com/studio/terms).
sdk_package() {
    [ -d "$SDK_DIR/$1" ] && return
    info "Installing SDK package $1"
    android_cli sdk install "$1"
}

setup_sdk() {
    if [ ! -x "$ANDROID_CLI" ]; then
        info "Downloading Android command line tools"
        local tmp
        tmp="$(mktemp -d)"
        fetch_verified "https://dl.google.com/android/repository/commandlinetools-linux-${CMDLINE_TOOLS_VERSION}_latest.zip" \
            "$tmp/cmdline-tools.zip" "$CMDLINE_TOOLS_SHA256"
        unzip -q "$tmp/cmdline-tools.zip" -d "$tmp"
        mkdir -p "$SDK_DIR/cmdline-tools"
        mv "$tmp/cmdline-tools" "$SDK_DIR/cmdline-tools/latest"
        rm -rf "$tmp"
    fi

    # The SDK packages follow app/build.gradle.kts, so bumping compileSdk or
    # buildToolsVersion there is enough; the next build installs them.
    # Since API 37 the platform packages carry a minor version (android-37.0).
    local api platform
    api="$(gradle_value compileSdk)"
    platform="platforms/android-$api"
    [ "$api" -ge 37 ] && platform="$platform.0"
    sdk_package platform-tools
    sdk_package "$platform"
    sdk_package "build-tools/$(gradle_value buildToolsVersion)"
    sdk_package "ndk/$(gradle_value ndkVersion)"

    # For running ./gradlew by hand and for editor tooling; ignored by git.
    echo "sdk.dir=$SDK_DIR" > "$ANDROID_DIR/local.properties"
}

# The Rust toolchain (../.tools via tools/rust.sh) plus cargo-ndk.
setup_rust() {
    eval "$("$ROOT_DIR/tools/rust.sh" env)"
    export CARGO_TARGET_DIR="$ROOT_DIR/target/android"
    export ANDROID_NDK_HOME="$SDK_DIR/ndk/$(gradle_value ndkVersion)"
    if ! command -v cargo-ndk >/dev/null || [ "$(cargo ndk --version 2>/dev/null | awk '{print $2}')" != "$CARGO_NDK_VERSION" ]; then
        info "Installing cargo-ndk $CARGO_NDK_VERSION into ../.tools/cargo"
        cargo install --locked --quiet cargo-ndk --version "$CARGO_NDK_VERSION"
    fi
}

# Builds the core for each ABI into the generated jniLibs, and the Kotlin
# bindings from a host build of the same library.
build_rust() {
    local abis="${1:-$ALL_ABIS}" args=()
    setup_rust
    for abi in $abis; do args+=(-t "$abi"); done
    info "Building the Rust core for: $abis"
    rm -rf "$GENERATED/jniLibs"
    (cd "$ROOT_DIR" && cargo ndk -P "$(gradle_value minSdk)" "${args[@]}" -o "$GENERATED/jniLibs" \
        build --release --locked -p hfpredict-ffi)
    info "Generating the Kotlin bindings"
    # From an unstripped host build: release builds strip the metadata the
    # generator reads.
    (cd "$ROOT_DIR" && cargo build --locked -q -p hfpredict-ffi --features bindgen)
    rm -rf "$GENERATED/kotlin"
    (cd "$ROOT_DIR" && "$CARGO_TARGET_DIR/debug/uniffi-bindgen" generate --no-format \
        --library "$CARGO_TARGET_DIR/debug/libhfpredict_ffi.so" --language kotlin --out-dir "$GENERATED/kotlin")
}

setup() {
    [ "$(uname -s)" = Linux ] || die "Only Linux is supported."
    # Google ships the SDK build tools (aapt2, d8...) for x86_64 Linux only.
    [ "$(uname -m)" = x86_64 ] || die "Unsupported architecture $(uname -m), the Android SDK needs x86_64."
    require curl tar unzip sha256sum
    setup_jdk
    setup_sdk
}

gradle() {
    (cd "$ANDROID_DIR" && ./gradlew --no-daemon "$@")
}

# The debug build installs next to the release one, as <applicationId>.debug.
debug_app_id() {
    echo "$(sed -n 's/^ *applicationId *= *"\(.*\)".*/\1/p' "$APP_GRADLE").debug"
}

build_debug() {
    setup
    build_rust "${HFP_ABIS:-$ALL_ABIS}"
    info "Building debug APK"
    gradle assembleDebug
    mkdir -p "$DIST_DIR"
    cp "$ANDROID_DIR/app/build/outputs/apk/debug/app-universal-debug.apk" "$DIST_DIR/hfpredict-debug.apk"
    info "Done: $DIST_DIR/hfpredict-debug.apk"
}

version() {
    sed -n 's/^version *= *"\(.*\)"/\1/p' "$ROOT_DIR/Cargo.toml" | head -n 1
}

# Release APKs: one per ABI and a universal one, named for the release page
# (HFPredict-<version>-<abi>.apk). Signed when HFPREDICT_KEYSTORE is set;
# see docs/signing.md.
build_release() {
    setup
    build_rust "$ALL_ABIS"
    info "Building release APKs"
    gradle assembleRelease
    local v out tools
    v="$(version)"
    tools="$SDK_DIR/build-tools/$(gradle_value buildToolsVersion)"
    mkdir -p "$DIST_DIR"
    for abi in universal $ALL_ABIS; do
        local unsigned="$ANDROID_DIR/app/build/outputs/apk/release/app-$abi-release-unsigned.apk"
        out="$DIST_DIR/HFPredict-$v-$abi.apk"
        if [ -z "${HFPREDICT_KEYSTORE:-}" ]; then
            cp "$unsigned" "${out%.apk}-unsigned.apk"
            continue
        fi
        local args=(sign --ks "$HFPREDICT_KEYSTORE" --out "$out")
        [ -n "${HFPREDICT_KEY_ALIAS:-}" ] && args+=(--ks-key-alias "$HFPREDICT_KEY_ALIAS")
        # Passwords are read from the environment if set, otherwise apksigner asks.
        [ -n "${HFPREDICT_KEYSTORE_PASS:-}" ] && args+=(--ks-pass env:HFPREDICT_KEYSTORE_PASS)
        [ -n "${HFPREDICT_KEY_PASS:-}" ] && args+=(--key-pass env:HFPREDICT_KEY_PASS)
        "$tools/zipalign" -f -p 4 "$unsigned" "$out.aligned"
        "$tools/apksigner" "${args[@]}" "$out.aligned"
        rm -f "$out.aligned" "$out.aligned.idsig"
        "$tools/apksigner" verify "$out" >/dev/null
    done
    [ -z "${HFPREDICT_KEYSTORE:-}" ] && info "HFPREDICT_KEYSTORE not set: APKs left unsigned in $DIST_DIR"
    ls -la "$DIST_DIR"
}

install_debug() {
    build_debug
    info "Installing on connected device"
    local out
    if ! out="$("$ADB" install -r "$DIST_DIR/hfpredict-debug.apk" 2>&1)"; then
        echo "$out" >&2
        # After distclean the debug key is new, and Android refuses the update.
        # Uninstalling wipes the app's data, so leave that to the user.
        case "$out" in
            *INSTALL_FAILED_UPDATE_INCOMPATIBLE*)
                die "Signed with a different debug key. Run: $ADB uninstall $(debug_app_id)" ;;
        esac
        die "adb install failed."
    fi
}

emulator() {
    setup
    sdk_package emulator
    sdk_package "$(echo "$EMULATOR_IMAGE" | tr ';' '/')"
    local avd="$ANDROID_USER_HOME/avd/hfpredict.avd" name="${1:-phone}"
    if [ ! -d "$avd" ]; then
        info "Creating the AVD"
        mkdir -p "$avd"
        cat >"$ANDROID_USER_HOME/avd/hfpredict.ini" <<INI
avd.ini.encoding=UTF-8
path=$avd
target=android-$EMULATOR_API
INI
        cat >"$avd/config.ini" <<INI
image.sysdir.1=system-images/android-$EMULATOR_API/default/x86_64/
abi.type=x86_64
hw.cpu.arch=x86_64
hw.ramSize=3072
hw.gpu.enabled=yes
hw.gpu.mode=swangle_indirect
hw.keyboard=yes
hw.lcd.density=420
hw.lcd.width=1080
hw.lcd.height=2400
disk.dataPartition.size=4G
INI
    fi
    mkdir -p "$ANDROID_DIR/build"
    info "Starting the emulator (headless)"
    ANDROID_AVD_HOME="$ANDROID_USER_HOME/avd" setsid nohup "$SDK_DIR/emulator/emulator" -avd hfpredict \
        -no-window -no-audio -no-boot-anim -no-snapshot-save -gpu swangle_indirect >"$ANDROID_DIR/build/emulator.log" 2>&1 &
    "$ADB" wait-for-device
    until [ "$("$ADB" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do sleep 2; done
    info "Emulator ready"
}

shots() {
    exec "$ANDROID_DIR/tools/screenshots.sh" "$@"
}

logcat() {
    local pid
    pid="$("$ADB" shell pidof -s "$(debug_app_id)" || true)"
    [ -n "$pid" ] || die "$(debug_app_id) is not running."
    "$ADB" logcat --pid="$pid"
}

run_app() {
    install_debug
    info "Launching $(debug_app_id)"
    "$ADB" shell am start -W -n "$(debug_app_id)/io.alvaroedo.hfpredict.MainActivity" >/dev/null
    logcat
}

check() {
    setup
    build_rust "${HFP_ABIS:-x86_64}"
    gradle testDebugUnitTest lintDebug
}

print_env() {
    setup >&2
    printf 'export JAVA_HOME=%q ANDROID_HOME=%q GRADLE_USER_HOME=%q ANDROID_USER_HOME=%q\n' \
        "$JAVA_HOME" "$ANDROID_HOME" "$GRADLE_USER_HOME" "$ANDROID_USER_HOME"
    printf 'export PATH=%q:"$PATH"\n' "$JDK_DIR/bin:$SDK_DIR/platform-tools:$SDK_DIR/cmdline-tools/latest/bin"
    printf 'unset ANDROID_SDK_ROOT\n'
}

# Remove build outputs.
clean() {
    rm -rf "$ANDROID_DIR/build" "$ANDROID_DIR/app/build" "$ANDROID_DIR/.gradle" "$ANDROID_DIR/.kotlin" "$DIST_DIR"
}

# Remove build outputs and the downloaded toolchain.
distclean() {
    clean
    rm -rf "$TOOLS_DIR" "$ANDROID_DIR/local.properties"
}

case "${1:-debug}" in
    debug) build_debug ;;
    release) build_release ;;
    install) install_debug ;;
    run) run_app ;;
    logcat) logcat ;;
    test) check ;;
    gradle) shift; setup; gradle "$@" ;;
    env) print_env ;;
    setup) setup ;;
    rust) setup; build_rust "${HFP_ABIS:-$ALL_ABIS}" ;;
    emulator) shift; emulator "$@" ;;
    shots) shift; shots "$@" ;;
    clean) clean ;;
    distclean) distclean ;;
    -h|--help|help) sed -n '2,27p' "$0" | sed 's/^# \{0,1\}//' ;;
    *) die "Unknown command '$1'. Run ./build.sh help." ;;
esac
