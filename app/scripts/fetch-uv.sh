#!/bin/sh
# Put the `uv` the app ships (it installs the Python tools on first use)
# where Tauri's externalBin expects it: src-tauri/binaries/uv-<target>.
set -e
cd "$(dirname "$0")/.."
target=$(rustc -vV | sed -n 's/^host: //p')
dest="src-tauri/binaries/uv-$target"
[ -f "$dest" ] && exit 0
uv=$(command -v uv || echo "$HOME/.local/bin/uv")
[ -x "$uv" ] || { echo "uv not found: install it (https://docs.astral.sh/uv/)" >&2; exit 1; }
mkdir -p src-tauri/binaries
cp "$uv" "$dest"
echo "copied $uv -> $dest"
