# RB-EXPORT

Vlastní exporter / správce knihovny pro Pioneer CDJ/XDJ (a Denon) — náhrada rekordbox exportu, která nerozbije flashku.
Plán: [PLAN.md](PLAN.md).

## Build
```sh
cargo build --release
cargo test --release
# volitelně test nad vlastní knihovnou analýz (jen čtení):
RBX_ANLZ_CORPUS="/Volumes/Extended/PIONEER/Master/share/PIONEER" cargo test --release
```

## CLI
```sh
rbx usb /Volumes/DJ                       # playlisty a tracky na flashce
rbx extract /Volumes/DJ ~/Music/Z-flashky --numbered        # playlisty do složek
rbx extract /Volumes/DJ ~/Music/X -p "Klub X" --dry-run     # jen ukázat co se stane
rbx anlz /Volumes/DJ/PIONEER/USBANLZ/P016/0000875E/ANLZ0000.EXT   # cue, grid, waveformy
rbx pdb /Volumes/DJ                       # technický výpis export.pdb
```
Všechny příkazy flashku jen **čtou**.
