#!/usr/bin/env bash
# screenshots.sh - Captures the debug app on the running emulator (or a
# device) in several states, into ../target/screenshots/android/.
# Start one with `./build.sh emulator` and install with `./build.sh install`.
#
# Usage: tools/screenshots.sh [scenario...]   (default: all)
set -euo pipefail

ANDROID_DIR="$(cd "$(dirname "$0")/.." && pwd)"
eval "$("$ANDROID_DIR/build.sh" env 2>/dev/null)"
OUT="$ANDROID_DIR/../target/screenshots/android"
APP=io.alvaroedo.HFPredict.debug
mkdir -p "$OUT"

state() { # $1 theme, $2 "to" json or null
    cat <<JSON
{ "settings": { "theme": "$1", "units": "auto", "language": null, "model": "truecast", "network": true, "keepMaps": true },
  "stations": [ { "id": "s1", "name": "Home", "mode": "js8", "watts": 5.0,
                  "antenna": { "kind": "dipole", "heightM": 10.0, "gainDbd": 6.0, "beamDeg": 350.0 } } ],
  "activeStation": "s1",
  "from": { "name": "Valencia", "detail": "Spain", "lat": 39.47, "lon": -0.38, "grid": "IM99sl" },
  "to": $2, "band": "20m", "recent": [] }
JSON
}
TOKYO='{ "name": "Tokyo", "detail": "Japan", "lat": 35.69, "lon": 139.69, "grid": "PM95tq" }'

prepare() { # $1 theme, $2 to
    adb shell am force-stop "$APP"
    state "$1" "$2" | adb shell "run-as $APP sh -c 'mkdir -p files && cat > files/state.json'"
}

shot() { # $1 name, $2 seconds to wait
    adb shell am start -W -n "$APP/io.alvaroedo.hfpredict.MainActivity" >/dev/null
    sleep "${2:-8}"
    adb exec-out screencap -p >"$OUT/$1.png"
    echo "captured $1"
}

tap_menu() { # opens a dialog through the overflow menu: $1 item index (0-based)
    local size w
    size=$(adb shell wm size | tail -1 | awk '{print $3}')
    w=${size%x*}
    adb shell input tap $((w - 60)) 160
    sleep 1
    adb shell input tap $((w - 300)) $((240 + 150 * $1))
    sleep 2
}

phone() { adb shell wm size reset; adb shell wm density reset; }
tablet() { adb shell wm size 2560x1600; adb shell wm density 320; }

want() { [ $# -eq 0 ] || [[ " $* " == *" $1 "* ]]; }
ALL=("$@")
run() { want "$1" ${ALL[@]+"${ALL[@]}"}; }

phone
if run phone_dark; then adb shell cmd uimode night yes; prepare system "$TOKYO"; shot phone_dark 10; fi
if run phone_light; then adb shell cmd uimode night no; prepare system null; shot phone_light 10; fi
if run phone_lowlight; then prepare lowLight null; shot phone_lowlight 10; fi
if run phone_station; then adb shell cmd uimode night yes; prepare system "$TOKYO"; shot phone_station_base 8; adb shell input tap 300 330; sleep 2; adb exec-out screencap -p >"$OUT/phone_station.png"; echo captured phone_station; fi
if run phone_prefs; then prepare system "$TOKYO"; shot phone_prefs_base 6; tap_menu 1; adb exec-out screencap -p >"$OUT/phone_prefs.png"; echo captured phone_prefs; fi
if run firstrun; then adb shell am force-stop "$APP"; adb shell "run-as $APP rm -f files/state.json"; shot firstrun 4; fi
if run tablet_dark; then tablet; adb shell cmd uimode night yes; prepare system "$TOKYO"; shot tablet_dark 12; phone; fi
if run tablet_light; then tablet; adb shell cmd uimode night no; prepare system "$TOKYO"; shot tablet_light 12; phone; fi
adb shell cmd uimode night no
ls -la "$OUT"
