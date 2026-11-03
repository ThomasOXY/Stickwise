<script lang="ts">
  import { BookmarkPlus, Magnet, Trash, Undo2 } from "@lucide/svelte";
  import type { Cue } from "./api";
  import { SLOT_LETTERS, beatsLater, blankCue, cssColor, firstFreeSlot, hotCue, replaceCue, snap, withoutSlot } from "./cues";
  import { cueTime } from "./format";
  import { app } from "./state.svelte";

  interface Props {
    getTime: () => number;
    setTime: (t: number) => void;
    beats: [number, number][];
    bpm: number;
  }

  let { getTime, setTime, beats, bpm }: Props = $props();

  const cues = $derived(app.detail?.cues ?? []);
  const memoryCount = $derived(cues.filter((c) => c.slot === 0).length);
  let loopIn = $state<number | null>(null);
  let menu = $state<{ slot: number; x: number; y: number } | null>(null);
  let comment = $state("");

  /** Current position in ms, on the grid when quantize is on. */
  function here(): number {
    const ms = getTime() * 1000;
    return app.quantize ? snap(ms, beats) : Math.round(ms);
  }

  export function trigger(slot: number) {
    const c = hotCue(cues, slot);
    if (c) setTime(c.timeMs / 1000);
    else app.setCues([...cues, blankCue(slot, here())]);
  }

  export function remove(slot: number) {
    if (hotCue(cues, slot)) app.setCues(withoutSlot(cues, slot));
  }

  export function addMemory() {
    const t = here();
    if (cues.some((c) => c.slot === 0 && Math.abs(c.timeMs - t) < 10)) return;
    app.setCues([...cues, blankCue(0, t)]);
  }

  function autoLoop(n: number, asHot: boolean) {
    const start = here();
    const loop = { ...blankCue(0, start), loopEndMs: beatsLater(start, n, beats, bpm), loopBeats: [n, 1] as [number, number] };
    if (asHot) {
      const slot = firstFreeSlot(cues);
      if (slot == null) {
        app.notice = "Všech 8 hot cue je obsazených. Smaž jeden, nebo ulož smyčku jako memory.";
        return;
      }
      loop.slot = slot;
    }
    app.setCues([...cues, loop]);
  }

  function loopOut() {
    if (loopIn == null) return;
    const end = here();
    if (end > loopIn + 20) app.setCues([...cues, { ...blankCue(0, loopIn), loopEndMs: end }]);
    loopIn = null;
  }

  function openMenu(e: MouseEvent, slot: number) {
    e.preventDefault();
    const c = hotCue(cues, slot);
    if (!c) return;
    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
    comment = c.comment;
    menu = { slot, x: r.left, y: r.top };
  }

  function update(change: Partial<Cue>) {
    if (!menu) return;
    const c = hotCue(cues, menu.slot);
    if (c) app.setCues(replaceCue(cues, c, { ...c, ...change }));
  }

  function setColor(code: number) {
    update({ hotColor: code, hotLed: null });
  }

  function setLoop(n: number | null) {
    if (!menu) return;
    const c = hotCue(cues, menu.slot);
    if (!c) return;
    update(
      n == null
        ? { loopEndMs: null, loopBeats: null }
        : { loopEndMs: beatsLater(c.timeMs, n, beats, bpm), loopBeats: [n, 1] },
    );
  }

  function onKey(e: KeyboardEvent) {
    const el = e.target as HTMLElement;
    if (el?.tagName === "INPUT" || app.toolsOpen || app.exportOpen || app.writeOpen || !app.detail) return;
    if (e.key === "Escape") menu = null;
    const digit = /^Digit([1-8])$/.exec(e.code);
    if (digit && !e.metaKey && !e.ctrlKey) {
      e.preventDefault();
      const slot = Number(digit[1]);
      e.shiftKey ? remove(slot) : trigger(slot);
    } else if (e.key === "m" && !e.metaKey) {
      addMemory();
    } else if (e.key === "q" && !e.metaKey) {
      app.setQuantize(!app.quantize);
    } else if (e.key === "z" && e.metaKey) {
      e.preventDefault();
      app.undo();
    }
  }
</script>

<svelte:window onkeydown={onKey} onclick={(e) => menu && !(e.target as HTMLElement).closest(".menu") && (menu = null)} />

<div class="pads">
  {#each SLOT_LETTERS as letter, i}
    {@const slot = i + 1}
    {@const c = hotCue(cues, slot)}
    <button
      class="pad"
      class:set={c}
      style:--c={c ? cssColor(c) : undefined}
      title={c ? `${letter}: skok na ${cueTime(c.timeMs)} · pravým tlačítkem úpravy · ⇧${slot} smaže` : `Nastavit hot cue ${letter} na aktuální pozici (${slot})`}
      onclick={(e) => (e.altKey ? openMenu(e, slot) : trigger(slot))}
      oncontextmenu={(e) => openMenu(e, slot)}
    >
      <span class="letter">{letter}</span>
      <span class="txt">
        {#if c}
          <span class="readout">{cueTime(c.timeMs)}</span>
          <span class="cm">{c.comment || (c.loopEndMs ? "smyčka" : "")}</span>
        {:else}
          <span class="readout">–</span>
          <span class="cm">volný</span>
        {/if}
      </span>
      {#if c?.loopEndMs}<span class="tag">{c.loopBeats ? `${c.loopBeats[0]}` : "∞"}⟲</span>{/if}
    </button>
  {/each}

  <div class="group" role="group" aria-label="Smyčka">
    <button class="g" class:armed={loopIn != null} onclick={() => (loopIn = here())} title="Začátek smyčky">IN</button>
    <button class="g" onclick={loopOut} disabled={loopIn == null} title="Konec smyčky, uloží memory smyčku">OUT</button>
    {#each [4, 8, 16, 32] as n}
      <button class="g" onclick={(e) => autoLoop(n, e.shiftKey)} title="Smyčka {n} dob od aktuální pozice. Se Shiftem jako hot cue.">{n}</button>
    {/each}
  </div>
  <div class="group" role="group" aria-label="Cue">
    <button class="g icon" onclick={addMemory} title="Memory cue na aktuální pozici (M){memoryCount ? ` · ve skladbě ${memoryCount}` : ''}" aria-label="Přidat memory cue">
      <BookmarkPlus size={14} />{#if memoryCount}<span class="n">{memoryCount}</span>{/if}
    </button>
    <button
      class="g icon"
      class:on={app.quantize}
      onclick={() => app.setQuantize(!app.quantize)}
      title="Quantize: cue se přichytí na nejbližší dobu (Q)"
      aria-label="Quantize"
      aria-pressed={app.quantize}
    >
      <Magnet size={14} />
    </button>
    <button class="g icon" onclick={() => app.undo()} title="Vrátit (⌘Z)" aria-label="Vrátit"><Undo2 size={14} /></button>
  </div>
</div>

{#if menu}
  {@const c = hotCue(cues, menu.slot)}
  {#if c}
    <div class="menu" style:left="{menu.x}px" style:top="{menu.y - 8}px" role="dialog" aria-label="Hot cue {SLOT_LETTERS[menu.slot - 1]}">
      <div class="head">
        <span class="letter" style:background={cssColor(c)}>{SLOT_LETTERS[menu.slot - 1]}</span>
        <span class="readout">{cueTime(c.timeMs)}{c.loopEndMs ? ` – ${cueTime(c.loopEndMs)}` : ""}</span>
      </div>
      <div class="swatches">
        <button class="sw default" class:on={c.hotColor === 0} onclick={() => setColor(0)} title="Výchozí barva"></button>
        {#each app.palette as p}
          <button
            class="sw"
            class:on={c.hotColor === p.code}
            style:background="rgb({p.color[0]} {p.color[1]} {p.color[2]})"
            onclick={() => setColor(p.code)}
            aria-label="Barva {p.code}"
          ></button>
        {/each}
      </div>
      <input
        type="text"
        placeholder="Název, např. Drop"
        bind:value={comment}
        onchange={() => update({ comment })}
        onkeydown={(e) => e.key === "Enter" && (update({ comment }), (menu = null))}
      />
      <div class="row">
        <span class="lbl">Smyčka</span>
        {#each [null, 4, 8, 16, 32] as n}
          <button
            class="btn small"
            class:on={n == null ? !c.loopEndMs : c.loopBeats?.[0] === n}
            onclick={() => setLoop(n)}
          >{n ?? "Ne"}</button>
        {/each}
      </div>
      <div class="row">
        <button class="btn small" onclick={() => update({ timeMs: here() })}>Přesunout sem</button>
        <button class="btn small danger" onclick={() => (remove(menu!.slot), (menu = null))}><Trash size={12} /> Smazat</button>
      </div>
    </div>
  {/if}
{/if}

<style>
  .pads {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .pad {
    position: relative;
    flex: 1 1 0;
    min-width: 0;
    height: 36px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 8px 0 6px;
    border: 0;
    border-radius: 8px;
    background: var(--surface-1);
    text-align: left;
  }
  .pad:hover {
    background: color-mix(in srgb, var(--surface-1) 70%, var(--surface-3));
  }
  .letter {
    width: 24px;
    height: 24px;
    flex: none;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    font-size: 11px;
    background: var(--surface-3);
    color: var(--text-3);
  }
  .pad.set .letter {
    background: var(--c);
    color: #fff;
    text-shadow: 0 1px 1px rgb(0 0 0 / 0.25);
  }
  .txt {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.1;
  }
  .txt .readout {
    font-size: 14px;
  }
  .pad:not(.set) .txt .readout {
    color: var(--text-3);
  }
  .cm {
    font-size: 11px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tag {
    position: absolute;
    right: 6px;
    top: 4px;
    font-size: 10px;
    color: var(--text-2);
  }
  .group {
    display: flex;
    align-items: center;
    gap: 2px;
    flex: none;
    height: 36px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--surface-1);
  }
  .g {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 3px;
    min-width: 28px;
    height: 28px;
    padding: 0 6px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--text-2);
    font-size: 11.5px;
    font-weight: 600;
  }
  .g:hover {
    color: var(--text);
    background: var(--surface-2);
  }
  .g:disabled {
    opacity: 0.4;
    pointer-events: none;
  }
  .g.on {
    color: var(--accent);
  }
  .g.armed {
    background: var(--accent);
    color: var(--on-accent);
  }
  .btn.small {
    height: 26px;
    padding: 0 9px;
    font-size: 12px;
  }
  .btn.on {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .btn.danger {
    background: var(--danger-soft);
    color: #f2a3a5;
  }
  .btn.danger :global(svg) {
    color: #f2a3a5;
  }
  .n {
    font-family: var(--font-readout);
    font-size: 13px;
    color: var(--text-3);
  }
  .menu {
    position: fixed;
    transform: translateY(-100%);
    width: 236px;
    padding: 10px;
    border-radius: 12px;
    background: var(--surface-1);
    box-shadow: var(--shadow-pop);
    display: flex;
    flex-direction: column;
    gap: 10px;
    z-index: 30;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .head .letter {
    width: 18px;
    height: 18px;
    border-radius: 5px;
    text-align: center;
    line-height: 18px;
    font-weight: 700;
    font-size: 11px;
    color: #fff;
  }
  .head .readout {
    font-size: 16px;
  }
  .swatches {
    display: grid;
    grid-template-columns: repeat(9, 1fr);
    gap: 5px;
  }
  .sw {
    width: 20px;
    height: 20px;
    border-radius: 4px;
    border: 2px solid transparent;
    padding: 0;
  }
  .sw.on {
    border-color: var(--text);
  }
  .sw.default {
    background: linear-gradient(135deg, #28e214 50%, #ff8c00 50%);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .lbl {
    font-size: 11px;
    color: var(--text-2);
    margin-right: 4px;
  }
  input {
    width: 100%;
  }
</style>
