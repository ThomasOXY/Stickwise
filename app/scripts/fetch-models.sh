#!/bin/sh
# Beat This! models (MIT, JKU Linz; ONNX by danigb/beat-this-rs) for the
# analyser, into src-tauri/models/. Downloaded once, checksums verified.
set -e
cd "$(dirname "$0")/.."
dir=src-tauri/models
mkdir -p "$dir"

fetch() { # name url sha256
    if [ -f "$dir/$1" ] && echo "$3  $dir/$1" | shasum -a 256 -c - >/dev/null 2>&1; then
        return 0
    fi
    echo "downloading $1"
    curl -fL --retry 3 -o "$dir/$1.part" "$2"
    echo "$3  $dir/$1.part" | shasum -a 256 -c - >/dev/null || { echo "checksum mismatch: $1" >&2; rm -f "$dir/$1.part"; exit 1; }
    mv "$dir/$1.part" "$dir/$1"
}

fetch mel_spectrogram.onnx \
    https://raw.githubusercontent.com/danigb/beat-this-rs/9ad271c3e1017cfc4d9b6b43bb691c2afd545383/models/mel_spectrogram.onnx \
    fdd59e65c515331308e4c8841edf99972deca646bdf6197744c2a5b7755e3de9
fetch beat_this.onnx \
    https://github.com/danigb/beat-this-rs/releases/download/model-large/beat_this.onnx \
    5f810debe53459b559127fb55bbad40035bb47cc567b20e501670f968c770f02
