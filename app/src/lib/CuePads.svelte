<script lang="ts">
  import { Magnet, Trash, Undo2 } from "@lucide/svelte";
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
    if (el?.tagName === "INPUT" || app.exportOpen || app.writeOpen || !app.detail) return;
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
      {#if c}
        <span class="txt">
          <span class="readout">{cueTime(c.timeMs)}</span>
          {#if c.loopEndMs}<span class="tag">{c.loopBeats ? `${c.loopBeats[0]}` : "∞"}⟲</span>{/if}
          {#if c.comment}<span class="cm">{c.comment}</span>{/if}
        </span>
      {/if}
    </button>
  {/each}

  <span class="gap"></span>

  <button class="btn small" onclick={addMemory} title="Memory cue na aktuální pozici (M)">
    Memory{#if memoryCount}<span class="readout n">{memoryCount}</span>{/if}
  </button>
  <div class="group" role="group" aria-label="Smyčka">
    <button class="btn small" class:armed={loopIn != null} onclick={() => (loopIn = here())} title="Začátek smyčky">In</button>
    <button class="btn small" onclick={loopOut} disabled={loopIn == null} title="Konec smyčky, uloží memory smyčku">Out</button>
    {#each [4, 8, 16, 32] as n}
      <button class="btn small" onclick={(e) => autoLoop(n, e.shiftKey)} title="Smyčka {n} dob od aktuální pozice. Se Shiftem jako hot cue.">{n}</button>
    {/each}
  </div>
  <button
    class="btn small icon"
    class:on={app.quantize}
    onclick={() => app.setQuantize(!app.quantize)}
    title="Quantize: cue se přichytí na nejbližší dobu (Q)"
    aria-pressed={app.quantize}
  >
    <Magnet size={13} />
  </button>
  <button class="btn small icon" onclick={() => app.undo()} title="Vrátit (⌘Z)"><Undo2 size={13} /></button>
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
    gap: 6px;
  }
  .pad {
    position: relative;
    width: 92px;
    height: 30px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 8px 0 0;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--booth);
    overflow: hidden;
    text-align: left;
  }
  .pad:hover {
    border-color: var(--text-3);
  }
  .pad .letter {
    width: 20px;
    align-self: stretch;
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 700;
    font-size: 11px;
    color: var(--text-3);
    border-right: 1px solid var(--line);
  }
  .pad.set {
    border-color: color-mix(in srgb, var(--c) 55%, transparent);
    background: color-mix(in srgb, var(--c) 14%, var(--booth));
  }
  .pad.set .letter {
    background: var(--c);
    color: var(--booth);
    border-right: 0;
  }
  .txt {
    display: flex;
    flex-direction: column;
    min-width: 0;
    line-height: 1.05;
  }
  .txt .readout {
    font-size: 14px;
  }
  .cm {
    font-size: 10px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tag {
    position: absolute;
    right: 5px;
    top: 3px;
    font-size: 10px;
    color: var(--text-2);
  }
  .gap {
    flex: 1;
  }
  .group {
    display: flex;
    gap: 3px;
  }
  .btn.small {
    height: 26px;
    padding: 0 9px;
    font-size: 12px;
  }
  .btn.small.icon {
    width: 26px;
    padding: 0;
  }
  .btn.on {
    border-color: var(--cue);
    color: var(--cue);
  }
  .btn.armed {
    background: var(--cue);
    border-color: var(--cue);
    color: var(--booth);
  }
  .btn.danger {
    color: #f2a3a5;
    background: var(--panel);
    border-color: rgb(229 72 77 / 0.45);
  }
  .n {
    margin-left: 6px;
    font-size: 13px;
    color: var(--text-2);
  }
  .menu {
    position: fixed;
    transform: translateY(-100%);
    width: 236px;
    padding: 10px;
    border-radius: 10px;
    border: 1px solid var(--line);
    background: var(--panel);
    box-shadow: 0 16px 40px rgb(0 0 0 / 0.5);
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
    border-radius: 3px;
    text-align: center;
    line-height: 18px;
    font-weight: 700;
    font-size: 11px;
    color: var(--booth);
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
