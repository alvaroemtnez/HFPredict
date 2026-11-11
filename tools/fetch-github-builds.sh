#!/usr/bin/env bash
# fetch-github-builds.sh TAG DIR - waits for the GitHub mirror's runs of
# TAG (the macOS and Windows workflows) and copies the .dmg and -setup.exe
# files they built into DIR, so the release carries them and SHA256SUMS covers
# them. Used by .forgejo/workflows/release.yml before the checksums.
#
# Never fails the release: a run that is missing, failed or still going at
# the deadline is reported and skipped (a run that failed in one job still
# gives the other job's files). Those can be attached by hand afterwards
# (docs/releasing.md, "Windows and macOS").
#
# Environment:
#   GITHUB_REPO, GITHUB_TOKEN   the mirror; the token needs "Actions: read"
#   GITHUB_WAIT_MINUTES         how long to wait for the runs (default 120)
#   GITHUB_WORKFLOWS            default "macos.yml windows.yml"
set -uo pipefail

TAG="$1"
DIR="$2"
WORKFLOWS="${GITHUB_WORKFLOWS:-macos.yml windows.yml}"
if [ -z "${GITHUB_TOKEN:-}" ] || [ -z "${GITHUB_REPO:-}" ]; then
    echo "No GITHUB_TOKEN: GitHub builds skipped"
    exit 0
fi
API="https://api.github.com/repos/$GITHUB_REPO"
DEADLINE=$(( $(date +%s) + ${GITHUB_WAIT_MINUTES:-120} * 60 ))
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$DIR"

ghapi() { curl -fsSL --retry 3 -H "Authorization: Bearer $GITHUB_TOKEN" -H "Accept: application/vnd.github+json" "$@"; }

# Prints "id status conclusion" of the latest run of workflow $1 for TAG
# (a tag push's run has the tag as its head_branch), or nothing yet.
latest_run() {
    ghapi "$API/actions/workflows/$1/runs?branch=$TAG&event=push&per_page=1" | python3 -c '
import json, sys
runs = json.load(sys.stdin).get("workflow_runs", [])
if runs:
    r = runs[0]
    print(r["id"], r["status"], r["conclusion"] or "-")'
}

for wf in $WORKFLOWS; do
    echo "==> $wf for $TAG"
    run=""
    while :; do
        line="$(latest_run "$wf" || true)"
        read -r id status conclusion <<<"$line"
        if [ "${status:-}" = completed ]; then
            echo "run $id: $conclusion"
            run="$id"
            break
        fi
        if [ "$(date +%s)" -ge "$DEADLINE" ]; then
            echo "::warning::$wf: no finished run for $TAG in time (${line:-no run}); skipped"
            break
        fi
        echo "${line:-no run yet}; waiting"
        sleep 60
    done
    [ -n "$run" ] || continue

    ghapi "$API/actions/runs/$run/artifacts?per_page=100" | python3 -c '
import json, sys
for a in json.load(sys.stdin).get("artifacts", []):
    if not a["expired"]:
        print(a["name"], a["archive_download_url"])' >"$WORK/list" || { echo "::warning::$wf: no artifact list"; continue; }
    [ -s "$WORK/list" ] || echo "::warning::$wf: run $run has no artifacts"
    while read -r name url; do
        echo "artifact $name"
        # The download redirects to signed storage; curl drops the token there.
        if ghapi -o "$WORK/$name.zip" "$url" && unzip -q -o "$WORK/$name.zip" -d "$WORK/$name"; then
            find "$WORK/$name" -type f \( -name '*.dmg' -o -name '*-setup.exe' \) -exec cp -v {} "$DIR/" \;
        else
            echo "::warning::$wf: could not download $name"
        fi
    done <"$WORK/list"
done
exit 0
