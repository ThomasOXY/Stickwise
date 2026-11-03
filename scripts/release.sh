#!/bin/sh
# Release a new version: bump it everywhere, commit, tag and push.
# GitHub Actions (.github/workflows/release.yml) then builds the DMG and the update.
#
#   scripts/release.sh 0.2.0              notes from the feat/fix commits
#   scripts/release.sh 0.2.0 notes.md     notes from a file (Markdown)
set -e
cd "$(dirname "$0")/.."

v=${1#v}
echo "$v" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || { echo "usage: $0 X.Y.Z [notes.md]" >&2; exit 1; }
[ -z "$2" ] || [ -f "$2" ] || { echo "no such notes file: $2" >&2; exit 1; }
[ -z "$(git status --porcelain)" ] || { echo "commit or stash your changes first" >&2; exit 1; }
git rev-parse -q --verify "refs/tags/v$v" >/dev/null && { echo "tag v$v already exists" >&2; exit 1; }

# The workspace version is the app's (app/src-tauri uses version.workspace).
sed -i '' -E "/^\[workspace.package\]/,/^\[/ s/^version = \".*\"/version = \"$v\"/" Cargo.toml
sed -i '' -E "s/^  \"version\": \".*\",/  \"version\": \"$v\",/" app/src-tauri/tauri.conf.json
(cd app && npm version "$v" --no-git-tag-version --allow-same-version >/dev/null)
cargo update --workspace --offline -q

git add Cargo.toml Cargo.lock app/src-tauri/tauri.conf.json app/package.json app/package-lock.json
git commit -q -m "chore: release v$v"
if [ -n "$2" ]; then
    git tag -a "v$v" -F "$2"
else
    git tag -a "v$v" -m "v$v"
fi
git push -q origin HEAD "v$v"
echo "v$v pushed; the build runs at https://github.com/ThomasOXY/Stickwise/actions"
