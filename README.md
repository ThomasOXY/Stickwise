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

## Sdílení playlistu k poslechu
Pravý klik na playlist → **Sdílet k poslechu…** převede skladby na AAC 192 kb/s, nahraje je na tvůj
PHP hosting a dá ti odkaz, případně s heslem. Serverová část je ve složce [cloud/](cloud/README.md)
a každý release ji má jako `Stickwise-Cloud.zip`. Test: `scripts/test-cloud.sh` (potřebuje Docker).

## Vydání a aktualizace
```sh
scripts/release.sh 0.3.2              # poznámky z feat/fix commitů od minulé verze
scripts/release.sh 0.3.2 notes.md     # poznámky z vlastního souboru (Markdown)
scripts/release.sh --ci 0.3.2         # postavit v GitHub Actions místo na Macu
```
Skript zvedne verzi, postaví a podepíše appku na tomhle Macu, zkontroluje podpis,
commitne, pushne tag `v0.3.2` a vydá GitHub release s DMG, updatem a `latest.json`.
Když build selže, nezůstane po něm commit ani tag.
Appka si `latest.json` z posledního releasu kontroluje při startu a každé 4 h, nabídne
novou verzi i s poznámkami a po stažení se restartuje.

Klíče v `~/.tauri/` (zálohuj, bez nich nejde vydat update):
- `rb-export.key`: podpis updatu (v repu secret `TAURI_SIGNING_PRIVATE_KEY`), bez něj appka
  update odmítne.
- `stickwise-signing.p12` (heslo v `.p12.pass`): vlastní certifikát, kterým je appka
  podepsaná. Díky němu ji macOS po updatu pozná a nechá jí povolený přístup k disku, flashkám
  a síti. Pro build na Macu musí být jednou v keychain:
  `security import ~/.tauri/stickwise-signing.p12 -k ~/Library/Keychains/login.keychain-db -P "$(cat ~/.tauri/stickwise-signing.p12.pass)" -T /usr/bin/codesign`.
  V CI ho dávají secrets `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD` a `APPLE_SIGNING_IDENTITY`.

Appka není podepsaná Apple certifikátem: při **první** instalaci z DMG ji macOS zablokuje,
povol ji v Nastavení → Soukromí a zabezpečení → „Přesto otevřít“
(nebo `xattr -cr /Applications/Stickwise.app`). Další aktualizace už jdou bez dotazu.
