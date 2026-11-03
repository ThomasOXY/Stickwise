#!/bin/sh
# Release a new version: bump it everywhere, build and sign the app on this
# Mac, then commit, tag, push and publish the GitHub release with the DMG,
# the signed update and latest.json, which running apps poll.
#
#   scripts/release.sh 0.3.2                   notes from the feat/fix/perf commits
#   scripts/release.sh 0.3.2 notes.md          notes from a file (Markdown)
#   scripts/release.sh --ci 0.3.2 [notes.md]   build in GitHub Actions instead
#
# Needs the updater key ~/.tauri/rb-export.key and the code signing
# certificate in the login keychain (once):
#   security import ~/.tauri/stickwise-signing.p12 -k ~/Library/Keychains/login.keychain-db \
#     -P "$(cat ~/.tauri/stickwise-signing.p12.pass)" -T /usr/bin/codesign
set -e
cd "$(dirname "$0")/.."

REPO=ThomasOXY/Stickwise
IDENTITY="Stickwise Self-Signed Code Signing"
KEY="$HOME/.tauri/rb-export.key"
VERSIONED="Cargo.toml Cargo.lock app/src-tauri/tauri.conf.json app/package.json app/package-lock.json"

die() { echo "$*" >&2; exit 1; }

ci=
[ "$1" = --ci ] && { ci=1; shift; }
v=${1#v}
echo "$v" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || die "usage: $0 [--ci] X.Y.Z [notes.md]"
[ -z "$2" ] || [ -f "$2" ] || die "no such notes file: $2"
[ -z "$(git status --porcelain)" ] || die "commit or stash your changes first"
git rev-parse -q --verify "refs/tags/v$v" >/dev/null && die "tag v$v already exists"
if [ -z "$ci" ]; then
    [ -f "$KEY" ] || die "missing the updater key $KEY"
    security find-certificate -c "$IDENTITY" >/dev/null 2>&1 \
        || die "the code signing certificate is not in the keychain (see the top of $0)"
fi

# Notes: the file, else the feat/fix/perf commits since the last tag. They go
# into the tag too, where the CI build takes them from.
notes=$(mktemp)
trap 'rm -f "$notes"' EXIT
if [ -n "$2" ]; then
    cp "$2" "$notes"
else
    prev=$(git describe --tags --abbrev=0 2>/dev/null || true)
    section() { # title, type regex
        lines=$(git log "${prev:+$prev..}HEAD" --no-merges --pretty='%s' | grep -E "^($2)(\([^)]*\))?!?: " | sed -E 's/^[a-z]+(\([^)]*\))?!?: /- /' || true)
        [ -z "$lines" ] || printf '## %s\n%s\n\n' "$1" "$lines"
    }
    { section "New" "feat"; section "Fixes" "fix"; section "Performance" "perf"; } > "$notes"
    [ -s "$notes" ] || echo "Minor changes and improvements." > "$notes"
fi

# The workspace version is the app's (app/src-tauri uses version.workspace).
sed -i '' -E "/^\[workspace.package\]/,/^\[/ s/^version = \".*\"/version = \"$v\"/" Cargo.toml
sed -i '' -E "s/^  \"version\": \".*\",/  \"version\": \"$v\",/" app/src-tauri/tauri.conf.json
(cd app && npm version "$v" --no-git-tag-version --allow-same-version >/dev/null)
cargo update --workspace --offline -q

if [ -z "$ci" ]; then
    # A failed build leaves nothing behind: no commit, no tag, versions back.
    trap 'rm -f "$notes"; [ -n "$committed" ] || git checkout -- $VERSIONED' EXIT
    echo "Building Stickwise $v…"
    (
        cd app
        unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD
        APPLE_SIGNING_IDENTITY="$IDENTITY" \
            TAURI_SIGNING_PRIVATE_KEY="$(cat "$KEY")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
            npx tauri build
    )
    b=target/release/bundle
    app="$b/macos/Stickwise.app"
    codesign --verify --deep --strict "$app"
    # Signed by the certificate, not ad hoc: else macOS forgets the app's permissions.
    codesign -d -r- "$app" 2>&1 | grep -q "certificate root" || die "$app is not signed by the certificate"

    out=target/release/v$v
    rm -rf "$out"
    mkdir -p "$out"
    cp "$b/dmg/Stickwise_${v}_aarch64.dmg" "$out/"
    cp "$b/macos/Stickwise.app.tar.gz" "$out/Stickwise_aarch64.app.tar.gz"
    cp "$b/macos/Stickwise.app.tar.gz.sig" "$out/Stickwise_aarch64.app.tar.gz.sig"
    V="$v" NOTES="$notes" OUT="$out" REPO="$REPO" node -e '
      const fs = require("fs");
      const { V, NOTES, OUT, REPO } = process.env;
      const signature = fs.readFileSync(`${OUT}/Stickwise_aarch64.app.tar.gz.sig`, "utf8").trim();
      const url = `https://github.com/${REPO}/releases/download/v${V}/Stickwise_aarch64.app.tar.gz`;
      const json = {
        version: V,
        notes: `Stickwise ${V}\n\n${fs.readFileSync(NOTES, "utf8").trim()}`,
        pub_date: new Date().toISOString(),
        platforms: { "darwin-aarch64": { signature, url }, "darwin-aarch64-app": { signature, url } },
      };
      fs.writeFileSync(`${OUT}/latest.json`, JSON.stringify(json, null, 2));
    '
fi

git add $VERSIONED
git commit -q -m "chore: release v$v"
git tag -a "v$v" -F "$notes"
committed=1
git push -q origin HEAD "v$v"

if [ -n "$ci" ]; then
    gh workflow run release.yml -R "$REPO" -f tag="v$v"
    echo "v$v pushed; the build runs at https://github.com/$REPO/actions"
    exit
fi

gh release create "v$v" -R "$REPO" --title "Stickwise v$v" --notes-file "$notes" --verify-tag \
    "$out/Stickwise_${v}_aarch64.dmg" "$out/Stickwise_aarch64.app.tar.gz" \
    "$out/Stickwise_aarch64.app.tar.gz.sig" "$out/latest.json"
echo "v$v released: https://github.com/$REPO/releases/tag/v$v"
