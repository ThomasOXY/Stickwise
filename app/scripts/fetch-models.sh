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
fetch beat_this_small.onnx \
    https://raw.githubusercontent.com/danigb/beat-this-rs/e82b5de28962663be047adca84781edae566504f/models/beat_this_small.onnx \
    a5f8d39d989f31859454ba27afe61c5317ca95e4d9373e6853e5361b8937172f
