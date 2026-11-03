<script lang="ts">
  import { Laptop } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { peerApi } from "./api";
  import { plural, size } from "./format";
  import SheetFrame from "./SheetFrame.svelte";
  import { app } from "./state.svelte";

  /** Another Mac offers playlists and sets: shown wherever the DJ is in the app. */
  const r = $derived(app.offerRequest!);
  const s = $derived(r.summary);
  let musicDir = $state("");

  onMount(() => {
    peerApi.settings().then((v) => (musicDir = v.musicDir.replace(/^\/Users\/[^/]+/, "~")));
  });

  const what = $derived.by(() => {
    const names = s.playlists.slice(0, 3).map((n) => `„${n}“`);
    const more = s.playlists.length - names.length;
    if (!names.length) return `${s.tracks} ${plural(s.tracks, "skladbu", "skladby", "skladeb")}`;
    return more > 0 ? `${names.join(", ")} a ${more} další` : names.join(", ");
  });

  async function answer(accept: boolean) {
    const req = r.request;
    app.offerRequest = null;
    await peerApi.answerOffer(req, accept);
  }
</script>

<SheetFrame id="offer-title" icon={Laptop} title="{r.from} posílá {what}" sub="Sync mezi Macy" width={520} onclose={() => answer(false)}>
  <div class="body">
    <div class="facts">
      <div><span class="n readout">{s.tracks}</span>{plural(s.tracks, "skladba", "skladby", "skladeb")}</div>
      <div>
        <span class="n readout">{s.newTracks}</span>{plural(s.newTracks, "nová", "nové", "nových")}
        {#if s.newTracks}<span class="quiet">· {size(s.newBytes)}</span>{/if}
      </div>
      {#if s.playlists.length}
        <div>
          <span class="n readout">{s.playlists.length}</span>{s.sets === s.playlists.length
            ? plural(s.sets, "set", "sety", "setů")
            : plural(s.playlists.length, "playlist", "playlisty", "playlistů")}
        </div>
      {/if}
    </div>
    {#if s.newTracks}
      <p>Nové skladby se uloží do <strong>{musicDir}</strong>. Soubory, které už tu jsou, se nepřepíšou.</p>
    {:else}
      <p>Všechny skladby už tu jsou, přijdou jen playlisty, cue a gridy.</p>
    {/if}
    <p class="quiet">Kde je úprava novější na {r.from}, přepíše tu zdejší. Přepsané cue jdou vrátit.</p>
  </div>

  {#snippet foot()}
    <button class="btn ghost lg" onclick={() => answer(false)}>Odmítnout</button>
    <button class="btn primary lg" onclick={() => answer(true)}>Přijmout</button>
  {/snippet}
</SheetFrame>

<style>
  .body {
    padding: 22px 26px 24px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .facts {
    display: flex;
    gap: 22px;
    padding: 14px 16px;
    border-radius: var(--r-panel);
    background: var(--surface-2);
    font-size: 14px;
    color: var(--text-2);
  }
  .n {
    margin-right: 6px;
    font-size: 22px;
    color: var(--text);
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
