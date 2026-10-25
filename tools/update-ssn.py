#!/usr/bin/env python3
"""Regenerate core/data/ssn.json from NOAA SWPC's solar-cycle feeds.

Run by hand before a release (see docs/releasing.md). The table drives the
VOACAP model offline: observed twelve-month smoothed sunspot numbers where
NOAA has them ("c"), its prediction for later months ("f").

Usage: tools/update-ssn.py [--since 2020-01]
"""
import json
import sys
import urllib.request
from datetime import date

BASE = "https://services.swpc.noaa.gov/json/solar-cycle/"
OUT = "core/data/ssn.json"


def fetch(name):
    with urllib.request.urlopen(BASE + name, timeout=30) as response:
        return json.load(response)


def main():
    since = sys.argv[sys.argv.index("--since") + 1] if "--since" in sys.argv else "2020-01"
    months = {}
    for row in fetch("observed-solar-cycle-indices.json"):
        tag, smoothed = row["time-tag"], row.get("smoothed_ssn", -1)
        if tag >= since and smoothed is not None and smoothed >= 0:
            months[tag] = [round(smoothed, 1), "c"]
    for row in fetch("predicted-solar-cycle.json"):
        tag = row["time-tag"]
        if tag >= since and tag not in months:
            months[tag] = [round(row["predicted_ssn"], 1), "f"]
    table = {
        "source": "NOAA SWPC solar-cycle indices, observed and predicted",
        "fetched": date.today().isoformat(),
        "note": "c = observed twelve-month smoothed SSN, f = SWPC prediction",
        "months": dict(sorted(months.items())),
    }
    with open(OUT, "w") as out:
        json.dump(table, out, indent=1)
        out.write("\n")
    tags = sorted(months)
    print(f"{OUT}: {len(tags)} months, {tags[0]} to {tags[-1]}")


if __name__ == "__main__":
    main()
