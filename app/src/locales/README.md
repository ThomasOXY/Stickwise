# Translations

Stickwise speaks Czech, English and German. Every text the DJ sees comes from
here (frontend) or from `crates/i18n/locales` (texts the Rust side sends:
errors, job labels and progress, the menu). Both use the same format.

## Format

One JSON file per part of the app, all languages side by side:

```json
{
  "cs": { "title": "Synchronizace", "tracks": { "one": "{count} skladba", "few": "{count} skladby", "other": "{count} skladeb" } },
  "en": { "title": "Sync", "tracks": { "one": "{count} track", "other": "{count} tracks" } },
  "de": { "title": "Sync", "tracks": { "one": "{count} Track", "other": "{count} Tracks" } }
}
```

- Keys are camelCase English, grouped by screen or component.
- `{name}` placeholders are filled from the call; every language uses the same ones.
- Plurals: Czech `one`/`few`/`other` (1 / 2–4 / 5+), English and German `one`/`other`, chosen by `{count}`.
- Czech is the reference. `npm run check` (and `cargo test -p i18n`) fail when a key or a placeholder is missing in another language.

## Use

Frontend: `t("sync.title")`, `t("common.tracks", { count: n })` from `lib/i18n.svelte`. Keys are typed: a typo fails `npm run check`. A new file needs an import in `lib/i18n.svelte.ts`. Numbers and dates: `num()`, `date()` from `lib/format`.

Rust: `i18n::t!("sync.copying")`, `i18n::t!("sync.done", count = n, name = stick)`. A new file needs a line in `FILES` in `crates/i18n/src/lib.rs`.

Never build a sentence from pieces (`"Copied " + n + " tracks"`): word order differs between languages. One key per sentence, with placeholders.

## Voice

Short, plain, friendly; the DJ is addressed as one person: Czech *ty*, German *du*. English without filler. Labels and buttons in sentence case (German nouns capitalised, as German does).

## Glossary

| Czech | English | German |
|---|---|---|
| flashka | USB stick (short: stick) | USB-Stick (kurz: Stick) |
| skladba | track | Track |
| playlist / složka / set | playlist / folder / set | Playlist / Ordner / Set |
| knihovna | library | Bibliothek |
| přehrávač | player | Player |
| analýza | analysis | Analyse |
| beatgrid, grid | beat grid, grid | Beatgrid, Grid |
| tónina | key | Tonart |
| tempo | tempo (BPM) | Tempo (BPM) |
| waveform | waveform | Waveform |
| hot cue / memory cue | hot cue / memory cue | Hot Cue / Memory Cue |
| přechod | transition | Übergang |
| synchronizace | sync | Sync |
| spárovat | pair | koppeln |
| zrcadlit | mirror | spiegeln |
| úloha | job | Aufgabe |
| Nastavení | Settings | Einstellungen |
| formátovat | format | formatieren |
| vysunout | eject | auswerfen |

Product and gear names stay as they are: rekordbox, Engine DJ, Denon, CDJ, XDJ, DJM, My Settings, Smart Cue, Beat This!, CUE-DETR.
