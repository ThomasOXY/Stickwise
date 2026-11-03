<script lang="ts">
  import { Info, PenLine, ShieldCheck } from "@lucide/svelte";
  import { plural } from "./format";
  import SheetFrame from "./SheetFrame.svelte";
  import { app } from "./state.svelte";

  let busy = $state(false);
  const n = $derived(app.pending.size);
  const names = $derived(
    [...app.pending]
      .map((id) => app.trackById.get(id))
      .filter((t) => !!t)
      .slice(0, 6)
      .map((t) => `${t!.artist ? t!.artist + " – " : ""}${t!.title}`),
  );

  async function write() {
    busy = true;
    await app.writeCuesToStick();
    busy = false;
    app.writeOpen = false;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !busy && (app.writeOpen = false)} />

<SheetFrame
  id="write-title"
  icon={PenLine}
  title="Zapsat cue na {app.data?.info.name}"
  sub="Upravené cue u {n} {plural(n, 'skladby', 'skladeb', 'skladeb')} se zapíšou do souborů s analýzou na flashce. Databáze ani hudba se nemění."
  width={560}
  onclose={busy ? undefined : () => (app.writeOpen = false)}
>
  <div class="body">
    {#if n}
      <ul>
        {#each names as name}<li>{name}</li>{/each}
        {#if n > names.length}<li class="more">a {n - names.length} dalších</li>{/if}
      </ul>
    {/if}
    <p class="note">
      <ShieldCheck size={15} class="ok" />
      <span>
        Původní soubory se nejdřív zazálohují do ~/Stickwise/backups. Každý soubor se zapíše vedle originálu, ověří a
        teprve pak vymění, takže vytažení flashky uprostřed nic nerozbije.
      </span>
    </p>
    <p class="note">
      <Info size={15} />
      <span>
        CDJ-3000X, XDJ-AZ a OPUS-QUAD čtou cue z OneLibrary, tu zatím neumíme zapsat. Tam se změny projeví až po exportu.
        NXS2, CDJ-3000 a Denon je uvidí hned.
      </span>
    </p>
  </div>

  {#snippet foot()}
    <button class="btn ghost lg" onclick={() => (app.writeOpen = false)} disabled={busy}>Zrušit</button>
    <button class="btn primary lg" onclick={write} disabled={busy || n === 0}>
      <PenLine size={16} />
      {busy ? "Zapisuji…" : `Zapsat ${n} ${plural(n, "skladbu", "skladby", "skladeb")}`}
    </button>
  {/snippet}
</SheetFrame>

<style>
  .body {
    padding: 20px 26px 22px;
    overflow-y: auto;
  }
  ul {
    margin: 0 0 16px;
    padding: 8px 14px;
    list-style: none;
    border-radius: var(--r-panel);
    background: var(--surface-2);
    font-size: 13px;
  }
  li {
    padding: 5px 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .more {
    color: var(--text-3);
  }
  .note {
    display: flex;
    gap: 10px;
    margin: 0 0 10px;
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text-2);
  }
  .note :global(svg) {
    flex: none;
    margin-top: 2px;
    color: var(--text-3);
  }
  .note :global(svg.ok) {
    color: var(--success);
  }
</style>
