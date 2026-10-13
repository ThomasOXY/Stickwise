# Design prompt — RB-EXPORT desktop app (for Pencil.dev)

Design a macOS desktop app called **RB-EXPORT**: a USB stick manager and exporter for DJs who play on Pioneer CDJ / XDJ players (and sometimes Denon). It replaces rekordbox's USB export, which corrupts sticks and silently exports files the players cannot play. The user is a working club DJ preparing sticks before a gig: they need to see at a glance whether a stick is safe to play from, browse its playlists, preview tracks with their real waveform and hot cues, copy playlists to disk, and clone sticks.

UI copy is in **Czech** (examples below). Window: 1440 × 900, macOS, dark appearance, native traffic-light window controls inset into the sidebar, translucent (vibrancy) sidebar.

## Visual direction: "Booth"

The look of a dimly lit DJ booth: graphite hardware, small precise labels, and colour that comes only from the music itself. The one bold element is the **track waveform**, drawn in rekordbox/CDJ-style colours with hot cue flags in their real cue colours. Everything around it stays quiet and disciplined.

**Colour**
- Booth (window background): `#14161B` — blue-tinted graphite, not pure black
- Panel (raised surfaces, sidebar tint, deck): `#1C1F26`
- Line (dividers, table rules, inputs): `#2A2E37`
- Text: `#E8E6E1` (warm off-white); secondary text `#8B909C`
- Cue amber `#F2A33A` — the colour of the CUE button on a CDJ. Used **only** for the primary action on a screen, the playhead and keyboard focus.
- Play green `#59C98A` — "OK / plays on this player"
- Fault red `#E5484D` — "will not play / error"
- Warning `#D9B54A` — "plays, but something is off"
- Waveform and hot cue colours come from the track data (blue/orange/white RGB waveform; hot cues A–H in pink, red, orange, yellow, green, aqua, blue, purple).

**Type**
- UI text: SF Pro (system), sentence case everywhere, no all-caps labels.
- Readouts (BPM, key, time, track count, capacity): **Barlow Condensed** SemiBold with tabular figures — like the CDJ-3000 screen.
- Type scale: 11 / 13 / 15 / 20 / 28 px. Body 13 px. Left-aligned throughout.

**Shape and depth**
- Radius: 6 px on buttons and inputs, 10 px on sheets and dialogs, 0 on table rows. Not everything is a card.
- Depth comes from surface colour (Booth → Panel), not from drop shadows. Sheets and dialogs get one soft shadow.
- Hairline dividers in Line colour. Dense, information-rich tables like a pro-audio app, 28 px row height.

**Status language:** a small filled dot next to a name. Green = plays everywhere, amber-yellow = warnings, red = some tracks will not play. The same language is used for sticks, tracks and players.

**Avoid:** identical rounded cards with grey shadows, gradient washes, all-caps eyebrow labels, monospace data labels, meta strings joined with middle dots, arrows appended to button text, an acid-green accent, and decorative numbering.

## Layout

```
┌──────────────┬──────────────────────────────────────────────────────────────┐
│ ● ● ●        │  Stick header: name, capacity bar, filesystem, status, actions│
│              ├───────────────┬──────────────────────────────────────────────┤
│ Flashky      │ Playlists     │ Track table                                  │
│  ● THOMASOXY │ tree          │                                              │
│  ● BACKUP-2  │               │                                              │
│              │               │                                              │
│ Knihovna     │               │                                              │
│ (coming)     │               │                                              │
│              ├───────────────┴──────────────────────────────────────────────┤
│ Úlohy        │ Deck: play, title/artist, BPM, key, time, colour waveform,  │
│  progress    │ hot cue flags A–H, playhead                                  │
└──────────────┴──────────────────────────────────────────────────────────────┘
```

- Sidebar 240 px, translucent. Sections: **Flashky** (connected sticks, each with status dot, name and a thin capacity bar; an eject icon on hover), **Knihovna** (dimmed, "Brzy"), and at the bottom **Úlohy** (running jobs with small progress bars).
- Deck at the bottom spans the full content width, 120 px tall, always visible once a track is selected.

## Real content to use

Stick **THOMASOXY**: 64 GB, exFAT, 12.3 GB used, 567 tracks, 35 playlists, 1 history playlist.
Playlists (newest last): "1.1.2024 - TECHHOUSE" (28), "PARADOX - SET" (16), "23.5.2025 - TechHouse" (50), "16.10.2025" (48), "SET 23.1.2026" (13), "16.2.2026 - TechHouse" (19), "27.3.2026" (6), "23.5.2026 - RAVE" (14), "12.6.2026 - Rozjezd" (20), "21.8.2026" (26), "21.8.2026 - HARD" (12). History: "HISTORY 001" (9).
Tracks (title, artist, BPM, key, time, format):
- Rise Again X Surrender (Yarpo Edit) — Hardwell, Ryos, Alesso, Becky Hill — 140.00 — 8A — 4:02 — MP3 320
- Promises X In The Music (Yarpo Edit) — NERO, R3SPAWN, Robert Junior, CAZOK — 145.00 — 10A — 3:43 — MP3
- MAD X Calling (Yarpo Edit) — Will Sparks, Sebastian Ingrosso, Alesso — 127.00 — 12A — 5:10 — MP3
- Party Rock Anthem (Never Sleep Remix) — LMFAO — 130.00 — 4A — 3:58 — MP3 (has 8 hot cues A–H)
- DAFT PUNK - Around The World (Westend Edit) — 126.00 — 7A — 7:33 — WAV 24-bit 44.1 kHz
- Maneater x Call You Back (Mo27Da Mashup) — Nelly Furtado x Layton Giordani — 128.00 — 9A — 3:20 — WAV 32-bit float (does not play on any CDJ)
- Blah Tour (Patrick Junior Mashup) — Macky Gee x Armin van Buuren — 174.00 — 6A — 4:05 — MP3 with a broken tag

Health check result for this stick: 5 errors (4 × "32-bit float WAV – nepřehraje žádný CDJ", 1 × "poškozený tag MP3"), 182 warnings ("velikost souboru nesedí s databází"), filesystem exFAT ("CDJ-3000 ano, CDJ-2000NXS2 a starší ne"), 551 macOS "._" files.

## Screens

### 1. No stick connected (empty state)
Sidebar Flashky section empty. Content area: a quiet illustration of a USB stick outline in Line colour, headline "Připoj flashku", one line of text "Uvidíš její playlisty, stav a co na kterém přehrávači pojede.", and a secondary button "Otevřít složku…" for a stick image or backup folder. No primary button.

### 2. Stick browser (main screen) — the hero screen
- **Stick header:** name "THOMASOXY" in 28 px Barlow Condensed; beside it a capacity bar (12.3 of 64 GB) and filesystem "exFAT". A status pill: red dot + "5 skladeb nepůjde přehrát". Actions on the right: primary amber "Zkontrolovat flashku", secondary "Exportovat na disk", secondary "Klonovat", and an eject icon button.
- **Player strip** under the header: four compact chips "CDJ-2000NXS", "CDJ-2000NXS2", "CDJ-3000", "CDJ-3000X", each with a status dot (NXS and NXS2 red because of exFAT, 3000 and 3000X amber because of the float WAVs). Clicking a chip filters the table to tracks that will not play there.
- **Playlists tree (260 px):** search field on top, then playlists with track counts right-aligned in secondary text; folders collapsible; a separate "Historie" group at the bottom. Selected playlist "16.2.2026 - TechHouse" highlighted with a Panel background and a 2 px amber bar on the left.
- **Track table:** columns # · status dot · Název · Interpret · BPM · Tónina · Čas · Formát · Hot cue. BPM/key/time in Barlow Condensed tabular figures. Key shown as Camelot (8A) with a tiny colour swatch matching the Camelot wheel. Hot cue column shows up to 8 tiny coloured squares. Rows with problems show a red or amber dot and the format cell in that colour (e.g. "WAV 32f"). One row selected.
- **Deck (bottom):** circular play button (amber when playing), title and artist, large BPM "128.00" and key "9A" readouts, elapsed / remaining time, and the **full-width colour waveform** of the whole track with hot cue flags A–H above it (small coloured tabs with the letter), memory cues as small triangles below, beat-bar ticks, and a thin amber playhead. Hovering the waveform shows a time tooltip; clicking seeks.

### 3. Health check (Zkontrolovat flashku)
Replaces the track table with a report; header and deck stay.
- Top: a summary row of four numbers in Barlow Condensed: "567 skladeb", "5 chyb" (red), "182 varování" (amber), "551 souborů ._" (secondary).
- **Players matrix:** one row per player (CDJ-2000NXS, CDJ-2000NXS2, CDJ-3000, CDJ-3000X); columns "Souborový systém", "Formáty", "Výsledek", with green/red/amber marks and a short reason, e.g. "exFAT nepodporuje — použij FAT32".
- **Chyby** list: each item shows the track, artist, a plain-language problem ("32-bit float WAV. CDJ přehraje jen 16 a 24 bitů."), which playlists it is in, and actions "Ukázat ve Finderu" and "Opravit…" (convert to 24-bit).
- **Varování** collapsed by default with a count: "182 skladeb má v databázi jinou velikost souboru než ve skutečnosti. Hrát půjdou." Expandable.
- **Úklid**: "551 souborů ._ od macOS" with a button "Smazat ._ soubory".
- Footer: "Kontrola jen četla, na flashku se nic nezapsalo." in secondary text.

### 4. Export to disk (sheet over the main screen)
A macOS sheet, 560 px wide. Title "Exportovat playlisty na disk".
- Playlist picker: checkbox tree with "Vybrat vše", counts and total size ("12 playlistů, 214 skladeb, 4.2 GB").
- Destination: path field "~/Music/THOMASOXY" with "Změnit…".
- Options: "Očíslovat podle pořadí (01 – …)", "Pojmenovat Interpret – Název", "Vytvořit .m3u8", "Zahrnout historii", and duplicates choice as a segmented control "Klon (nezabírá místo) / Kopie / Odkaz".
- Preview of the resulting folder structure (3–4 lines, file-tree style).
- Buttons: "Zrušit" and primary "Exportovat 214 skladeb".
- **Progress state** of the same sheet: large progress bar, "128 / 214 · 2.4 / 4.2 GB" — use separate labels rather than a dotted string — current track name, "Zastavit". **Done state:** green check, "Hotovo. 214 skladeb v ~/Music/THOMASOXY", buttons "Otevřít ve Finderu" and "Zavřít".

### 5. Clone stick (wizard sheet, 3 steps — a real sequence, so a step indicator is fine)
1. "Zdroj": the selected stick, shown read-only with a lock icon and "Zdroj se připojí jen pro čtení".
2. "Cíl": list of other connected removable drives with name, size and model; internal disks are never listed. Mode as segmented control: "Klon 1:1 (soubory)" / "Obraz flashky" / "Migrace s opravou". Option "Před kopírováním naformátovat na FAT32".
3. "Potvrzení": a red-bordered confirmation box: "Všechna data na BACKUP-2 (SanDisk Ultra, 64 GB) budou smazána." The user must type the stick name to enable the red button "Smazat a klonovat".
Then a progress view with per-phase steps: Formátování → Kopírování → Kontrola souborů → Kontrola databáze.

### 6. Track detail (side panel, opens from a double-click on a row)
360 px panel sliding from the right over the table: artwork, title, artist, album, BPM, key, time, format details ("WAV · PCM 24-bit · 44.1 kHz · 79.4 MB" — as separate rows, not a dotted string), a zoomed scrolling waveform (detail view) with beat grid lines, a list of hot cues A–H with colour, time and comment, memory cues and loops, and the path on the stick with "Ukázat ve Finderu".

## Interaction notes
- Keyboard first: ↑/↓ moves in the table, Space plays/pauses, ⌘K opens a command palette (show it once as an overlay variant of screen 2 with commands like "Zkontrolovat flashku", "Exportovat playlist…", "Vysunout THOMASOXY").
- Drag a playlist from the tree onto a stick in the sidebar: the stick row highlights with an amber outline and the label "Pustit pro export" (show as a variant).
- Motion only in response to actions: sheets slide down, the side panel slides in, progress bars fill. No decorative animation.
- Visible amber focus ring (2 px) on all controls. Text contrast at least 4.5:1.

## Deliverables
Frames: (1) empty state, (2) stick browser with a playing track, (2b) command palette overlay, (2c) drag-to-stick variant, (3) health check report, (4) export sheet — setup, progress and done, (5) clone wizard — step 3 confirmation, (6) track detail panel. Also a small component sheet: buttons (primary, secondary, destructive, icon), status dots, player chips, table row states (default, hover, selected, error, warning), hot cue flags A–H, segmented control, progress bar, sidebar stick row.
