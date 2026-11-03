# Stickwise

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

## Sync mezi Macy
Stickwise na dvou Macích ve stejné síti se najdou samy (Bonjour) a po spárování kódem si posílají
knihovnu přes šifrované spojení: **Poslat** vybrané sety a playlisty, nebo **Zrcadlit** celou knihovnu
oběma směry s náhledem změn. Hudba, kterou Mac už má, se nepřepisuje; u cue, gridů a tagů vyhrává
novější úprava; smazání playlistu se propíše jen po potvrzení. Podrobnosti v [PLAN.md](PLAN.md) (5b).

## Vydání a aktualizace
```sh
scripts/release.sh 0.2.0              # poznámky z feat/fix commitů od minulé verze
scripts/release.sh 0.2.0 notes.md     # poznámky z vlastního souboru (Markdown)
```
Skript zvedne verzi, commitne, pushne tag `v0.2.0` a GitHub Actions postaví DMG a release.
Appka si `latest.json` z posledního releasu kontroluje při startu a každé 4 h, nabídne
novou verzi i s poznámkami a po stažení se restartuje. Update je podepsaný klíčem
`~/.tauri/rb-export.key` (v repu secret `TAURI_SIGNING_PRIVATE_KEY`); bez něj appka
update odmítne, takže klíč neztrať.

Appka není podepsaná Apple certifikátem: při **první** instalaci z DMG ji macOS zablokuje,
povol ji v Nastavení → Soukromí a zabezpečení → „Přesto otevřít“
(nebo `xattr -cr /Applications/Stickwise.app`). Další aktualizace už jdou bez dotazu.
