# v2 redesign — implementation brief (for sub-agents)

Goal: make the Svelte UI in `app/src` look like the v2 design. References: `redesign/v2/*.png`
(1600×1000 mockups; Czech labels). Visual change only: **keep every feature, handler, state
binding and label meaning**. Don't add backend calls. Keep Czech UI text.

## Already done (do NOT edit these files)
- `app/src/app.css`: v2 tokens + shared classes. `app/src/App.svelte`, `lib/Rail.svelte`,
  `lib/Sidebar.svelte`, `lib/PlaylistTree.svelte` (shell: icon rail · library panel · main card).
- Main area is a rounded card (`--surface-1`) on `--bg`. Don't draw borders between big regions:
  use spacing (16–24 px), soft surfaces (`--surface-2/3`) and radius instead of 1 px lines.

## Tokens (app.css)
`--bg --surface-1 --surface-2 --surface-3 --border --hairline --accent --accent-soft --on-accent
--success(-soft) --warning(-soft) --danger(-soft) --text --text-2 --text-3 --r-panel(12) --r-control(8)
--shadow-pop --font-ui --font-readout`. Old names (`--panel`, `--line`, `--cue`, `--fault`, …) are aliases.
Shared classes: `.btn` (secondary, surface-3), `.btn.ghost`, `.btn.primary` (amber), `.btn.danger`,
`.btn.lg` (40 px), `.btn.icon`; `.seg > button.on` segmented control; `.eyebrow` small-caps label;
`.keypill` with inline `style:--k={keyColor(key)}` (keyColor from `lib/format.ts`); `.readout`
(Barlow Condensed numbers). Lucide icons from `@lucide/svelte`.

## Style rules
- Type: page title 34px/700 (letter-spacing -0.02em); section 15–16px/600; body 13–13.5px;
  meta 12px `--text-3`; numbers in `.readout`.
- Only one primary (amber) action per view. Secondary actions `.btn`, tertiary `.btn.ghost`.
- Radii: panels 12–16, controls 8, chips 12–13 (pill).
- Sheets/modals: `--surface-1`, radius 18, `box-shadow: var(--shadow-pop)`, header with 40px icon tile
  + title 20/700 + subtitle, body, footer right-aligned (Zrušit ghost + primary .lg).
  Scrim: rgb(5 6 8 / .6) + backdrop-filter blur(6px).

## Verify
1. `cd app && npx svelte-check --tsconfig ./tsconfig.json` → 0 errors, 0 warnings.
2. Screenshots without the Tauri backend: Vite dev server already runs on http://localhost:1420.
   Copy `/tmp/shot/shoot.mjs` to `/tmp/shot/<yourname>.mjs`, change scenarios, run
   `cd /tmp/shot && node <yourname>.mjs /tmp/shot/<yourname> all` (needs sandbox off; uses Brave
   headless + mock backend `/tmp/shot/mock.js` with real library data). Drive state via
   `app((a)=>{...})` like the existing script. Compare with the reference PNG, iterate.
3. Don't touch `/Applications/RB-EXPORT.app`, don't run `tauri build`, don't commit.
