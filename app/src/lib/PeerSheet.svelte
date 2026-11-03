<script lang="ts">
  import { CircleCheck, Folder, Laptop, LoaderCircle, Wifi } from "@lucide/svelte";
  import { listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { peerApi, type PeerSettings } from "./api";
  import { plural } from "./format";
  import SheetFrame from "./SheetFrame.svelte";
  import { app } from "./state.svelte";

  let s = $state<PeerSettings | null>(null);
  let error = $state("");

  const home = (p: string) => p.replace(/^\/Users\/[^/]+/, "~");
  const ready = $derived(!!s && s.fingerprinted >= s.total);
  const pct = $derived(s && s.total ? (s.fingerprinted / s.total) * 100 : 0);

  onMount(() => {
    peerApi.settings().then((v) => (s = v), (e) => (error = String(e)));
    const off = listen<{ fingerprinted: number; total: number }>("peer-fingerprints", (e) => {
      if (s) s = { ...s, ...e.payload };
    });
    return () => off.then((f) => f());
  });

  async function chooseDir() {
    const d = await open({ directory: true, defaultPath: s?.musicDir });
    if (typeof d !== "string") return;
    await save(d);
  }

  async function save(path: string | null) {
    error = "";
    try {
      s = await peerApi.setMusicDir(path);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<SheetFrame id="peer-title" icon={Laptop} title="Sync mezi Macy" sub="Knihovna s dalšími Stickwise ve stejné síti" width={620} onclose={() => (app.peerOpen = false)}>
  <div class="body">
    <section>
      <div class="eyebrow">Hudba z jiných Maců se ukládá do</div>
      <div class="dest">
        <Folder size={15} />
        <span class="path">{s ? home(s.musicDir) : "…"}</span>
        {#if s && !s.defaultMusicDir}
          <button class="btn ghost" onclick={() => save(null)}>Výchozí</button>
        {/if}
        <button class="btn" onclick={chooseDir} disabled={!s}>Změnit…</button>
      </div>
      <p class="hint">Uvnitř podle interpreta a alba. Soubory, které už tu jsou, se nikdy nepřepíšou.</p>
    </section>

    <section>
      <div class="eyebrow">Příprava knihovny</div>
      {#if s}
        {#if ready}
          <p class="ok"><CircleCheck size={16} />Všech {s.total} {plural(s.total, "skladba má", "skladby mají", "skladeb má")} otisk, knihovna je připravená.</p>
        {:else}
          <div class="progress">
            <div class="bar"><span style:width="{pct}%"></span></div>
            <span class="readout">{s.fingerprinted} / {s.total}</span>
          </div>
          <p class="hint">
            <LoaderCircle size={13} class="spin" />Počítají se otisky skladeb. Podle nich druhý Mac pozná stejnou skladbu, i když leží
            jinde. Běží na pozadí, skladby na odpojeném disku přijdou na řadu po připojení.
          </p>
        {/if}
      {/if}
    </section>

    <section>
      <div class="eyebrow">Macy poblíž</div>
      <div class="empty"><Wifi size={18} />Hledání Maců ve stejné síti přijde v další verzi.</div>
    </section>

    {#if error}<p class="err">{error}</p>{/if}
  </div>

  {#snippet foot()}
    <button class="btn primary lg" onclick={() => (app.peerOpen = false)}>Hotovo</button>
  {/snippet}
</SheetFrame>

<style>
  .body {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 20px 26px 24px;
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .dest {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 44px;
    padding: 0 6px 0 14px;
    border-radius: var(--r-panel);
    background: var(--surface-2);
    color: var(--accent);
  }
  .path {
    flex: 1;
    min-width: 0;
    color: var(--text);
    font-size: 13.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .hint {
    margin: 0;
    display: flex;
    align-items: flex-start;
    gap: 6px;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--text-3);
  }
  .hint :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 12px;
    color: var(--text-2);
    font-size: 14px;
  }
  .bar {
    flex: 1;
    height: 6px;
    border-radius: 3px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar span {
    display: block;
    height: 100%;
    background: var(--accent);
    border-radius: inherit;
    transition: width 0.3s linear;
  }
  .ok,
  .err {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }
  .ok {
    color: var(--success);
  }
  .err {
    color: var(--danger);
  }
  .empty {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px;
    border-radius: var(--r-panel);
    background: var(--surface-2);
    color: var(--text-3);
    font-size: 13.5px;
  }
</style>
