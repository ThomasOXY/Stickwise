<script lang="ts">
  import { Laptop } from "@lucide/svelte";
  import { peerApi } from "./api";
  import SheetFrame from "./SheetFrame.svelte";
  import { app } from "./state.svelte";

  /** Another Mac asks to pair: shown wherever the DJ is in the app. */
  const r = $derived(app.pairRequest!);

  async function answer(accept: boolean) {
    const req = r.request;
    app.pairRequest = null;
    await peerApi.answer(req, accept);
    if (accept) app.notice = `Spárováno s ${r.peer.name}.`;
  }
</script>

<SheetFrame id="pair-title" icon={Laptop} title="{r.peer.name} se chce spárovat" sub="Sync mezi Macy" width={480} onclose={() => answer(false)}>
  <div class="body">
    <div class="code readout">{r.code}</div>
    <p>
      Zkontroluj, že <strong>{r.peer.name}</strong> ukazuje stejný kód. Když se kódy liší, odmítni: někdo jiný se tváří jako
      tvůj Mac.
    </p>
    <p class="quiet">Spárované Macy si mezi sebou můžou posílat knihovnu. Spárování jde kdykoli zrušit.</p>
  </div>

  {#snippet foot()}
    <button class="btn ghost lg" onclick={() => answer(false)}>Odmítnout</button>
    <button class="btn primary lg" onclick={() => answer(true)}>Kód sedí, spárovat</button>
  {/snippet}
</SheetFrame>

<style>
  .body {
    padding: 22px 26px 24px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .code {
    align-self: center;
    padding: 12px 22px;
    border-radius: var(--r-panel);
    background: var(--surface-2);
    color: var(--accent);
    font-size: 34px;
    letter-spacing: 0.08em;
  }
  p {
    margin: 0;
    font-size: 14px;
    line-height: 1.5;
    color: var(--text-2);
  }
  strong {
    color: var(--text);
  }
  .quiet {
    font-size: 12.5px;
    color: var(--text-3);
  }
</style>
