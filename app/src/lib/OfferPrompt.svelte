<script lang="ts">
  import { Laptop } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { peerApi } from "./api";
  import { size } from "./format";
  import { t } from "./i18n.svelte";
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
    const names = s.playlists.slice(0, 3).map((n) => t("peer.offer.quoted", { name: n }));
    const more = s.playlists.length - names.length;
    if (!names.length) return t("peer.offer.whatTracks", { count: s.tracks });
    return more > 0 ? t("peer.offer.whatMore", { names: names.join(", "), count: more }) : names.join(", ");
  });

  const saveTo = $derived(t("peer.offer.saveTo").split("{dir}"));

  async function answer(accept: boolean) {
    const req = r.request;
    app.offerRequest = null;
    await peerApi.answerOffer(req, accept);
  }
</script>

<SheetFrame id="offer-title" icon={Laptop} title={t("peer.offer.title", { from: r.from, what })} sub={t("peer.title")} width={520} onclose={() => answer(false)}>
  <div class="body">
    <div class="facts">
      <div><span class="n readout">{s.tracks}</span>{t("peer.offer.tracks", { count: s.tracks })}</div>
      <div>
        <span class="n readout">{s.newTracks}</span>{t("peer.offer.newTracks", { count: s.newTracks })}
        {#if s.newTracks}<span class="quiet">· {size(s.newBytes)}</span>{/if}
      </div>
      {#if s.playlists.length}
        <div>
          <span class="n readout">{s.playlists.length}</span>{s.sets === s.playlists.length
            ? t("peer.offer.sets", { count: s.sets })
            : t("peer.offer.playlists", { count: s.playlists.length })}
        </div>
      {/if}
    </div>
    {#if s.newTracks}
      <p>{saveTo[0]}<strong>{musicDir}</strong>{saveTo[1]}</p>
    {:else}
      <p>{t("peer.offer.allHere")}</p>
    {/if}
    <p class="quiet">{t("peer.offer.newerWins", { from: r.from })}</p>
  </div>

  {#snippet foot()}
    <button class="btn ghost lg" onclick={() => answer(false)}>{t("peer.decline")}</button>
    <button class="btn primary lg" onclick={() => answer(true)}>{t("peer.offer.accept")}</button>
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
