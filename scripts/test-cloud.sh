#!/bin/bash
# End-to-end test of cloud/ against a throwaway MySQL container and PHP's own
# web server. Touches nothing outside a temp folder. Needs php with pdo_mysql,
# curl, a running Docker, and on the last step cargo and afconvert.
#
#   scripts/test-cloud.sh
set -u
cd "$(dirname "$0")/.."

ROOT=$(mktemp -d)
MYSQL_PORT=33971
WEB_PORT=8971
BASE="http://127.0.0.1:$WEB_PORT"
DB=stickwise-cloud-test-$$
SECRET=0123456789abcdef0123456789abcdef0123
fails=0

cleanup() {
    [ -n "${WEB_PID:-}" ] && kill "$WEB_PID" 2>/dev/null
    docker rm -f "$DB" >/dev/null 2>&1
    rm -rf "$ROOT"
}
trap cleanup EXIT

is() { # name, expected, actual
    if [ "$2" = "$3" ]; then
        echo "ok    $1"
    else
        echo "FAIL  $1: expected [$2], got [$3]"
        fails=$((fails + 1))
    fi
}

docker run -d --rm --name "$DB" -p 127.0.0.1:$MYSQL_PORT:3306 -e MYSQL_ROOT_PASSWORD=root \
    -e MYSQL_DATABASE=stickwise -e MYSQL_USER=sw -e MYSQL_PASSWORD=pw "${MYSQL_IMAGE:-mysql:8.4}" >/dev/null || exit 1
sql() { docker exec -i -e MYSQL_PWD=root "$DB" mysql -uroot --default-character-set=utf8mb4 -N "$@" 2>/dev/null; }
# The image restarts once while setting up: wait for the TCP port, which comes last.
for _ in $(seq 120); do
    docker exec -e MYSQL_PWD=pw "$DB" mysql -usw -h127.0.0.1 -e 'SELECT 1' stickwise >/dev/null 2>&1 && break
    sleep 1
done
docker exec -e MYSQL_PWD=pw "$DB" mysql -usw -h127.0.0.1 -e 'SELECT 1' stickwise >/dev/null 2>&1 || { echo "MySQL did not start"; docker logs --tail 20 "$DB"; exit 1; }
sql stickwise < cloud/schema.sql

WEB="$ROOT/web"
cp -R cloud "$WEB"
cat > "$WEB/config.php" <<EOF
<?php
return ['db_host' => '127.0.0.1;port=$MYSQL_PORT', 'db_name' => 'stickwise', 'db_user' => 'sw', 'db_pass' => 'pw',
        'secret' => '$SECRET', 'storage' => __DIR__ . '/storage'];
EOF
php -d post_max_size=64M -d memory_limit=16M -S 127.0.0.1:$WEB_PORT -t "$WEB" >"$ROOT/php.log" 2>&1 &
WEB_PID=$!
for _ in $(seq 40); do curl -s "$BASE/api.php?action=ping" >/dev/null && break; sleep 0.25; done

code() { curl -s -o /dev/null -w '%{http_code}' "$@"; }
field() { php -r '$d = json_decode(stream_get_contents(STDIN), true); $v = $d; foreach (array_slice($argv, 1) as $k) { $v = $v[$k] ?? null; } echo is_bool($v) ? ($v ? "true" : "false") : (string) $v;' -- "$@"; }
api() { # action, json
    curl -s -X POST -H "X-Stickwise-Token: ${TOKEN:-}" --data-binary "$2" "$BASE/api.php?action=$1"
}

echo "--- install"
is "ping names the app" stickwise-cloud "$(curl -s "$BASE/api.php?action=ping" | field app)"
is "ping reads post_max_size" 67108864 "$(curl -s "$BASE/api.php?action=ping" | field maxUpload)"
is "install refuses a wrong key" 0 "$(curl -s -d 'secret=nope&username=dj&password=longenoughpw&password2=longenoughpw' "$BASE/install.php" >/dev/null; sql stickwise -e 'SELECT COUNT(*) FROM users')"
curl -s -d "secret=$SECRET&username=dj&password=longenoughpw&password2=longenoughpw" "$BASE/install.php" >/dev/null
is "install makes the account" 1 "$(sql stickwise -e 'SELECT COUNT(*) FROM users')"
curl -s -d "secret=$SECRET&username=second&password=longenoughpw&password2=longenoughpw" "$BASE/install.php" >/dev/null
is "install never makes a second one" 1 "$(sql stickwise -e 'SELECT COUNT(*) FROM users')"

echo "--- sign-in"
is "upload without a token is refused" 401 "$(code -X POST --data-binary x "$BASE/api.php?action=upload&key=$(printf 'a%.0s' {1..64})&format=m4a&sha256=$(printf 'a%.0s' {1..64})")"
is "wrong password is refused" bad_login "$(api login '{"username":"dj","password":"wrong"}' | field error)"
TOKEN=$(api login '{"username":"dj","password":"longenoughpw","label":"test"}' | field token)
is "token has 64 characters" 64 "${#TOKEN}"
is "token is stored hashed" 0 "$(sql stickwise -e "SELECT COUNT(*) FROM api_tokens WHERE token_hash = '$TOKEN'")"

echo "--- upload"
A="$ROOT/a.m4a"
B="$ROOT/b.mp3"
head -c 40000000 /dev/urandom > "$A"
head -c 100000 /dev/urandom > "$B"
SHA_A=$(shasum -a 256 "$A" | cut -d' ' -f1)
SHA_B=$(shasum -a 256 "$B" | cut -d' ' -f1)
KEY_A=$(printf 'a%.0s' {1..64})
KEY_B=$(printf 'b%.0s' {1..64})
# Request bodies in variables: bash 3.2 misreads \" inside "$(...)".
KEYS="{\"keys\":[\"$KEY_A\",\"$KEY_B\"]}"
is "both tracks are missing" 2 "$(api missing "$KEYS" | php -r 'echo count(json_decode(stream_get_contents(STDIN), true)["missing"]);')"
upload() { # key, sha, format, file
    curl -s -o /dev/null -w '%{http_code}' -X POST -H "X-Stickwise-Token: $TOKEN" -H 'Content-Type: application/octet-stream' \
        --data-binary "@$4" "$BASE/api.php?action=upload&key=$1&sha256=$2&format=$3"
}
is "damaged upload is refused" 422 "$(upload "$KEY_A" "$SHA_B" m4a "$A")"
is "damaged upload leaves no file" 0 "$(find "$WEB/storage" -name '*.part' -o -name '*.m4a' | wc -l | tr -d ' ')"
is "unknown format is refused" 400 "$(upload "$KEY_A" "$SHA_A" php "$A")"
is "40 MB upload with memory_limit 16M" 200 "$(upload "$KEY_A" "$SHA_A" m4a "$A")"
is "second upload" 200 "$(upload "$KEY_B" "$SHA_B" mp3 "$B")"
is "nothing is missing now" 0 "$(api missing "$KEYS" | php -r 'echo count(json_decode(stream_get_contents(STDIN), true)["missing"]);')"

echo "--- publish"
UNKNOWN="{\"ref\":\"7\",\"title\":\"X\",\"tracks\":[{\"key\":\"$(printf 'c%.0s' {1..64})\"}]}"
is "publish fails on an unknown track" track_missing "$(api playlist_save "$UNKNOWN" | field error)"
TRACKS="[{\"key\":\"$KEY_A\",\"title\":\"<b>One</b>\",\"artist\":\"Příliš žluťoučký\",\"duration\":185},{\"key\":\"$KEY_B\",\"title\":\"Two\",\"artist\":\"B\",\"duration\":200}]"
SLUG=$(api playlist_save "{\"ref\":\"7\",\"title\":\"Klub & Co\",\"password\":\"tajne\",\"tracks\":$TRACKS}" | field playlist slug)
is "slug has 32 characters" 32 "${#SLUG}"
is "playlist_get finds it" "$SLUG" "$(api playlist_get '{"ref":"7"}' | field playlist slug)"
is "playlist_get says password" true "$(api playlist_get '{"ref":"7"}' | field playlist hasPassword)"
AGAIN="{\"ref\":\"7\",\"title\":\"Klub & Co\",\"tracks\":$TRACKS}"
is "sharing again keeps the link" "$SLUG" "$(api playlist_save "$AGAIN" | field playlist slug)"
is "sharing again keeps the password" true "$(api playlist_get '{"ref":"7"}' | field playlist hasPassword)"

echo "--- listening"
TID=$(sql stickwise -e "SELECT id FROM tracks WHERE track_key = '$KEY_A'")
JAR="$ROOT/cookies"
PAGE="$BASE/index.php?p=$SLUG"
STREAM="$BASE/stream.php?p=$SLUG&t=$TID"
is "a made-up link is a 404" 404 "$(code "$BASE/index.php?p=$(printf '0%.0s' {1..32})")"
is "no link is a 404" 404 "$(code "$BASE/")"
is "locked page hides the tracks" 0 "$(curl -s "$PAGE" | grep -c 'data-src')"
is "locked page shows the form" 1 "$(curl -s "$PAGE" | grep -c 'name="password"')"
is "locked audio is refused" 403 "$(code "$STREAM")"
is "wrong password stays locked" 401 "$(code -d password=nope "$PAGE")"
is "right password redirects" 303 "$(code -c "$JAR" -d password=tajne "$PAGE")"
is "unlocked page lists the tracks" 2 "$(curl -s -b "$JAR" "$PAGE" | grep -c 'data-src')"
is "titles are escaped" 2 "$(curl -s -b "$JAR" "$PAGE" | grep -c '&lt;b&gt;One&lt;/b&gt;')"
is "a forged cookie is refused" 403 "$(code -b "sw_${SLUG:0:12}=9999999999.deadbeef" "$STREAM")"
is "whole file" 200 "$(code -b "$JAR" "$STREAM")"
is "whole file size" 40000000 "$(curl -s -b "$JAR" "$STREAM" | wc -c | tr -d ' ')"
is "whole file bytes" "$SHA_A" "$(curl -s -b "$JAR" "$STREAM" | shasum -a 256 | cut -d' ' -f1)"
is "range is a 206" 206 "$(code -b "$JAR" -H 'Range: bytes=0-1' "$STREAM")"
is "range length" 2 "$(curl -s -b "$JAR" -H 'Range: bytes=0-1' "$STREAM" | wc -c | tr -d ' ')"
is "range header" "bytes 100-199/40000000" "$(curl -s -D - -o /dev/null -b "$JAR" -H 'Range: bytes=100-199' "$STREAM" | tr -d '\r' | sed -n 's/^Content-Range: //p')"
is "open-ended range" "bytes 39999990-39999999/40000000" "$(curl -s -D - -o /dev/null -b "$JAR" -H 'Range: bytes=39999990-' "$STREAM" | tr -d '\r' | sed -n 's/^Content-Range: //p')"
is "suffix range" "bytes 39999995-39999999/40000000" "$(curl -s -D - -o /dev/null -b "$JAR" -H 'Range: bytes=-5' "$STREAM" | tr -d '\r' | sed -n 's/^Content-Range: //p')"
is "range past the end" 416 "$(code -b "$JAR" -H 'Range: bytes=40000000-' "$STREAM")"
is "a track of no playlist is refused" 404 "$(code -b "$JAR" "$BASE/stream.php?p=$SLUG&t=999999")"

echo "--- password changes"
api playlist_password '{"ref":"7","password":"nove"}' >/dev/null
is "a new password ends old cookies" 403 "$(code -b "$JAR" "$STREAM")"
api playlist_password '{"ref":"7","password":null}' >/dev/null
is "no password opens the audio" 200 "$(code -H 'Range: bytes=0-1' -o /dev/null "$STREAM" | sed 's/206/200/')"
is "no password opens the page" 2 "$(curl -s "$PAGE" | grep -c 'data-src')"

echo "--- guessing"
api playlist_password '{"ref":"7","password":"nove"}' >/dev/null
for _ in $(seq 10); do code -d password=nope "$PAGE" >/dev/null; done
is "too many tries are stopped" 429 "$(code -d password=nove "$PAGE")"
sql stickwise -e 'DELETE FROM failed_attempts'

echo "--- removing"
A_FILE=$(sql stickwise -e "SELECT file_name FROM tracks WHERE track_key = '$KEY_A'")
B_FILE=$(sql stickwise -e "SELECT file_name FROM tracks WHERE track_key = '$KEY_B'")
api playlist_save "{\"ref\":\"8\",\"title\":\"Other\",\"tracks\":[{\"key\":\"$KEY_B\",\"title\":\"Two\"}]}" >/dev/null
api playlist_save "{\"ref\":\"7\",\"title\":\"Shorter\",\"tracks\":[{\"key\":\"$KEY_B\",\"title\":\"Two\"}]}" >/dev/null
is "a dropped track's file goes" no "$([ -f "$WEB/storage/$A_FILE" ] && echo yes || echo no)"
is "a stale row is gone too" 0 "$(sql stickwise -e "SELECT COUNT(*) FROM tracks WHERE track_key = '$KEY_A'")"
api playlist_delete '{"ref":"7"}' >/dev/null
is "unshared link is a 404" 404 "$(code "$PAGE")"
is "a track another playlist uses stays" yes "$([ -f "$WEB/storage/$B_FILE" ] && echo yes || echo no)"
api playlist_delete '{"ref":"8"}' >/dev/null
is "the last playlist takes the file along" no "$([ -f "$WEB/storage/$B_FILE" ] && echo yes || echo no)"
api logout '{}' >/dev/null
is "logout ends the token" 401 "$(code -X POST -H "X-Stickwise-Token: $TOKEN" -d '{"ref":"7"}' "$BASE/api.php?action=playlist_get")"

echo "--- the app's side"
if CLOUD_TEST_URL="$BASE" CLOUD_TEST_USER=dj CLOUD_TEST_PASSWORD=longenoughpw \
    cargo test -q -p rbx-app cloud::tests::shares_against_a_real_server -- --ignored >"$ROOT/cargo.log" 2>&1; then
    echo "ok    share from Rust: convert, upload, publish, share again"
else
    echo "FAIL  share from Rust"
    tail -30 "$ROOT/cargo.log"
    fails=$((fails + 1))
fi
M4A=$(find "$WEB/storage" -name '*.m4a' | head -1)
if [ -n "$M4A" ] && command -v afinfo >/dev/null; then
    INFO=$(afinfo "$M4A")
    is "converted to AAC" 1 "$(echo "$INFO" | grep -c 'Data format: *2 ch, *44100 Hz, aac')"
    RATE=$(echo "$INFO" | sed -n 's/^bit rate: \([0-9]*\).*/\1/p')
    is "at about 192 kb/s" yes "$([ "${RATE:-0}" -gt 170000 ] && [ "${RATE:-0}" -lt 215000 ] && echo yes || echo "no: $RATE")"
fi

echo
if [ "$fails" -gt 0 ]; then
    echo "$fails failed"
    grep -E 'stickwise|PHP (Fatal|Warning)' "$ROOT/php.log" | tail -20
    exit 1
fi
echo "all passed"
