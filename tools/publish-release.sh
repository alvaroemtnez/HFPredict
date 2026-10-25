#!/usr/bin/env bash
# publish-release.sh TAG DIR - creates the release for TAG on Forgejo and on
# the GitHub mirror, with every file in DIR attached and the CHANGELOG.md
# entry as its text. Used by .forgejo/workflows/release.yml.
#
# Environment:
#   FORGEJO_URL, FORGEJO_REPO, FORGEJO_TOKEN   e.g. https://code.alvaroedo.io, alvaroemtnez/HFPredict
#   GITHUB_REPO, GITHUB_TOKEN                  optional: the mirror (alvaroemtnez/HFPredict)
#   TARGET_SHA                                 the tagged commit (the mirror may not have the tag yet)
set -euo pipefail

TAG="$1"
DIR="$2"
VERSION="${TAG#v}"
NOTES=$("$(dirname "$0")/release-notes.sh" "$VERSION")
BODY=$(python3 -c 'import json,sys; print(json.dumps({"tag_name": sys.argv[1], "name": "HFPredict " + sys.argv[2], "body": sys.argv[3], "target_commitish": sys.argv[4]}))' \
    "$TAG" "$VERSION" "$NOTES" "${TARGET_SHA:-main}")
json_field() { python3 -c 'import json,sys; print(json.load(sys.stdin)[sys.argv[1]])' "$1"; }

forgejo() {
    local api="$FORGEJO_URL/api/v1/repos/$FORGEJO_REPO" id
    id=$(curl -fsS -X POST "$api/releases" -H "Authorization: token $FORGEJO_TOKEN" \
        -H "Content-Type: application/json" -d "$BODY" | json_field id)
    for f in "$DIR"/*; do
        echo "forgejo: $(basename "$f")"
        curl -fsS -X POST "$api/releases/$id/assets?name=$(basename "$f")" -H "Authorization: token $FORGEJO_TOKEN" \
            -F "attachment=@$f" >/dev/null
    done
}

github() {
    local id
    id=$(curl -fsS -X POST "https://api.github.com/repos/$GITHUB_REPO/releases" \
        -H "Authorization: Bearer $GITHUB_TOKEN" -H "Accept: application/vnd.github+json" \
        -d "$BODY" | json_field id)
    for f in "$DIR"/*; do
        echo "github: $(basename "$f")"
        curl -fsS -X POST "https://uploads.github.com/repos/$GITHUB_REPO/releases/$id/assets?name=$(basename "$f")" \
            -H "Authorization: Bearer $GITHUB_TOKEN" -H "Content-Type: application/octet-stream" \
            --data-binary "@$f" >/dev/null
    done
}

forgejo
if [ -n "${GITHUB_TOKEN:-}" ] && [ -n "${GITHUB_REPO:-}" ]; then github; else echo "No GITHUB_TOKEN: mirror release skipped"; fi
