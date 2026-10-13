<script lang="ts">
  import { plural } from "./format";
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

<div class="dim"></div>
<div class="sheet" role="dialog" aria-modal="true" aria-labelledby="write-title">
  <div class="body">
    <h2 id="write-title">Zapsat cue na {app.data?.info.name}</h2>
    <p>
      Upravené cue u {n} {plural(n, "skladby", "skladeb", "skladeb")} se zapíšou do souborů s analýzou na flashce.
      Databáze ani hudba se nemění.
    </p>
    <ul>
      {#each names as name}<li>{name}</li>{/each}
      {#if n > names.length}<li class="more">a {n - names.length} dalších</li>{/if}
    </ul>
    <p class="sub">
      Původní soubory se nejdřív zazálohují do ~/RB-EXPORT/backups. Každý soubor se zapíše vedle originálu,
      ověří a teprve pak vymění, takže vytažení flashky uprostřed nic nerozbije.
    </p>
    <p class="sub">
      CDJ-3000X, XDJ-AZ a OPUS-QUAD čtou cue z OneLibrary, tu zatím neumíme zapsat. Tam se změny projeví až po
      exportu. NXS2, CDJ-3000 a Denon je uvidí hned.
    </p>
  </div>
  <div class="foot">
    <button class="btn" onclick={() => (app.writeOpen = false)} disabled={busy}>Zrušit</button>
    <button class="btn primary" onclick={write} disabled={busy || n === 0}>
      {busy ? "Zapisuji…" : `Zapsat ${n} ${plural(n, "skladbu", "skladby", "skladeb")}`}
    </button>
  </div>
</div>

<style>
  .dim {
    position: fixed;
    inset: 0;
    background: rgb(10 11 14 / 0.55);
    z-index: 20;
  }
  .sheet {
    position: fixed;
    top: 36px;
    left: 50%;
    transform: translateX(-50%);
    width: 500px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: 0 24px 60px rgb(0 0 0 / 0.5);
    z-index: 21;
  }
  .body {
    padding: 20px;
  }
  h2 {
    margin: 0 0 10px;
    font-size: 15px;
    font-weight: 600;
  }
  p {
    margin: 0 0 10px;
    line-height: 1.45;
  }
  .sub {
    font-size: 12px;
    color: var(--text-2);
  }
  ul {
    margin: 0 0 12px;
    padding: 8px 12px;
    list-style: none;
    background: var(--booth);
    border: 1px solid var(--line);
    border-radius: 6px;
    font-size: 12px;
  }
  li {
    padding: 2px 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .more {
    color: var(--text-2);
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--line);
  }
</style>
