#!/usr/bin/env python3
"""Generates every app icon from one description, so the GNOME icon, the
symbolic icon and the Android adaptive icon cannot drift apart.

The mark is the app's own map: an earth disc seen from the station, a
blob of rounded cells where the band reaches (brightest near the station,
like the quality ramp), a dashed distance ring and the station's dot.
The cell field nods to HFcast's icon, which this app's design grew from.

Usage: tools/gen-icons.py        (run from the repository root)
"""
import math
import os

# Colours: deep-sea disc, a teal quality ramp, the amber station.
DISC = "#0F2B3A"
DISC_RIM = "#1C4457"
RAMP = ["#1E6E86", "#2F9EBB", "#86DDEB"]  # weak, patchy, reliable
STATION = "#FFC23D"
RING = "#FFFFFF"

# Geometry on a 100 x 100 design square, disc centred.
C = 50.0
R = 44.0
PITCH = 6.3
CELL = 5.3
STATION_XY = (43.0, 57.0)


def reach(x, y):
    """0..1: how well the band reaches this point. A lobed blob around
    the station, stronger to the north-east, as a real map tends to be."""
    sx, sy = STATION_XY
    dx, dy = x - sx, y - sy
    d = math.hypot(dx, dy)
    angle = math.atan2(-dy, dx)
    lobe = 31.0 + 8.0 * math.cos(angle - 0.8) + 2.5 * math.cos(3 * angle + 0.4)
    return max(0.0, 1.0 - d / lobe)


def cells():
    out = []
    n = int(2 * R / PITCH) + 2
    for i in range(-n, n + 1):
        for j in range(-n, n + 1):
            x = C + i * PITCH
            y = C + j * PITCH
            # Whole cells only, inside the disc with a margin.
            if math.hypot(x - C, y - C) > R - CELL * 0.75:
                continue
            r = reach(x, y)
            if r <= 0.08:
                continue
            level = 2 if r > 0.58 else 1 if r > 0.3 else 0
            out.append((x - CELL / 2, y - CELL / 2, level))
    return out


def ring_points(radius, dashes=28):
    for k in range(dashes):
        a0 = 2 * math.pi * k / dashes
        a1 = a0 + math.pi / dashes
        yield a0, a1, radius


def svg(size=128, symbolic=False):
    s = size / 100.0
    parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 0 100 100">']
    if symbolic:
        fg = "#241f31"
        parts.append(f'<circle cx="{C}" cy="{C}" r="{R + 2}" fill="none" stroke="{fg}" stroke-width="7"/>')
        for x, y, level in cells():
            if level >= 1:
                parts.append(f'<rect x="{x:.2f}" y="{y:.2f}" width="{CELL}" height="{CELL}" rx="1.6" fill="{fg}" opacity="{0.55 if level == 1 else 1}"/>')
        sx, sy = STATION_XY
        parts.append(f'<circle cx="{sx}" cy="{sy}" r="6" fill="#ffffff"/><circle cx="{sx}" cy="{sy}" r="4" fill="{fg}"/>')
        parts.append("</svg>")
        return "\n".join(parts)
    # GNOME style: a soft drop shadow under the disc, then the disc.
    parts.append(f'<ellipse cx="{C}" cy="{C + 4}" rx="{R + 1}" ry="{R}" fill="#000000" opacity="0.18"/>')
    parts.append(f'<circle cx="{C}" cy="{C}" r="{R + 2}" fill="{DISC_RIM}"/>')
    parts.append(f'<circle cx="{C}" cy="{C}" r="{R}" fill="{DISC}"/>')
    for x, y, level in cells():
        parts.append(f'<rect x="{x:.2f}" y="{y:.2f}" width="{CELL}" height="{CELL}" rx="1.3" fill="{RAMP[level]}"/>')
    sx, sy = STATION_XY
    for a0, a1, rr in ring_points(36.0, 34):
        x0, y0 = sx + rr * math.cos(a0), sy + rr * math.sin(a0)
        x1, y1 = sx + rr * math.cos(a1), sy + rr * math.sin(a1)
        if math.hypot(x0 - C, y0 - C) < R - 2 and math.hypot(x1 - C, y1 - C) < R - 2:
            parts.append(f'<path d="M{x0:.2f} {y0:.2f} A{rr} {rr} 0 0 1 {x1:.2f} {y1:.2f}" stroke="{RING}" stroke-opacity="0.45" stroke-width="1.2" fill="none" stroke-linecap="round"/>')
    parts.append(f'<circle cx="{sx}" cy="{sy}" r="6.2" fill="{DISC}"/>')
    parts.append(f'<circle cx="{sx}" cy="{sy}" r="4.4" fill="{STATION}"/>')
    parts.append("</svg>")
    return "\n".join(parts)


def rounded_rect_path(x, y, w, h, r):
    return (f"M{x + r:.2f},{y:.2f}h{w - 2 * r:.2f}a{r},{r} 0 0 1 {r},{r}v{h - 2 * r:.2f}"
            f"a{r},{r} 0 0 1 -{r},{r}h-{w - 2 * r:.2f}a{r},{r} 0 0 1 -{r},-{r}v-{h - 2 * r:.2f}a{r},{r} 0 0 1 {r},-{r}z")


def android_vector(monochrome=False):
    """The adaptive icon's foreground on a 108 dp canvas: the design square
    scaled into the 66 dp safe zone, centred."""
    k = 66.0 / 100.0 * 1.06
    off = (108 - 100 * k) / 2

    def t(v):
        return off + v * k

    paths = []
    if not monochrome:
        paths.append((f"M{t(C - R):.2f},{t(C):.2f}a{R * k:.2f},{R * k:.2f} 0 1,0 {2 * R * k:.2f},0a{R * k:.2f},{R * k:.2f} 0 1,0 -{2 * R * k:.2f},0", DISC))
    by_level = {0: [], 1: [], 2: []}
    for x, y, level in cells():
        by_level[level].append(rounded_rect_path(t(x), t(y), CELL * k, CELL * k, 1.6 * k))
    for level in (0, 1, 2):
        if monochrome and level == 0:
            continue
        colour = "#FFFFFFFF" if monochrome else RAMP[level]
        paths.append(("".join(by_level[level]), colour if not monochrome else ("#99FFFFFF" if level == 1 else "#FFFFFFFF")))
    sx, sy = STATION_XY
    if not monochrome:
        r = 6.2 * k
        paths.append((f"M{t(sx) - r:.2f},{t(sy):.2f}a{r:.2f},{r:.2f} 0 1,0 {2 * r:.2f},0a{r:.2f},{r:.2f} 0 1,0 -{2 * r:.2f},0", DISC))
    r = 4.4 * k
    paths.append((f"M{t(sx) - r:.2f},{t(sy):.2f}a{r:.2f},{r:.2f} 0 1,0 {2 * r:.2f},0a{r:.2f},{r:.2f} 0 1,0 -{2 * r:.2f},0", "#FFFFFFFF" if monochrome else STATION))
    body = "\n".join(f'    <path android:fillColor="{c}" android:pathData="{d}"/>' for d, c in paths)
    return ('<?xml version="1.0" encoding="utf-8"?>\n'
            '<!-- Generated by tools/gen-icons.py; edit that, not this. -->\n'
            '<vector xmlns:android="http://schemas.android.com/apk/res/android"\n'
            '    android:width="108dp" android:height="108dp"\n'
            '    android:viewportWidth="108" android:viewportHeight="108">\n'
            f"{body}\n</vector>\n")


def write(path, text):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        f.write(text)
    print(path)


def main():
    app = "io.alvaroedo.HFPredict"
    write(f"gnome/data/icons/hicolor/scalable/apps/{app}.svg", svg())
    write(f"gnome/data/icons/hicolor/symbolic/apps/{app}-symbolic.svg", svg(16, symbolic=True))
    res = "android/app/src/main/res"
    write(f"{res}/drawable/ic_launcher_foreground.xml", android_vector())
    write(f"{res}/drawable/ic_launcher_monochrome.xml", android_vector(monochrome=True))
    write(f"{res}/values/ic_launcher_background.xml",
          '<?xml version="1.0" encoding="utf-8"?>\n<resources>\n'
          f'    <color name="ic_launcher_background">{DISC_RIM}</color>\n</resources>\n')
    write(f"{res}/mipmap-anydpi/ic_launcher.xml",
          '<?xml version="1.0" encoding="utf-8"?>\n'
          '<adaptive-icon xmlns:android="http://schemas.android.com/apk/res/android">\n'
          '    <background android:drawable="@color/ic_launcher_background"/>\n'
          '    <foreground android:drawable="@drawable/ic_launcher_foreground"/>\n'
          '    <monochrome android:drawable="@drawable/ic_launcher_monochrome"/>\n'
          '</adaptive-icon>\n')


if __name__ == "__main__":
    main()
