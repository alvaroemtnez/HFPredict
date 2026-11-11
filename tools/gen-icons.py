#!/usr/bin/env python3
"""Generates every app icon from one description, so the desktop icon, the
symbolic icon and the Android icons cannot drift apart.

The mark is the app's own map: a field of rounded cells where the band
reaches, coloured by the quality ramp (brightest near the station), and the
station's dot. It sits on a flat ground with no rim, centred on the field's
own extent, so every platform shows the same art and only the corner shape
changes. The cell field nods to HFcast's icon, which this design grew from.

Android offers every colourway in Preferences (launcher activity-aliases);
everywhere else the icon is the first one, Deep sea.

Small sizes get their own coarse grid (the symbolic icon, the status-bar
icon): the fine field turns to mush below 32 px.

Usage: tools/gen-icons.py        (run from the repository root; rsvg-convert,
                                  if present, also renders the PNGs)
"""
import math
import os
import shutil
import subprocess

APP = "io.alvaroedo.HFPredict"

# (id, ground, quality ramp weak -> reliable, station). The first is the default.
COLOURWAYS = [
    ("deepSea", "#0F2B3A", ["#1E6E86", "#2F9EBB", "#86DDEB"], "#FFC23D"),
    ("night", "#1D1B3A", ["#3F3A8A", "#6E66D1", "#BDB6FF"], "#FFC23D"),
    ("sunspot", "#2A160B", ["#7A3812", "#C8641F", "#FFB46B"], "#FFFFFF"),
    ("bandOpen", "#161B22", ["#B4442F", "#E0A31B", "#47C27A"], "#FFFFFF"),
    ("aurora", "#0C2421", ["#17665A", "#2BA889", "#9BF0CF"], "#FFC23D"),
    ("daylight", "#EAF2F5", ["#9CCBDA", "#3F95B5", "#0F5E7C"], "#E8820C"),
]

# The field, on a 100-unit design square.
C = 50.0
R = 44.0
PITCH = 6.3
CELL = 5.3
CORNER = 1.3
STATION = (43.0, 57.0)
HALO = 6.2
DOT = 4.4
# The field's half-extent as a share of the visible icon (Android's 72 dp
# window, the desktop tile): 0.64 leaves the ground a clear margin and keeps
# the field inside Android's 66 dp safe circle.
FILL = 0.64


def reach(x, y):
    """0..1: how well the band reaches this point. A lobed blob around
    the station, stronger to the north-east, as a real map tends to be."""
    sx, sy = STATION
    dx, dy = x - sx, y - sy
    d = math.hypot(dx, dy)
    angle = math.atan2(-dy, dx)
    lobe = 31.0 + 8.0 * math.cos(angle - 0.8) + 2.5 * math.cos(3 * angle + 0.4)
    return max(0.0, 1.0 - d / lobe)


def centre(cells, size):
    """Shift cells (x, y, level centres) so their bounding box is centred on C;
    returns the cells, the station and the half-extent."""
    xs = [c[0] for c in cells]
    ys = [c[1] for c in cells]
    dx = C - (min(xs) + max(xs)) / 2
    dy = C - (min(ys) + max(ys)) / 2
    moved = [(x + dx, y + dy, level) for x, y, level in cells]
    extent = max(max(abs(x - C), abs(y - C)) for x, y, _ in moved) + size / 2
    return moved, (STATION[0] + dx, STATION[1] + dy), extent


def fine_field():
    out = []
    n = int(2 * R / PITCH) + 2
    for i in range(-n, n + 1):
        for j in range(-n, n + 1):
            x, y = C + i * PITCH, C + j * PITCH
            if math.hypot(x - C, y - C) > R - CELL * 0.75:
                continue
            r = reach(x, y)
            if r <= 0.08:
                continue
            out.append((x, y, 2 if r > 0.58 else 1 if r > 0.3 else 0))
    return centre(out, CELL)


def coarse_field(pitch=12.6):
    out = []
    for i in range(-4, 5):
        for j in range(-4, 5):
            x, y = STATION[0] + i * pitch, STATION[1] + j * pitch
            r = reach(x, y)
            if r > 0.12:
                out.append((x, y, 2 if r > 0.5 else 1))
    return centre(out, pitch * 0.78)


def placed(field, half):
    """The field scaled so its half-extent is `half` around (C, C): a list of
    (x, y, size, corner, level) squares plus the station and a scale factor."""
    cells, station, extent = field
    k = half / extent
    size = CELL * k

    def t(v):
        return C + (v - C) * k

    return [(t(x) - size / 2, t(y) - size / 2, size, CORNER * k, lvl) for x, y, lvl in cells], (t(station[0]), t(station[1])), k


def placed_coarse(half, pitch=12.6):
    cells, station, extent = coarse_field(pitch)
    k = half / extent
    size = pitch * 0.78 * k

    def t(v):
        return C + (v - C) * k

    return [(t(x) - size / 2, t(y) - size / 2, size, size * 0.28, lvl) for x, y, lvl in cells], (t(station[0]), t(station[1])), size


# ---------------------------------------------------------------- desktop

def desktop_svg(way=COLOURWAYS[0]):
    """A rounded tile on the 128 grid (80% of the canvas, the share GNOME's
    and macOS's square icons use), a soft contact shadow, the field."""
    _, ground, ramp, station = way
    tile, x0, radius = 80.0, 10.0, 18.0
    cells, (sx, sy), k = placed(fine_field(), FILL * 50)
    s = tile / 100.0
    parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="128" height="128" viewBox="0 0 100 100">',
             f'<rect x="{x0}" y="{x0 + 1.6}" width="{tile}" height="{tile}" rx="{radius}" fill="#000000" opacity="0.2"/>',
             f'<rect x="{x0}" y="{x0}" width="{tile}" height="{tile}" rx="{radius}" fill="{ground}"/>',
             f'<g transform="translate({x0} {x0}) scale({s})">']
    for x, y, size, corner, level in cells:
        parts.append(f'<rect x="{x:.2f}" y="{y:.2f}" width="{size:.2f}" height="{size:.2f}" rx="{corner:.2f}" fill="{ramp[level]}"/>')
    parts.append(f'<circle cx="{sx:.2f}" cy="{sy:.2f}" r="{HALO * k:.2f}" fill="{ground}"/>')
    parts.append(f'<circle cx="{sx:.2f}" cy="{sy:.2f}" r="{DOT * k:.2f}" fill="{station}"/>')
    parts.append("</g></svg>")
    return "\n".join(parts) + "\n"


def symbolic_svg():
    """GNOME symbolic: the coarse field in one ink; the weak ring at half
    opacity, the station in a clearing."""
    fg = "#241f31"
    cells, (sx, sy), size = placed_coarse(49.0)
    parts = ['<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 100 100">']
    near = size * 0.95
    for x, y, sz, corner, level in cells:
        if math.hypot(x + sz / 2 - sx, y + sz / 2 - sy) < near:
            continue
        parts.append(f'<rect x="{x:.2f}" y="{y:.2f}" width="{sz:.2f}" height="{sz:.2f}" rx="{corner:.2f}" fill="{fg}" opacity="{0.45 if level == 1 else 1}"/>')
    parts.append(f'<circle cx="{sx:.2f}" cy="{sy:.2f}" r="{size * 0.6:.2f}" fill="{fg}"/>')
    parts.append("</svg>")
    return "\n".join(parts) + "\n"


# ---------------------------------------------------------------- Android

def circle_path(cx, cy, r):
    return f"M{cx - r:.2f},{cy:.2f}a{r:.2f},{r:.2f} 0 1,0 {2 * r:.2f},0a{r:.2f},{r:.2f} 0 1,0 -{2 * r:.2f},0"


def rounded_rect_path(x, y, w, h, r):
    return (f"M{x + r:.2f},{y:.2f}h{w - 2 * r:.2f}a{r:.2f},{r:.2f} 0 0 1 {r:.2f},{r:.2f}v{h - 2 * r:.2f}"
            f"a{r:.2f},{r:.2f} 0 0 1 -{r:.2f},{r:.2f}h-{w - 2 * r:.2f}a{r:.2f},{r:.2f} 0 0 1 -{r:.2f},-{r:.2f}v-{h - 2 * r:.2f}"
            f"a{r:.2f},{r:.2f} 0 0 1 {r:.2f},-{r:.2f}z")


def vector(paths, size=108, comment="", ignore_unused=False):
    def alpha(a):
        return "" if a is None else ' android:fillAlpha="%s"' % a

    body = "\n".join(f'    <path android:fillColor="{c}"{alpha(a)} android:pathData="{d}"/>' for d, c, a in paths)
    tools = ' xmlns:tools="http://schemas.android.com/tools"\n    tools:ignore="UnusedResources"' if ignore_unused else ""
    return ('<?xml version="1.0" encoding="utf-8"?>\n'
            f'<!-- Generated by tools/gen-icons.py; edit that, not this.{comment} -->\n'
            f'<vector xmlns:android="http://schemas.android.com/apk/res/android"{tools}\n'
            f'    android:width="{size}dp" android:height="{size}dp"\n'
            f'    android:viewportWidth="{size}" android:viewportHeight="{size}">\n'
            f"{body}\n</vector>\n")


def android_foreground(way=None):
    """The adaptive icon's foreground on the 108 dp canvas. The launcher shows
    the inner 72 dp; the field's half-extent is FILL of that window. With
    `way` None it is the monochrome (themed) layer: one ink, levels as alpha,
    the station in a clearing instead of a halo."""
    window = 72.0
    cells, (sx, sy), k = placed(fine_field(), FILL * 50)
    s = window / 100.0
    off = (108 - window) / 2

    def t(v):
        return off + v * s

    paths = []
    near = (HALO + CELL * 0.75) * k
    for x, y, size, corner, level in cells:
        if way is None:
            if level == 0 or math.hypot(x + size / 2 - sx, y + size / 2 - sy) < near:
                continue
            colour, alpha = "#FFFFFFFF", ("0.55" if level == 1 else None)
        else:
            colour, alpha = way[2][level], None
        # One short path per cell: lint (rightly) objects to long vector paths.
        paths.append((rounded_rect_path(t(x), t(y), size * s, size * s, corner * s), colour, alpha))
    if way is None:
        paths.append((circle_path(t(sx), t(sy), DOT * k * s * 1.2), "#FFFFFFFF", None))
    else:
        paths.append((circle_path(t(sx), t(sy), HALO * k * s), way[1], None))
        paths.append((circle_path(t(sx), t(sy), DOT * k * s), way[3], None))
    return vector(paths, comment=f" Colourway {way[0]}." if way else " Themed (monochrome) layer.")


def android_status():
    """Status-bar icon (24 dp, white with alpha): the coarse field."""
    cells, (sx, sy), size = placed_coarse(49.0)
    s = 22.0 / 100.0
    off = 1.0

    def t(v):
        return off + v * s

    paths = []
    for x, y, sz, corner, level in cells:
        if math.hypot(x + sz / 2 - sx, y + sz / 2 - sy) < size * 0.95:
            continue
        paths.append((rounded_rect_path(t(x), t(y), sz * s, sz * s, corner * s), "#FFFFFFFF", "0.5" if level == 1 else None))
    paths.append((circle_path(t(sx), t(sy), size * 0.6 * s), "#FFFFFFFF", None))
    return vector(paths, size=24, comment=" Status-bar icon, for notifications.", ignore_unused=True)


def android_name(way_id):
    return "".join("_" + ch.lower() if ch.isupper() else ch for ch in way_id)


# ---------------------------------------------------------------- output

def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(text)
    print(path)


def render_png(svg_text, path, size):
    rsvg = shutil.which("rsvg-convert")
    if not rsvg:
        print(f"skipped {path} (no rsvg-convert)")
        return
    os.makedirs(os.path.dirname(path), exist_ok=True)
    subprocess.run([rsvg, "-w", str(size), "-h", str(size), "-o", path], input=svg_text.encode(), check=True)
    print(path)


def store_icon_svg(way=COLOURWAYS[0]):
    """Full-bleed square for stores (F-Droid crops it itself): the Android
    art at the same proportions."""
    _, ground, ramp, station = way
    cells, (sx, sy), k = placed(fine_field(), FILL * 50)
    parts = ['<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 100 100">',
             f'<rect width="100" height="100" fill="{ground}"/>']
    for x, y, size, corner, level in cells:
        parts.append(f'<rect x="{x:.2f}" y="{y:.2f}" width="{size:.2f}" height="{size:.2f}" rx="{corner:.2f}" fill="{ramp[level]}"/>')
    parts.append(f'<circle cx="{sx:.2f}" cy="{sy:.2f}" r="{HALO * k:.2f}" fill="{ground}"/>')
    parts.append(f'<circle cx="{sx:.2f}" cy="{sy:.2f}" r="{DOT * k:.2f}" fill="{station}"/>')
    parts.append("</svg>")
    return "\n".join(parts)


def main():
    write(f"gnome/data/icons/hicolor/scalable/apps/{APP}.svg", desktop_svg())
    write(f"gnome/data/icons/hicolor/symbolic/apps/{APP}-symbolic.svg", symbolic_svg())
    res = "android/app/src/main/res"
    colours = []
    for way in COLOURWAYS:
        name = android_name(way[0])
        write(f"{res}/drawable/ic_launcher_foreground_{name}.xml", android_foreground(way))
        write(f"{res}/mipmap-anydpi/ic_launcher_{name}.xml",
              '<?xml version="1.0" encoding="utf-8"?>\n'
              '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
              f'    <background android:drawable="@color/ic_launcher_background_{name}"/>\n'
              f'    <foreground android:drawable="@drawable/ic_launcher_foreground_{name}"/>\n'
              '    <monochrome android:drawable="@drawable/ic_launcher_monochrome"/>\n'
              '</adaptive-icon>\n')
        colours.append(f'    <color name="ic_launcher_background_{name}">{way[1]}</color>')
    write(f"{res}/drawable/ic_launcher_monochrome.xml", android_foreground(None))
    write(f"{res}/drawable/ic_stat_hfpredict.xml", android_status())
    write(f"{res}/values/ic_launcher_background.xml",
          '<?xml version="1.0" encoding="utf-8"?>\n<!-- Generated by tools/gen-icons.py. -->\n<resources>\n'
          + "\n".join(colours) + "\n</resources>\n")
    render_png(store_icon_svg(), "fastlane/metadata/android/en-US/images/icon.png", 512)


if __name__ == "__main__":
    main()
