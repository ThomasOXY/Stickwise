# RB-EXPORT — vlastní exporter, analyzátor a správce knihovny pro Pioneer CDJ/XDJ

> Cíl: spolehlivý export na USB pro CDJ/XDJ (hot cue, memory cue, loopy, beatgrid, waveformy, playlisty),
> vlastní analýza, správa playlistů, náhled skladby a waveformu.
> **Priorita č. 1: EXPORT NA FLASHKU, který nikdy nerozbije flashku.**

Datum plánu: 2026-10-13

---

## Stav (průběžně)
**2026-10-13**
- ✅ Rust workspace, `crates/pioneer-formats`, `crates/usb-tools`, CLI `rbx`.
- ✅ `export.pdb`: čtení (tolerantní k poškození — chyby se hlásí, nečtou se dál) + zápis ve stylu čerstvého exportu rekordbox 7.
  Ověřeno: roundtrip exportů z rekordboxu, řádky tracků bajt po bajtu shodné s rekordboxem, výstup čte nezávislý parser rekordcrate (600 tracků, UTF-16 názvy).
- ✅ ANLZ (`.DAT/.EXT/.2EX`): čtení + zápis, typové cue (PCOB/PCO2), beatgrid, cesta, waveformy.
  Ověřeno na **13 297 skutečných analýzách** z knihovny na disku `Extended` — 100 % bajt po bajtu.
  (`.3EX` = MessagePack „embedding“ pro doporučování v rekordboxu 7, na USB se nepoužívá.)
- ✅ `rbx usb` (výpis playlistů flashky), `rbx pdb`, `rbx anlz` (výpis cue/gridu/waveformů).
- ✅ `rbx extract` — export songů z flashky do složek podle playlistů (+ m3u8, APFS klony duplicit, bezpečné názvy, obnovitelné).
- ⏳ Chybí ověření cue bodů na reálných datech (lokální analýzy rekordboxu cue neobsahují — jsou v master.db) → potřeba testovací flashka.
- ⏭ Další: Fáze 2 — import rekordbox XML → kompletní export na flashku (Contents + PDB + ANLZ + bezpečný pipeline).

**2026-10-14**
- ✅ GUI (Tauri 2 + Svelte 5) podle návrhu z Pencilu; rekordbox-style rolující waveform se zoomem; editor cue (pady A–H, memory, smyčky, quantize, undo).
- ✅ Smart Cue (energie z 3pásmového waveformu) + **CUE-DETR** (sidecar, MPS): pady pokryjí 57 % DJových vlastních cue (bez AI 37 %).
- ✅ Bezpečný zápis cue na flashku (záloha, zápis vedle, F_FULLFSYNC, rename, read-back).
- ✅ **Knihovna v Macu** (SQLite) s importem z rekordboxu (pyrekordbox nad kopií master.db): 4 432 skladeb, 117 playlistů, 331 cue za 0,1 s; sety, playlisty, složky, drag & drop.
- ✅ **Synchronizace na flashku**: záloha, kopie jen chybějícího (rekordbox soubory na flashce se znovu použijí), ANLZ s cestou na flashku a cue z knihovny, odsunutí starého exportExt/OneLibrary, `export.pdb` vyměněn atomicky jako poslední. Ověřeno na skutečných datech do zkušební složky (23 skladeb, 112 hot cue).
- ⏳ Zatím neověřeno na skutečné flashce v CDJ/Denonu.
- ⏭ Další: **OneLibrary zápis** (bez něj CDJ-3000X / XDJ-AZ / OPUS-QUAD po synchronizaci knihovnu neuvidí), vlastní analýza nových skladeb (bez rekordboxu), Set Planner.

## 1. Co už existuje (rešerše GitHubu)

### 1.1 Dokumentace formátů (základ všeho)
| Projekt | Co to je | Proč nás zajímá |
|---|---|---|
| **Deep Symmetry – djl-analysis** (`djl-analysis.deepsymmetry.org`) | Reverse-engineered dokumentace `export.pdb`, ANLZ (`.DAT/.EXT/.2EX`), Pro DJ Link | Hlavní „specifikace“. Číst celou. |
| **Deep Symmetry – crate-digger** (Java) | Parsery + Kaitai Struct definice `rekordbox_pdb.ksy`, `rekordbox_anlz.ksy` | Kaitai definice = strojově čitelná specifikace, dá se z ní generovat validátor |
| **morizkraemer/fourfour → `pioneer-usb-writer/reference-code/PIONEER.md`** | Poznámky k PDB, ANLZ, OneLibrary | Praktické poznatky z psaní (ne jen čtení) |
| **gist 0xdevalias „OneLibrary / Device Library Plus exportLibrary.db“** | Poznámky k šifrování `exportLibrary.db` (SQLCipher, statický sdílený klíč) | Nutné pro OneLibrary export |

### 1.2 Projekty, které už USB export píšou
| Projekt | Jazyk / licence | Stav | Poznámka |
|---|---|---|---|
| **M-Igashi/baken** – příkaz `expressport` | Rust, **MIT** | Stabilní od v4.3.0, testováno na **CDJ-3000 (fw 3.22)** a **CDJ-2000NXS2** | Z `collection.xml` (rekordbox XML) zapíše `export.pdb`, volitelně OneLibrary, ANLZ (kopie z cache rekordboxu nebo vlastní generování), cue + barvy, beatgrid, aktivní loop. **Nejlepší reference – licence MIT, dá se z něj přímo čerpat.** |
| **morizkraemer/fourfour** | Rust (formáty, knihovna, Tauri) + Svelte UI + Python analýza | „Format library proven on CDJ-3000“, UI a analýza ve vývoji | Architektonicky skoro přesně to, co chceme. **Licence neuvedena** → kód nekopírovat, jen se inspirovat. |
| **Radexito/DjManager** | Electron + React + SQLite, MIT | Aktivní (~1000 commitů), hardware testy neuvedeny | Píše `export.pdb`, ANLZ DAT/EXT/2EX, MYSETTING; analýza přes Mixxx analyzer + FFmpeg; má cue editor, náhled, waveformy. Inspirace pro UI/feature set. Issue #553: emulátor CDJ-3000 firmware (`cdj3k-emu`) pro testy bez hardwaru. |
| **kimtore/rex** | Go | Experimentální, autor varuje „nepoužívat na gigu“ | Dobré jako seznam pastí (Denon Prime 4 knihovnu nenačte). |
| **fotoner/DJCrate** | ? | Pracuje na zápisu Device Library | Issue #42 dobře shrnuje, který přehrávač čte jaký formát. |

### 1.3 Čtení / parsování (validace našeho výstupu)
| Projekt | Jazyk / licence | Použití |
|---|---|---|
| **Holzhaus/rekordcrate** | Rust, MPL-2.0 | Parser `export.pdb`, `exportExt.pdb`, ANLZ, SETTING.DAT + CLI `dump`. **Nezávislý validátor našeho výstupu.** |
| **dylanljones/rbox** | Rust (+ Python/Node bindingy), **GPL-3.0** | Čtení i zápis OneLibrary, ANLZ, rekordbox XML, master.db, My-Settings. Pokud ho použijeme jako knihovnu, celá aplikace musí být GPL. |
| **dylanljones/pyrekordbox** | Python | Čtení master.db (rekordbox 6/7), XML, ANLZ, Device Library Plus. Ideální na rychlé prototypy a skripty. |
| **chrisle/alphatheta-connect**, **onelibrary-connect** | TypeScript | Pro DJ Link + čtení `export.pdb`/`exportLibrary.db` |
| **Mixxx** (C++, GPL) | – | Analyzátory (qm-dsp beat/key), import z rekordbox USB. Inspirace pro analýzu. |

**Závěr rešerše:** Nikdo z toho není „hotový rekordbox killer“, ale formáty jsou zmapované a **baken** dokazuje, že zápis `export.pdb` + ANLZ funguje na CDJ-3000 i NXS2. Neprošlapáváme cestu od nuly – stavíme na dokumentaci Deep Symmetry, referenci baken (MIT) a validujeme přes rekordcrate.

---

## 2. Co přesně musí být na flashce

```
/                                  (FAT32, MBR — max. kompatibilita)
├── Contents/<Artist>/<Album>/<soubor>.mp3|.flac|.wav|.aiff|.m4a
├── PIONEER/
│   ├── rekordbox/
│   │   ├── export.pdb             ← Device Library (DeviceSQL) — čte KAŽDÝ CDJ/XDJ
│   │   ├── exportExt.pdb          ← rozšíření (My Tag apod.)
│   │   └── exportLibrary.db       ← OneLibrary (SQLCipher) — CDJ-3000X, XDJ-AZ, OPUS-QUAD, OMNIS-DUO, CDJ-3000 fw ≥ 3.30
│   ├── USBANLZ/Pxxx/xxxxxxxx/
│   │   ├── ANLZ0000.DAT           ← beatgrid, cue (starý formát), preview waveform, VBR index
│   │   ├── ANLZ0000.EXT           ← cue s barvami/komentáři (PCO2), barevné waveformy, phrase (PSSI)
│   │   └── ANLZ0000.2EX           ← 3pásmové waveformy (CDJ-3000)
│   ├── Artwork/00001/a1.jpg, a1_m.jpg
│   ├── MYSETTING.DAT, MYSETTING2.DAT, DEVSETTING.DAT, DJMMYSETTING.DAT
└── .metadata_never_index, .fseventsd/no_log   ← ať macOS nesahá na flashku
```

### 2.1 `export.pdb` (DeviceSQL)
- Stránky po 4096 B, hlavička s ukazateli na tabulky (first/last/empty-candidate page).
- Řádky na stránce: heap od začátku, row index od konce ve skupinách po 16 s bitmapou přítomnosti.
- Stringy: DeviceSQL formát (krátké ASCII / dlouhé ASCII / UTF-16LE).
- Tabulky: `tracks, genres, artists, albums, labels, keys, colors, playlist_tree, playlist_entries, artwork, columns, history_playlists, history_entries, …`
- Track row: id, sample rate, velikost, bitrate, tempo×100, key/artist/album/genre/label id, délka, rating, barva, **cesta k souboru**, **cesta k ANLZ**, title, filename, datum…
- **Hlavní riziko celého projektu:** rozložení stránek, sekvenční čísla, prázdné stránky, přetékání tabulek přes více stránek. Rekordbox to dělá „po svém“ a firmware je na odchylky citlivý (viz rex na Denonu).

### 2.2 ANLZ tagy, které potřebujeme
| Tag | Soubor | Obsah |
|---|---|---|
| `PPTH` | DAT/EXT | cesta k audio souboru |
| `PVBR` | DAT | VBR seek index (důležité pro MP3 VBR!) |
| `PQTZ` | DAT | beatgrid: pro každý beat (číslo v taktu 1–4, tempo×100, čas ms) |
| `PCOB` | DAT/EXT | cue list – starý formát (memory + hot cue, bez barev) |
| `PCO2` | EXT | **rozšířené cue: hot cue A–H, memory cue, loopy, barvy, komentáře** |
| `PWAV`, `PWV2` | DAT | mono preview waveform (400 sloupců) / malý preview |
| `PWV3` | EXT | mono detail waveform (150 vzorků/s) |
| `PWV4`, `PWV5` | EXT | barevný preview / barevný detail (RGB) |
| `PSSI` | EXT | phrase / song structure (XOR maskované) – volitelné, až na konci |
| `PWV6`, `PWV7`, `PWVC` | 2EX | 3pásmové waveformy pro CDJ-3000 |

### 2.3 Kompatibilita — hraju na VŠECH Pioneer + občas Denon
Jedna flashka musí fungovat všude → **vždy zapisovat obojí: `export.pdb` + ANLZ (DAT/EXT/2EX) a zároveň OneLibrary `exportLibrary.db`.**

| Přehrávač | Co čte | Co si pohlídat |
|---|---|---|
| CDJ-2000NXS / 900NXS a starší | `export.pdb`, ANLZ `.DAT` (+ `.EXT` barevné waveformy) | jen `PCOB` cue (bez barev), omezené formáty (FLAC/ALAC ne) → kontrola / volitelný převod do MP3 320 |
| CDJ-2000NXS2, XDJ-1000MK2, XDJ-XZ, XDJ-RX3 | `export.pdb` + `.DAT/.EXT` | `PCO2` barevné hot cue, FLAC/ALAC OK |
| CDJ-3000 | `export.pdb` + `.2EX`; od fw 3.30 i OneLibrary | 3pásmové waveformy, 8 hot cue |
| CDJ-3000X, XDJ-AZ, OPUS-QUAD, OMNIS-DUO | OneLibrary `exportLibrary.db` | bez OneLibrary je flashka na nich nepoužitelná/omezená |
| **Denon Engine OS** (SC6000, Prime 4, SC Live…) | čte `export.pdb` + ANLZ, **převede si ho a zapíše na flashku vlastní `Engine Library/`** | 1) Denon parser je jiný/přísnější — rex tu selhal → Denon povinně v hardwarovém testu. 2) Náš export **nesmí mazat ani poškodit `Engine Library/`**. 3) Po re-exportu ověřit, že Denon pozná změnu a knihovnu přegeneruje (jinak volba „smazat zastaralou Engine Library“). 4) Denon zapisuje na flashku → další důvod pro zálohy a `rbx verify`. |

Exporter má mít **kontrolu kompatibility**: před exportem ukáže, které tracky nepůjdou na kterém přehrávači (formát, sample rate, velikost), s volbou převést.

Volitelně později: **nativní Engine Library** na flashku (knihovna `libdjinterop`, C++, LGPL-3.0 — používá ji Mixxx) → Denon nemusí nic převádět, načte hned a s cue/loopy 1:1.

### 2.4 Cue a loopy – datový model
Každý cue bod: `slot` (0 = memory, 1–8 = hot cue A–H), `type` (point | loop), `start_ms`, `end_ms` (u loopu), `color`, `comment`, `active_loop` flag.
Musí se zapsat **konzistentně** do `PCOB` (DAT i EXT) a `PCO2` (EXT) – starší přehrávače čtou PCOB, novější PCO2.

---

## 3. Architektura

**Stack: Rust jádro + Tauri v2 desktop + Svelte 5 UI. Jen macOS.**
Proč: rekordcrate, rbox, baken i fourfour jsou v Rustu → můžeme číst jejich kód a použít rekordcrate jako validátor; Rust = bezpečná práce s binárními formáty, rychlá analýza; Tauri = lehké moderní UI s WebGL pro waveformy, nativní okno macOS.

```
rb-export/
├── crates/
│   ├── pioneer-formats/   # čtení+zápis PDB, ANLZ, SETTING.DAT, OneLibrary   (žádné závislosti na zbytku)
│   ├── library/           # vlastní SQLite knihovna: tracks, cues, loops, beatgrid, playlisty, analýza
│   ├── analysis/          # dekódování (symphonia), BPM, beatgrid, key, waveform, loudness
│   ├── importers/         # playlisty: m3u/m3u8, rekordbox XML, master.db, Traktor NML, složky, USB flashka
│   ├── export/            # plánovač, staging, zápis, verifikace, hromadný export, profily, sync
│   ├── usb-tools/         # detekce flashek, klonovač, obraz flashky, export z flashky na disk, záchrana
│   ├── jobs/              # fronta úloh: progress, zrušení, obnovení po pádu, log
│   └── playback/          # přehrávání náhledu (cpal + symphonia), sample-přesné
├── cli/                   # `rbx export|clone|extract|verify|dump` — headless, pro testy a skripty
├── app/                   # Tauri v2 + Svelte 5 UI
└── testdata/              # golden exporty z rekordboxu (malé!)
```

**Pravidla:**
- `pioneer-formats` + `export` + `usb-tools` + `cli` musí fungovat samy bez UI. UI je jen nadstavba.
- Každá dlouhá operace (export, klon, extrakce, import, analýza) je **úloha v `jobs`**: běží na pozadí, posílá progress do UI, jde zrušit, po pádu aplikace se dá dokončit nebo bezpečně vrátit.

---

## 3b. Funkce aplikace

### 3b.1 Moderní GUI
- **Styl:** tmavé „pro-audio“ rozhraní (jako moderní DAW / CDJ-3000 displej), jedna výrazná akcentní barva, barvy waveformů jako na CDJ. Nativní pocit z macOS: průsvitný sidebar (vibrancy), semafor v titulku (`titleBarStyle: Overlay`), SF Pro / Inter, plynulé animace.
- **Ovládání:** klávesnice na prvním místě, ⌘K paleta příkazů, drag & drop všude (**playlist přetáhnout na flashku v sidebaru = export**).
- **Rozložení hlavního okna:**
  - vlevo **sidebar**: Knihovna · strom playlistů · **USB flashky** (živě: název, zaplnění, stav ✓/⚠, tlačítko vysunout);
  - uprostřed **tabulka tracků** (virtualizovaná, plynulá i s 50 000 tracky, vlastní sloupce);
  - nahoře **deck**: overview waveform + zoomovatelný detail, transport, **8 barevných cue padů jako na CDJ-3000**;
  - vpravo **inspektor**: info o tracku, seznam cue/loopů, beatgrid;
  - dole **panel úloh**: běžící exporty/klony/analýzy s progress barem.
- **Obrazovky:** Knihovna · Editor tracku (cue/loop/grid) · **USB Manager** (obsah flashky, zdraví, verify, export na disk, klon) · Hromadný export · Průvodce klonováním · Průvodce importem · Zálohy · Nastavení.
- **Technika:** Svelte 5, virtualizovaná tabulka, waveform ve WebGL2 (60 fps playhead), data z Rustu přes Tauri channels.
- **Postup:** nejdřív mockupy hlavních obrazovek a odsouhlasení vzhledu, pak kód.

### 3b.2 Hromadný export playlistů
- Vybrat víc playlistů/složek najednou → exportovat na **jednu nebo víc flashek současně** (zdroj se čte jednou, zápis paralelně na každou flashku).
- **Exportní profily:** uložená sada, např. „Klub X“ = playlisty A, B, C → flashky `DJ-1` + `DJ-2 (záloha)`; jedním klikem.
- **Náhled před exportem:** co přibude / ubude / změní se, velikost, volné místo, varování kompatibility (formáty, 4 GB limit).
- Režimy: **sync** (jen změny) / **čistý export** (flashka přepsána znovu).
- Stejný track v několika playlistech = jen jeden soubor v `Contents/`.

### 3b.3 Import playlistů
- Zdroje: **m3u/m3u8**, **rekordbox XML** (vybrané playlisty), **rekordbox master.db**, **Traktor NML**, **složka** (složka = playlist, podsložky = strom), **existující flashka** (i od rekordboxu — převezme playlisty, cue, loopy, grid a zkopíruje audio do knihovny).
- Párování tracků: cesta → když soubor chybí, hledat podle hashe / artist+title+délka; na konci report chybějících tracků s možností „najít / přelinkovat“.
- Víc souborů najednou, drag & drop do okna.

### 3b.4 Klonovač / migrace flashek
| Režim | Co dělá | Kdy |
|---|---|---|
| **Klon 1:1 (souborový)** — výchozí | Zkopíruje všechno (`Contents/`, `PIONEER/`, `Engine Library/`, ostatní) na cíl, volitelně cíl předtím naformátuje (FAT32/MBR), zkopíruje i název svazku. Kontrola hashů všech souborů. | Záložní flashka, nová flashka. Cíl stačí velký jako **obsazené** místo, ne celá kapacita. |
| **Obraz flashky (blokový)** | Bit po bitu včetně partition table (`/dev/rdiskN`), uložit jako `.img` na disk nebo zapsat na jinou flashku. | Expertní režim, úplná záloha. Cíl ≥ velikost zdroje. Kopíruje i případné poškození! |
| **Migrace s přestavbou** | Přečte knihovnu ze zdroje a na cíl **zapíše novou, čistou** databázi (oprava poškození, doplnění OneLibrary, převod nepodporovaných formátů). | Stará / rozbitá / rekordbox flashka → nová „zdravá“. Potřebuje writer (po Fázi 2b). |

- **Klon 1 → N:** víc cílových flashek najednou.
- **Bezpečnost:**
  - zdroj se připojí **jen pro čtení** (`diskutil mount readOnly`), aby do něj macOS ani nic jiného nezapsal;
  - jako cíl se nabízí **jen externí vyměnitelné disky** (DiskArbitration: ne interní disk, ne systémový svazek);
  - před smazáním cíle potvrzení s **názvem, velikostí a modelem** flashky (překlep = smazaný disk);
  - po klonu porovnat hashe + načíst a ověřit databázi na cíli.

### 3b.5 Export songů z flashky na disk („playlisty do složek“)
- Vybrat flashku → vybrat playlisty (strom, víc najednou, nebo vše, i HISTORY) → cílová složka.
- Výsledek: `<Cíl>/<Složka playlistu>/<Název playlistu>/01 - Artist - Title.mp3` (číslování podle pořadí v playlistu volitelně) + `<Název playlistu>.m3u8`.
- Stejný track ve víc playlistech: kopie / **APFS klon** (`clonefile` — druhá a další kopie nezabírají místo) / symlink.
- Bezpečné názvy pro macOS: nepovolené znaky (`/`, `:`), délka, Unicode normalizace (diakritika NFC/NFD), kolize jmen.
- Volitelně **zachránit i metadata**: vedle uložit rekordbox XML s cue, loopy a gridem → jde naimportovat zpět do aplikace nebo do rekordboxu.
- **Záchranný režim pro poškozenou flashku:** když je `export.pdb` rozbitý, číst stránku po stránce, co jde; zbytek tracků dohledat přes ANLZ (`PPTH` = cesta k souboru) a tagy v souborech. Playlisty se zachrání, pokud přežila jejich tabulka.

---

## 4. Bezpečný export (ochrana proti „corrupted flashka“)

Tohle je hlavní důvod projektu, takže to je v návrhu od prvního dne:

1. **Nikdy nepsat databázi přímo na USB.** Celý `PIONEER/` se sestaví ve **staging adresáři na disku počítače**.
2. **Validace před zápisem:** vygenerovaný `export.pdb` a všechny ANLZ se znovu načtou vlastním parserem **a** rekordcrate; každý track musí mít existující audio soubor a ANLZ.
3. **Záloha:** stávající `PIONEER/` z flashky se zkopíruje do `~/RB-EXPORT/backups/<usb-id>/<datum>/` (malé, jen DB + ANLZ, ne audio).
4. **Audio soubory** (`Contents/`) se kopírují inkrementálně: porovnání velikost + hash; zápis do `soubor.tmp` → `fsync` → rename.
5. **Databáze až nakonec:** `PIONEER.new/` na USB → `fsync` → přejmenovat starý na `PIONEER.old/` → `PIONEER.new/` na `PIONEER/` → smazat `PIONEER.old/`. Při výpadku uprostřed zůstane vždy jedna kompletní verze.
6. **Journal na USB** (`.rbx/journal.json`): co se právě dělá; po přerušení aplikace při dalším připojení dokončí nebo vrátí operaci.
7. **Verifikace po zápisu:** odpojit cache (`F_NOCACHE` / znovu připojit), přečíst z USB a porovnat hashe.
8. **macOS hygiena:** kopírovat bez extended attributes (žádné `._` AppleDouble soubory), po zápisu `dot_clean`, vytvořit `.metadata_never_index` a `.fseventsd/no_log`, vypnout Spotlight na svazku.
9. **Bezpečné vysunutí** přes `diskutil eject` přímo z aplikace; dokud neproběhne, UI ukazuje „NEODPOJOVAT“.
10. **Kontrola filesystému** před exportem: FAT32 (doporučeno), limit 4 GB/soubor, volné místo, `fsck_msdos -n` pro detekci již poškozené flashky.
11. **Lock:** jen jeden export na jednu flashku současně.
12. **Cizí data nechat být:** `Engine Library/` (Denon) a cokoliv mimo `PIONEER/` a `Contents/` se nesmí měnit ani mazat (jen se zazálohuje).

**macOS specifika (aplikace je jen pro macOS):** detekce připojení/odpojení flashky přes DiskArbitration, formátování přes `diskutil eraseDisk FAT32 … MBRFormat`, vysunutí `diskutil eject`, `F_FULLFSYNC` místo obyčejného `fsync`, přehrávání přes Core Audio (cpal).

---

## 5. Fáze vývoje

### Fáze 0 — Příprava a „golden“ data (cca 1 týden)
- [ ] Zjistit hardware, na kterém budeme testovat (modely + verze firmwaru).
- [ ] Vytvořit v rekordboxu **malé testovací exporty** na čistou flashku:
  - 3 tracky (MP3 CBR, MP3 VBR, FLAC/WAV/AIFF), 1 playlist;
  - track se všemi 8 hot cues v různých barvách + memory cues + loopy + aktivní loop + komentáře;
  - 2 úrovně složek playlistů, 100+ tracků (aby tabulky přetekly přes více stránek), dlouhé názvy s diakritikou (UTF-16!);
  - totéž s OneLibrary exportem (rekordbox 7);
  - jednu golden flashku strčit do Denonu a uložit, co tam Denon zapíše (`Engine Library/`).
- [ ] Uložit do `testdata/` (bez audia, nebo s krátkými generovanými tóny).
- [ ] Projít je `rekordcrate dump`, pyrekordbox a Kaitai (crate-digger) — zdokumentovat každé pole, kterému nerozumíme.
- [ ] Prostudovat `baken/expressport` a `fourfour/PIONEER.md`.

**Hotovo když:** máme sadu golden exportů a textový dump každého z nich.

### Fáze 1 — `pioneer-formats`: čtení a zápis formátů (2–3 týdny)
- [ ] ANLZ reader/writer (všechny tagy z 2.2 kromě PSSI) — roundtrip testy na golden datech: `read → write == původní bajty`.
- [ ] PDB reader.
- [ ] PDB writer: stránkování, row groups, DeviceSQL stringy, všechny tabulky.
- [ ] Roundtrip test: `read(golden) → write → byte-diff`. Kde se liší, zdokumentovat proč a zda to vadí.
- [ ] Property-based testy (proptest): náhodné knihovny 1–5000 tracků, dlouhé/unicode stringy → výstup musí projít rekordcrate parserem.
- [ ] SETTING.DAT reader/writer (vč. CRC).

**Hotovo když:** všechny golden soubory projdou roundtripem a rekordcrate čte všechno, co zapíšeme.

> Pořadí uvnitř fáze: **nejdřív readery** (PDB, ANLZ, SETTING) → hned se dají použít ve Fázi 1b, pak writery.

### Fáze 1b — USB nástroje + základ GUI (2 týdny) — rychlé výhry
Potřebuje jen **čtení** formátů, takže to jde hned po readerech. Funguje i s flashkami od rekordboxu, a otestuje parser na tvých skutečných flashkách.
- [ ] `usb-tools`: detekce flashek (DiskArbitration), info o svazku, zdraví (`fsck_msdos -n`).
- [ ] **Export songů z flashky na disk do složek podle playlistů** (3b.5) + m3u8 + APFS klony + volitelně XML s cue.
- [ ] **Klon 1:1 souborový** + **obraz flashky** (3b.4), 1 → N, ověření hashů, zdroj read-only.
- [ ] **Záchranný režim** — vyzkoušet na dnešní rozbité flashce.
- [ ] `jobs`: fronta úloh s progress a zrušením.
- [ ] CLI: `rbx extract`, `rbx clone`, `rbx image`, `rbx verify`.
- [ ] **Tauri kostra aplikace** + mockupy vzhledu + první obrazovka: **USB Manager** (seznam flashek, procházení playlistů na flashce, náhled tracku, Export na disk, Klonovat).

**Hotovo když:** z libovolné rekordbox flashky dostaneš playlisty do složek na disk a uděláš ověřený klon na jinou flashku — z GUI.

### Fáze 2 — Export MVP z rekordbox XML ⭐ NEJDŮLEŽITĚJŠÍ (2 týdny)
Ještě bez vlastní knihovny a analýzy — vstupem je **rekordbox XML** (rekordbox umí exportovat; BPM, beatgrid, cue, loopy, barvy jsou v něm) a ANLZ waveformy z cache rekordboxu.
- [ ] Import rekordbox XML: `TRACK`, `TEMPO` (beatgrid), `POSITION_MARK` (`Num` −1 = memory, 0–7 = hot cue A–H; `Type` 0 = cue, 4 = loop; `Start`/`End`; barvy RGB).
- [ ] Mapování XML → interní model → `export.pdb` + ANLZ.
- [ ] Bezpečný exportní pipeline podle kapitoly 4.
- [ ] CLI: `rbx export --xml collection.xml --playlists "Gig 2026-10" --usb /Volumes/DJ`.
- [ ] **Hromadný export** (3b.2): víc playlistů → víc flashek najednou, exportní profily, náhled změn, režim sync / čistý export.
- [ ] GUI: obrazovka Hromadný export + drag & drop playlistu na flashku.
- [ ] `rbx verify /Volumes/DJ` — kontrola libovolné flashky (i té od rekordboxu).
- [ ] Kontrola kompatibility per přehrávač (formát, sample rate) + volitelný převod do MP3 320 pro starší CDJ.
- [ ] **Hardware test:** flashka → CDJ (NXS2 i 3000) **a Denon**: načtení, browse playlistů, hot cue A–H, memory cue, loop, beat sync, waveform, seek ve VBR MP3.
- [ ] Kontrola i opačným směrem: rekordbox musí flashku otevřít jako zařízení bez chyb.

**Hotovo když:** flashka z našeho nástroje funguje na CDJ i Denonu stejně jako z rekordboxu. **Tímto už máme náhradu rekordbox exportu pro všechny přehrávače kromě OneLibrary-only.**

### Fáze 2b — OneLibrary (povinné, hned po MVP, 1–2 týdny)
Hraješ na všech → bez toho flashka nepojede na CDJ-3000X / XDJ-AZ / OPUS-QUAD / OMNIS-DUO.
- [ ] Golden OneLibrary exporty z rekordboxu 7 → dešifrovat, zdokumentovat schéma.
- [ ] Zápis `exportLibrary.db` (SQLite + SQLCipher, statický klíč; reference baken `--onelibrary`, rbox).
- [ ] Každý export zapisuje `export.pdb` **i** `exportLibrary.db` ze stejných dat ve stejné transakci (staging) → nikdy se nerozjedou.
- [ ] Hardware test na OneLibrary přehrávači.
- [ ] **Migrace s přestavbou** (3b.4): stará / rozbitá / rekordbox flashka → nová čistá flashka s `export.pdb` + OneLibrary.

### Fáze 3 — Vlastní knihovna + import playlistů (2–3 týdny)
- [ ] SQLite schéma: `tracks, files, cues, loops, beatgrids, playlists, playlist_items, folders, analysis, usb_devices, export_profiles, export_history`.
- [ ] **Import playlistů** (3b.3): m3u/m3u8, rekordbox XML, rekordbox `master.db` (přes pyrekordbox / vlastní; klíč viz rekordlocksmith), Traktor NML, složky, **existující flashka**; tagy přes `lofty`.
- [ ] Párování a přelinkování chybějících tracků, report.
- [ ] GUI: Knihovna, strom playlistů, Průvodce importem.
- [ ] Detekce duplicit (hash audia), přesun/přejmenování souborů bez ztráty cue.
- [ ] Undo/historie změn cue a gridu.
- [ ] Export čte z vlastní knihovny místo XML.

### Fáze 4 — Analyzátor (3–4 týdny)
- [ ] Dekódování: `symphonia` (MP3, AAC, FLAC, WAV, AIFF, ALAC). **Pozor na MP3 encoder delay / gapless offset** — musí sedět s tím, jak dekóduje CDJ, jinak se cue posunou o ~26–50 ms. Kalibrovat proti rekordbox gridu na golden datech.
- [ ] BPM + beatgrid: onset envelope → tempogram/autokorelace, předpoklad konstantního tempa (elektronika), fáze gridu, první downbeat. Volitelně dynamický grid.
- [ ] Tónina: chromagram (CQT) + key profily (Krumhansl/Temperley), výstup klasicky i Camelot.
- [ ] Waveformy: 3pásmová filtrace (low/mid/high) → PWAV/PWV2/PWV3/PWV4/PWV5/PWV6/PWV7 v přesném rozlišení (detail 150 vzorků/s, preview 400 sloupců).
- [ ] Loudness: EBU R128 (`ebur128` crate) – auto gain.
- [ ] Benchmark proti rekordboxu na vlastní kolekci: % shody BPM, odchylka gridu v ms, shoda tóniny.
- [ ] Paralelní analýza (rayon), cache výsledků.
- Licence: Mixxx/qm-dsp, aubio, libKeyFinder jsou GPL, Essentia AGPL. Pro osobní použití OK; pokud chceme MIT, napsat vlastní (doporučeno — pro elektronickou hudbu to není extrémně složité).

### Fáze 5 — Plná desktop aplikace (4–6 týdnů)
GUI roste průběžně od Fáze 1b; tady se dotahuje do finální podoby podle 3b.1.
- [ ] Prohlížeč knihovny (virtualizovaná tabulka, rychlé hledání, sloupce jako v rekordboxu), ⌘K paleta.
- [ ] Strom playlistů, drag & drop, smart playlisty (filtry BPM/key/žánr/rating/tag).
- [ ] **Náhled skladby:** přehrávač v Rustu (cpal), sample-přesný, stejný dekodér jako analýza → cue sedí.
- [ ] **Waveform:** overview celé skladby + zoomovatelný detail (WebGL/Canvas), RGB / 3pásmové zobrazení jako na CDJ-3000, beatgrid čáry, značky cue/loopů.
- [ ] **Cue/loop editor:** hot cue A–H, memory cue, loopy (vč. aktivního), barvy, komentáře, quantize na grid, klávesové zkratky.
- [ ] Editor beatgridu: posun, změna BPM, nastavení downbeatu, tap.
- [ ] Panel „USB“: seznam flashek, co se na ně exportuje, diff (co přibude/ubude), tlačítko Export + bezpečné vysunutí.

### Fáze 6 — Nativní Denon Engine Library (volitelné)
- [ ] Přes `libdjinterop` (LGPL) zapisovat i `Engine Library/` → Denon nemusí převádět, cue/loopy 1:1.
- [ ] Jen pokud se ve Fázi 2 ukáže, že převod rekordbox → Engine na Denonu něco ztrácí nebo je pomalý.

### Fáze 7 — Sync a pohodlí
- [ ] Automatická záloha flashky po každém gigu (při připojení), historie verzí flashky.
- [ ] Načtení **historie hraní** z flashky zpět do knihovny.
- [ ] Import cue bodů upravených na CDJ zpět do knihovny (pokud je přehrávač uloží).
- [ ] Phrase analýza (PSSI) — až úplně na konci.
- [ ] Extra: export do Traktor/Engine DJ, normalizace hlasitosti souborů (jako `baken headroom`).

### Fáze 8 — Hloubková analýza (stems) a Set Planner
Cíl: Smart Cue rozpozná i **vokály** a aplikace umí **naplánovat celý set**: pořadí skladeb, kde přesně přejít, jak dlouhý přechod, podle úrovně DJe.

**Rešerše (2026-10):**
| Nástroj | Co umí | Licence | Použití |
|---|---|---|---|
| **CUE-DETR** (ETH Zürich, ISMIR 2024) — github.com/ETH-DISCO/cue-detr | Odhad cue bodů z Mel spektrogramu, natrénováno na **21 461 cue od profi DJů v 4 710 EDM skladbách** | kód MIT, váhy na HF (licence neuvedena) | pozice cue (bez typu) → kombinovat s naší strukturou |
| **Demucs / HT-Demucs** (Meta) | rozdělení na stems: vokály, bicí, basa, ostatní | kód MIT, váhy sporné („research“) | **detekce vokálů** (energie vokální stopy v čase), čisté basy pro drop/breakdown |
| **All-In-One (allin1)** — github.com/mir-aidj/all-in-one | beaty, downbeaty, segmenty intro/verse/chorus/bridge/break/outro (nad Demucs stems) | kód MIT, váhy neuvedeny; trénováno na Harmonix (pop) | struktura u vokálních/pop edity |
| **Automix / M-DJCUE** (Zehren, ISMIR 2020) | pravidlová detekce „switch points“ pro EDM, dataset 134 skladeb | — | reference pravidel + testovací data |
| Mixed In Key | key, BPM, energy 1–10, 8 auto cue (intro, breakdown, drop, outro) | komerční | inspirace: energy level + šablona cue |
| DJ.Studio | Automix: pořadí podle BPM/key/energie (Camelot), timeline přechodů, doplnění chybějících skladeb | komerční | inspirace pro Set Planner UI |
| Essentia | key, loudness, modely (vokál/instrumentál, nálada) | **AGPL**, modely **CC BY-NC-ND** | jen pro osobní použití; raději vlastní |

**Architektura:** lokální **analytický sidecar v Pythonu** (spravuje ho aplikace přes `uv`, běží na Apple Silicon přes MPS), výsledky v cache (per soubor, hash). Později možný port na ONNX/Rust.

**Výstup analýzy na skladbu:** beaty+downbeaty, fráze, **vokální sekce** (z vocal stemu), breakdowny/dropy (z bass+drums stemů), energy level 1–10, tónina (Camelot), intro/outro délky, „mix-in okno“ (bez vokálu) a „mix-out okno“.

**Set Planner:**
- Vstup: výběr skladeb / playlist, délka setu, **úroveň** (Začátečník / Pokročilý / Profík), energetická křivka (Warm-up, Peak time, Closing, vlastní), styl (dlouhé blendy / rychlé přechody), tolerance tempa a tóniny.
- Pořadí: optimalizace (beam search / simulated annealing) nad cenou přechodu: Camelot vzdálenost, rozdíl BPM (a postupné změny tempa), shoda s energetickou křivkou, **kolize vokálů**, žánr/podobnost.
- Přechody: pro každý pár zvolí mix-out v A a mix-in v B zarovnané na fráze (16/32 taktů), délku podle úrovně, bass swap na frázi, případně pitch/key shift; varování při kolizi vokálů.
  - Začátečník: jen harmonicky kompatibilní (±1 Camelot, relativní), BPM ±3 %, dlouhé intro→outro blendy, žádné vokály přes sebe, tipy „bass swap na taktu 17“.
  - Pokročilý: energy boost (+2), drop swap, 16taktové přechody, tempo ramp.
  - Profík: double dropy (zarovnání dropů A+B), krátké přechody, echo-out / cut, odvážnější tóniny.
- Výstup: **timeline setu** s energetickou křivkou, přechody barevně (OK / pozor / kolize), **náhled přechodu** (přehraje oba tracky přes sebe), zápis cue „Mix in“, „Mix out → další skladba“ s komentářem, playlist v pořadí na flashku, tahák pro hraní.

**Postup:**
1. S1 — Set Planner nad tím, co máme (BPM, key, energie z waveformu, Smart Cue) → funguje hned.
2. S2 — Python sidecar: Demucs (vokály) + CUE-DETR (+ allin1) → Smart Cue s vokály, přesnější cue, vokální kolize v přechodech.
3. S3 — Náhled přechodů (dvě stopy přes sebe v aplikaci), učení z tvých úprav (porovnávat s tvými cue jako teď).

---

## 6. Testovací strategie
| Úroveň | Co | Jak |
|---|---|---|
| Unit | každý ANLZ tag, každý typ PDB řádku | Rust testy |
| Roundtrip | golden soubory z rekordboxu | `read → write → byte diff` |
| Fuzz/property | náhodné knihovny | proptest + rekordcrate jako nezávislý parser |
| Křížová validace | výstup | rekordcrate, pyrekordbox, crate-digger (Kaitai) |
| Rekordbox | rekordbox otevře naši flashku jako zařízení | ručně, před každým releasem |
| Hardware | matice přehrávačů × formáty audia × funkce | checklist (níže), před každým releasem |
| Emulace | bez hardwaru | prozkoumat `cdj3k-emu` (DjManager #553) |

**Hardware checklist (per přehrávač):** flashka se načte · playlisty a složky · řazení/hledání · artwork · waveform preview + detail + barvy · beatgrid a sync · hot cue A–H (pozice + barvy) · memory cue · loopy + aktivní loop · seek ve VBR MP3 · tónina, BPM, rating, barva tracku · 1000+ tracků · diakritika v názvech.

---

## 7. Rizika
| Riziko | Dopad | Opatření |
|---|---|---|
| CDJ odmítne PDB kvůli drobné odchylce v rozložení stránek | export nefunguje | co nejvěrněji kopírovat strukturu rekordboxu (golden roundtrip), reference baken |
| Posun cue/gridu o desítky ms (MP3 decoder delay) | cue nesedí na beat | kalibrace proti rekordboxu, testy s VBR/CBR, LAME header |
| Různé firmwary se chovají jinak | funguje jen někde | hardwarová matice, uvést podporované modely |
| AlphaTheta změní formát | nové přehrávače | `export.pdb` je legacy a stabilní; OneLibrary sledovat |
| Licence (GPL rbox/Mixxx, neznámá fourfour) | omezení distribuce | jádro psát vlastní; GPL knihovny jen v oddělených nástrojích |
| Phrase analýza (PSSI) | těžké | volitelná, nízká priorita |
| Klonovač smaže špatný disk | ztráta dat | jen externí vyměnitelné disky, potvrzení s názvem/velikostí/modelem, zdroj read-only |
| Blokový klon zkopíruje i poškození | rozbitá záloha | výchozí je souborový klon s ověřením; migrace s přestavbou pro rozbité flashky |

---

## 8. Rozhodnutí a otevřené otázky
**Rozhodnuto:**
- **Jen macOS.**
- **Všechny Pioneer přehrávače + občas Denon** → vždy `export.pdb` + ANLZ + OneLibrary; Denon v testovací matici; `Engine Library/` na flashce nesahat.

**Otevřené:**
1. **Kde máš dnes cue/loopy?** V rekordboxu (pak Fáze 2 z XML, žádná ztráta) — nebo začínáme od nuly?
2. **Jen pro tebe, nebo veřejný open-source projekt?** → licence (MIT vs. GPL) a jestli smíme použít rbox/Mixxx analyzátor.
3. Ke kterým přehrávačům máš přístup na testování (doma / v klubu)? Hlavně OneLibrary-only model a Denon.

---

## Zdroje
- https://djl-analysis.deepsymmetry.org (Deep Symmetry – dokumentace formátů)
- https://github.com/Deep-Symmetry/crate-digger
- https://github.com/M-Igashi/baken (issues #115, #208)
- https://github.com/morizkraemer/fourfour (`pioneer-usb-writer/reference-code/PIONEER.md`)
- https://github.com/Radexito/DjManager (PR #559, issue #553)
- https://github.com/kimtore/rex
- https://github.com/xsco/libdjinterop (Denon Engine Library, LGPL-3.0)
- https://community.enginedj.com/t/preparing-to-play-a-club-gig-with-pioneer-system-doomed/33075 (Denon převádí rekordbox USB)
- https://github.com/fotoner/DJCrate/issues/42
- https://github.com/Holzhaus/rekordcrate · https://docs.rs/rekordcrate
- https://github.com/dylanljones/rbox · https://github.com/dylanljones/pyrekordbox
- https://github.com/chrisle/alphatheta-connect · https://github.com/chrisle/onelibrary-connect
- https://gist.github.com/0xdevalias/b803476793b56f7c45e6361799168eb0
- https://alphatheta.com/en/information/dj-brands-unite-to-launch-onelibrary/
- https://musictech.com/news/gear/alphatheta-onelibrary/
