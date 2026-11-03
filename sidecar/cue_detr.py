"""CUE-DETR cue point estimation for Stickwise.

Inference adapted from ETH DISCO's cue-detr (MIT License, Copyright (c) 2024
ETH DISCO, https://github.com/ETH-DISCO/cue-detr): the track becomes a mel
spectrogram rendered as a viridis image, sliding windows of it go through a
DETR object detector fine-tuned on 21k cue points set by professional DJs,
and box centres become cue candidates.

Usage: python cue_detr.py FILE...   -> one JSON object per line on stdout:
  {"path": ..., "duration": s, "candidates": [{"time": s, "score": 0..1}, ...]}
  or {"path": ..., "error": "..."}
Progress and logs go to stderr.
"""

import json
import os
import subprocess
import sys
import tempfile

import librosa
import numpy as np
import torch
from matplotlib import cm
from PIL import Image
from scipy.signal import find_peaks
from transformers import DetrForObjectDetection, DetrImageProcessor

OVERLAP = 0.75
W_WIN = 355
PADDING = 266
SR = 22050
CHECKPOINT = os.environ.get("RBX_CUE_DETR", "disco-eth/cue-detr")


def log(*a):
    print(*a, file=sys.stderr, flush=True)


def device():
    if torch.backends.mps.is_available():
        return "mps"
    return "cuda" if torch.cuda.is_available() else "cpu"


def spectrogram_image(y):
    m = librosa.feature.melspectrogram(y=y, sr=SR, n_fft=2048)
    m_db = librosa.power_to_db(m, ref=np.max)
    arr = m_db[::-1]
    sm = cm.ScalarMappable(cmap="viridis")
    sm.set_clim(arr.min(), arr.max())
    rgba = np.require(sm.to_rgba(arr, bytes=True), requirements="C")
    im = Image.frombuffer("RGBA", (rgba.shape[1], rgba.shape[0]), rgba, "raw", "RGBA", 0, 1)
    return np.array(im)[:, :, :3]


def windows(image):
    width = image.shape[1] + PADDING
    n = int(np.floor(width / (W_WIN * (1 - OVERLAP))))
    images, borders = [], []
    for i in range(n):
        left = int(np.floor(i * W_WIN * (1 - OVERLAP))) - PADDING
        right = left + W_WIN
        borders.append(left)
        if left < 0:
            seg = np.pad(image[:, :right], ((0, 0), (-left, 0), (0, 0)), mode="linear_ramp")
        elif right > image.shape[1]:
            seg = image[:, left:]
            seg = np.pad(seg, ((0, 0), (0, right - left - seg.shape[1]), (0, 0)), mode="linear_ramp")
        else:
            seg = image[:, left:right]
        images.append(seg)
    return images, borders


def load_audio(path):
    try:
        y, _ = librosa.load(path, sr=SR, mono=True)
        return y
    except Exception:
        # libsndfile rejects some MP3s; macOS CoreAudio decodes them.
        with tempfile.TemporaryDirectory() as d:
            wav = os.path.join(d, "decoded.wav")
            subprocess.run(
                ["/usr/bin/afconvert", "-f", "WAVE", "-d", f"LEI16@{SR}", "-c", "1", path, wav],
                check=True,
                capture_output=True,
            )
            y, _ = librosa.load(wav, sr=SR, mono=True)
            return y


def predict(model, processor, dev, path, batch=32):
    y = load_audio(path)
    duration = len(y) / SR
    images, borders = windows(spectrogram_image(y))
    positions, scores = [], []
    for start in range(0, len(images), batch):
        enc = processor.preprocess(images[start : start + batch], do_resize=False, return_tensors="pt")
        pixel_values = enc["pixel_values"].to(dev)
        with torch.no_grad():
            out = model(pixel_values)
        preds = processor.post_process_object_detection(out, 0, [(128, W_WIN)] * pixel_values.shape[0])
        for p, left in zip(preds, borders[start : start + batch]):
            scores.extend(p["scores"].cpu().tolist())
            pos = (p["boxes"][:, 0] + p["boxes"][:, 2]) // 2 + left
            positions.extend(pos.cpu().long().tolist())
    if not scores:
        return duration, []
    s = np.array(scores)
    s = (s - s.min()) / max(1e-9, s.max() - s.min())
    order = np.argsort(positions, kind="stable")
    positions = np.array(positions)[order]
    s = s[order]
    # Same peak picking as the reference script (distance 16 in prediction
    # order), but every peak is returned so the app can choose sensitivity.
    peaks, props = find_peaks(s, height=0.0, distance=16)
    times = librosa.frames_to_time(positions[peaks], sr=SR)
    cands = [
        {"time": round(float(t), 3), "score": round(float(h), 4)}
        for t, h in zip(times, props["peak_heights"])
        if 0 <= t <= duration
    ]
    return duration, cands


def main():
    paths = sys.argv[1:]
    if not paths:
        log(__doc__)
        return 2
    dev = device()
    log(f"loading {CHECKPOINT} on {dev}")
    processor = DetrImageProcessor(do_resize=False, do_pad=False)
    model = DetrForObjectDetection.from_pretrained(CHECKPOINT, use_pretrained_backbone=False)
    model.to(dev).eval()
    for i, path in enumerate(paths):
        log(f"[{i + 1}/{len(paths)}] {os.path.basename(path)}")
        try:
            duration, cands = predict(model, processor, dev, path)
            print(json.dumps({"path": path, "duration": round(duration, 3), "candidates": cands}), flush=True)
        except Exception as e:  # one bad file must not stop the batch
            print(json.dumps({"path": path, "error": f"{type(e).__name__}: {e}"}), flush=True)
    return 0


if __name__ == "__main__":
    sys.exit(main())
