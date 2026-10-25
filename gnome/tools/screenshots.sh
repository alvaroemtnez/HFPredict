#!/usr/bin/env bash
# Runs inside the GNOME container (gnome/build.sh screenshot): starts a
# headless Wayland compositor and captures the app in several states into
# target/screenshots/. Pass scenario names to capture only those.
set -euo pipefail

OUT=target/screenshots
BIN=target/gnome/release/hfpredict
mkdir -p "$OUT"
export XDG_RUNTIME_DIR=/tmp/xdg
mkdir -p "$XDG_RUNTIME_DIR" && chmod 700 "$XDG_RUNTIME_DIR"
export WAYLAND_DISPLAY=wayland-hfp GDK_BACKEND=wayland GSK_RENDERER=cairo
export LANG=C.UTF-8 LC_ALL=C.UTF-8
weston --backend=headless --socket="$WAYLAND_DISPLAY" --width=2600 --height=1800 --idle-time=0 >/tmp/weston.log 2>&1 &
WESTON=$!
trap 'kill $WESTON 2>/dev/null || true' EXIT
for _ in $(seq 50); do [ -S "$XDG_RUNTIME_DIR/$WAYLAND_DISPLAY" ] && break; sleep 0.1; done

# A saved state: Valencia to Tokyo with a dipole, network on.
state() { # $1 dir, $2 theme, $3 "to" json or null, $4 language or null
    mkdir -p "$1/data"
    cat >"$1/data/state.json" <<JSON
{ "settings": { "theme": "$2", "units": "metric", "language": $4, "model": "truecast", "network": true, "keepMaps": true },
  "stations": [ { "id": "s1", "name": "Home", "mode": "js8", "watts": 5.0,
                  "antenna": { "kind": "dipole", "heightM": 10.0, "gainDbd": 6.0, "beamDeg": 350.0 } },
                { "id": "s2", "name": "Portable", "mode": "ssb", "watts": 10.0,
                  "antenna": { "kind": "vertical", "heightM": 6.0, "gainDbd": 6.0, "beamDeg": 0.0 } } ],
  "activeStation": "s1",
  "from": { "name": "Valencia", "detail": "Spain", "lat": 39.47, "lon": -0.38, "grid": "IM99sl" },
  "to": $3, "band": "20m", "recent": [] }
JSON
}
TOKYO='{ "name": "Tokyo", "detail": "Japan", "lat": 35.69, "lon": 139.69, "grid": "PM95tq" }'

shot() { # $1 name, $2 size, then env assignments
    local name=$1 size=$2; shift 2
    local dir=/tmp/hfp-$name
    rm -rf "$dir"; mkdir -p "$dir"
    if [[ " ${ONLY:-} " != "  " && " $ONLY " != *" $name "* ]]; then return; fi
    "$@" dir="$dir" true
    setup_$name "$dir"
    env HFPREDICT_DATA_DIR="$dir/data" HFPREDICT_CACHE_DIR="$dir/cache" HFPREDICT_SIZE="$size" \
        HFPREDICT_SCREENSHOT="$PWD/$OUT/$name.png" HFPREDICT_DEBUG=1 GTK_A11Y=none ${EXTRA:-} \
        dbus-run-session -- "$BIN" >"$OUT/$name.log" 2>&1 || { echo "!! $name failed"; tail -20 "$OUT/$name.log"; }
    [ -f "$OUT/$name.png" ] && echo "captured $name"
}

ONLY="${*:-}"
setup_wide_dark() { state "$1" dark "$TOKYO" null; }
setup_wide_light() { state "$1" light "$TOKYO" null; }
setup_phone_dark() { state "$1" dark null null; }
setup_phone_light() { state "$1" light "$TOKYO" null; }
setup_lowlight() { state "$1" lowLight null null; }
setup_firstrun() { mkdir -p "$1/data"; }
setup_station() { state "$1" dark "$TOKYO" null; }
setup_path() { state "$1" light "$TOKYO" null; }
setup_preferences() { state "$1" dark "$TOKYO" null; }
setup_help() { state "$1" light "$TOKYO" null; }
setup_about() { state "$1" dark "$TOKYO" null; }
setup_zoom() { state "$1" dark null null; }
setup_fullscreen() { state "$1" dark "$TOKYO" null; }
setup_probe() { state "$1" light null null; }
setup_firstrun_search() { mkdir -p "$1/data"; }
setup_path_search() { state "$1" dark "$TOKYO" null; }
setup_coarse() { state "$1" dark null null; }
setup_spanish() { state "$1" light "$TOKYO" '"es"'; }
setup_arabic() { state "$1" dark "$TOKYO" '"ar"'; }
setup_valencian_station() { state "$1" light "$TOKYO" '"ca"'; }

EXTRA="" shot wide_dark 1400x1000 true
EXTRA="" shot wide_light 1400x1000 true
EXTRA="" shot phone_dark 400x1700 true
EXTRA="" shot phone_light 400x1700 true
EXTRA="" shot lowlight 400x1200 true
EXTRA="" shot firstrun 420x900 true
EXTRA="HFPREDICT_OPEN=station" shot station 900x1000 true
EXTRA="HFPREDICT_OPEN=path" shot path 900x900 true
EXTRA="HFPREDICT_OPEN=preferences" shot preferences 900x900 true
EXTRA="HFPREDICT_OPEN=help" shot help 900x900 true
EXTRA="HFPREDICT_OPEN=about" shot about 900x900 true
EXTRA="HFPREDICT_BAND=40m HFPREDICT_ZOOM=5" shot zoom 1400x1000 true
EXTRA="HFPREDICT_OPEN=fullscreen HFPREDICT_PROBE=0.62,0.38" shot fullscreen 1400x1000 true
EXTRA="HFPREDICT_PROBE=0.55,0.42 HFPREDICT_ZOOM=2.5" shot probe 1400x1000 true
EXTRA="HFPREDICT_SEARCH=Val" shot firstrun_search 420x900 true
EXTRA="HFPREDICT_OPEN=path HFPREDICT_SEARCH=Tok" shot path_search 900x900 true
# The smoothed coarse preview, caught before the fine map arrives.
EXTRA="HFPREDICT_SCREENSHOT_DELAY=1" shot coarse 1400x1000 true
EXTRA="" shot spanish 1400x1000 true
EXTRA="" shot arabic 1400x1000 true
EXTRA="HFPREDICT_OPEN=station" shot valencian_station 900x1000 true
ls -la "$OUT"
