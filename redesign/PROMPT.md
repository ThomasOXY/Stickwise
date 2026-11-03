# Redesign prompt for pencil.dev

Attach every image from `screenshots/` together with the prompt below.

---

## Prompt

Redesign the desktop app shown in the attached screenshots. It is **RB-EXPORT**, a macOS app (Tauri, about 1600×1000 px window) for DJs. It replaces Pioneer rekordbox for library management and USB export. A DJ uses it to:

- keep a music library of about 5,000 tracks in playlists, folders and **sets** (a set is an ordered list of tracks for one gig)
- preview a track on a built-in **deck**: a scrolling 3-band waveform, an overview waveform, hot cues A–H, memory cues, loops and a beat grid editor
- generate **Smart Cue** suggestions (mix in, build, drop, break, mix out) and plan a set's order by key, tempo and energy (**Set Planner**)
- sync sets to a USB stick for Pioneer CDJ/XDJ players (and Denon Engine), check whether each player can read the stick (**stick health report**), format sticks and set the players' "My Settings"

The UI is in **Czech**. Keep every label in Czech exactly as it appears in the screenshots, and keep all features and information. This is a visual and layout redesign, not a feature change.

### What is wrong with the current design
- Too dense and flat. Every region sits edge to edge with 1 px dividers, so nothing stands out and the eye has nowhere to rest.
- Weak hierarchy. The page header, playlist header, table header and deck toolbar all compete with each other. Too many buttons have the same visual weight; the orange primary action appears in several places at once.
- Three columns (sources sidebar, playlist tree, track table) plus a detail panel plus the deck feel crammed. The playlist tree sits in its own full-height column, which wastes space.
- The track table is cramped: titles are cut off, the artist column is often empty, the hot-cue dots are tiny, and the row rhythm is monotonous.
- The deck at the bottom is a wall of controls. The waveform is great, but the transport, BPM/key readouts, zoom, cue pads and loop buttons are scattered in one row each with no grouping.
- Modals (sync, export, format, player settings) are walls of text with checkboxes and no clear step structure.
- The Tools launchpad (⌘K) uses loud, mismatched gradient app icons that do not fit the rest of the UI.

### Direction
- **Calm, premium, airy pro-audio tool.** Think Linear, Arc and Things 3 crossed with the restraint of a modern DAW, like Bitwig or the Ableton Live 12 browser. Dark mode first.
- **More breathing room:** an 8 pt spacing grid, generous padding inside panels, and 12–16 px gaps between panels instead of hairline borders. Use soft elevated surfaces (subtle tonal steps and very light shadows) rather than lines.
- **One clear accent.** Keep the warm amber/orange (`#f2a33a`) as the single brand and primary-action colour. Use green only for "plays / OK" status and red only for errors. Hot-cue colours stay exactly as they are (rekordbox colours); they are data, not decoration.
- **Typography:** Inter for the UI, a condensed tabular font (now Barlow Condensed) for numeric readouts (BPM, key, time, counts). Build a clear type scale: large page title, medium section titles, small muted meta text. Use fewer uppercase labels.
- **Rounded, soft geometry:** about 10–12 px radius on panels and 8 px on controls; segmented controls instead of rows of plain buttons.
- **Icons:** keep the Lucide line icons, used sparingly and consistently. Redesign the Tools launchpad as a refined command palette or grid with monochrome icon tiles carrying a subtle accent tint, not candy gradients.

### Screens to design (one frame each, matching the screenshots)
1. **Library: set view** (`01`, `02`). App shell with:
   - Left sidebar: Tools button (⌘K), sources (this Mac, USB sticks with a capacity bar) and running jobs at the bottom. Consider merging the playlist tree into this sidebar as a collapsible section so the main area gets more width.
   - Header: title, stats (4,664 tracks · 1 set · 123 playlists), one primary action ("Synchronizovat na flashku") with the others secondary.
   - Track table: comfortable 40–44 px rows, artwork thumbnail, title and artist stacked in one cell, BPM and key (key shown as a coloured Camelot pill), duration, format, date added, and hot-cue indicators as small coloured bars.
   - Right track detail panel: big artwork, readouts, file info, cue list.
2. **Deck / player** (bottom dock, visible on every screen). Make the waveform the hero, full width with clear cue markers and a playhead. Group the controls into clusters: transport · volume | Smart Cue · Grid · zoom | BPM · key · time readouts. Cue pads A–H become a neat row of coloured pads; the loop and memory controls go into a compact group on the right. Consider a collapsible dock.
3. **Smart Cue popover** (`03`): a list of suggested cue points with checkboxes, the AI badge and actions.
4. **Beat grid mode** (`04`).
5. **Whole library** (`05`): the same table with many tracks, showing that it scales.
6. **Set Planner** (`06`): options as segmented controls in a tidy settings card (order, transition cues, length, level, energy), one "Naplánovat" button, and the resulting plan below.
7. **Tools launchpad / command palette** (`07`).
8. **Sync to stick sheet** (`08`): restructure it as clear steps: choose stick → choose playlists → review summary (tracks, copy size, warnings) → options → confirm. Move the long explanation into a collapsible "Jak to funguje" note.
9. **Player settings, format stick, export** sheets (`09`, `10`, `13`): consistent modal or sheet pattern.
10. **USB stick view and health report** (`11`, `12`): the stick header with a capacity meter and per-player compatibility chips, then a health dashboard with summary stat cards (tracks, errors, warnings, cleanup), a player compatibility table and error rows with actions.

### Deliverables
- A small design system first: colour tokens (background, surface 1/2/3, border, text 1/2/3, accent, success, warning, danger), the type scale, spacing, radii, and components (button primary/secondary/ghost, segmented control, input/search, table row with hover/selected states, chip/pill, Camelot key pill, hot-cue pad, stat card, modal sheet, sidebar item).
- Then the screens listed above at 1600×1000, all built from those components.
- Keep the information density right for a pro tool: airy, but a DJ must still see 15+ tracks in the table at once.
