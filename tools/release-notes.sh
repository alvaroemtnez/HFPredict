#!/usr/bin/env bash
# release-notes.sh VERSION - prints that version's CHANGELOG.md entry (the
# lines under "## VERSION", up to the next "## "). Fails if there is none,
# so a release cannot go out without notes.
set -euo pipefail
v="${1#v}"
notes=$(awk -v v="$v" '
    /^## / { if (found) exit; if ($2 == v) { found = 1; next } }
    found { print }
' "$(dirname "$0")/../CHANGELOG.md")
[ -n "$(echo "$notes" | tr -d '[:space:]')" ] || { echo "CHANGELOG.md has no entry for $v" >&2; exit 1; }
printf '%s\n' "$notes"
