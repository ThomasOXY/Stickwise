# RB-EXPORT analysis sidecar

Local Python tools the app runs for analysis it cannot do itself yet.

- `cue_detr.py` — cue point estimation with CUE-DETR (ETH DISCO, MIT,
  https://github.com/ETH-DISCO/cue-detr; model `disco-eth/cue-detr` from
  Hugging Face, downloaded on first use into the app's data folder).

Setup (once):

```sh
cd sidecar && uv sync
```

Runs on Apple Silicon via MPS, about 5 s per track after the first model
download.
