<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { emitTo, listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { CircleCheck, Download, FileOutput, Folder, Laptop, Route, Settings, ShieldAlert, SlidersHorizontal, Usb } from "@lucide/svelte";
  import type { Component } from "svelte";
  import { onMount } from "svelte";
  import { api, peerApi, type PeerSettings } from "./lib/api";
  import PlayerSettingsEditor from "./lib/PlayerSettingsEditor.svelte";
  import { DUPLICATES, ENGINE, PLAN_CUES, PLAN_CURVES, PLAN_LEVELS, prefs, type Prefs } from "./lib/prefs.svelte";
  import SheetSwitch from "./lib/SheetSwitch.svelte";

  const SECTIONS: { id: string; title: string; icon: Component<{ size?: number }> }[] = [
    { id: "general", title: "Obecné", icon: Settings },
    { id: "player", title: "Přehrávač", icon: SlidersHorizontal },
    { id: "stick", title: "Flashka", icon: Usb },
    { id: "planner", title: "Plánovač setu", icon: Route },
    { id: "export", title: "Export na disk", icon: FileOutput },
    { id: "peer", title: "Sync mezi Macy", icon: Laptop },
  ];
  const known = (s: string | null) => (SECTIONS.some((x) => x.id === s) ? s! : "general");

  let section = $state(known(new URLSearchParams(location.search).get("section")));
  let version = $state("");
  let fullDisk = $state<boolean | null>(null);
  let peer = $state<PeerSettings | null>(null);
  let error = $state("");

  const p = $derived(prefs.value);
  const set = <K extends keyof Prefs>(group: K, change: Partial<Prefs[K]>) => prefs.set(group, change).catch((e) => (error = String(e)));
  const home = (path: string) => path.replace(/^\/Users\/[^/]+/, "~");

  onMount(() => {
    prefs.init().catch((e) => (error = String(e)));
    getVersion().then((v) => (version = v));
    peerApi.settings().then((v) => (peer = v), (e) => (error = String(e)));
    const checkDisk = () => api.fullDiskAccess().then((v) => (fullDisk = v));
    checkDisk();
    // Switched on in System Settings while this window is open.
    const poll = setInterval(checkDisk, 2000);
    const off = listen<string>("settings-section", (e) => (section = known(e.payload)));
    return () => {
      clearInterval(poll);
      off.then((f) => f());
    };
  });

  async function musicDir(path: string | null) {
    error = "";
    try {
      peer = await peerApi.setMusicDir(path);
    } catch (e) {
      error = String(e);
    }
  }

  async function chooseMusicDir() {
    const d = await open({ directory: true, defaultPath: peer?.musicDir });
    if (typeof d === "string") await musicDir(d);
  }

  async function autoMirror(on: boolean) {
    error = "";
    try {
      peer = await peerApi.setAutoMirror(on);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<div class="window">
  <nav data-tauri-drag-region>
    <div class="drag" data-tauri-drag-region></div>
    {#each SECTIONS as s (s.id)}
      <button class:on={section === s.id} onclick={() => (section = s.id)}>
        <s.icon size={16} />
        <span>{s.title}</span>
      </button>
    {/each}
  </nav>

  <main>
    <header data-tauri-drag-region>
      <h1 data-tauri-drag-region>{SECTIONS.find((s) => s.id === section)?.title}</h1>
    </header>

    {#if error}<p class="err">{error}</p>{/if}

    {#if section === "general"}
      <div class="pane">
        <section>
          <div class="eyebrow">Pomocníci</div>
          <SheetSwitch
            checked={p.general.helpers}
            onchange={(on) => set("general", { helpers: on })}
            label="Smart Cue, AI analýza cue a plánovač setu"
            desc="Vypnuté jako za starých časů: cue i pořadí setu děláš sám. Cue, které už ve skladbách jsou, zůstanou; analýza a kontrola flashky běží dál."
          />
        </section>
        <section>
          <div class="eyebrow">Verze</div>
          <div class="line">
            <span>Stickwise <strong class="readout">{version}</strong></span>
            <button class="btn" onclick={() => emitTo("main", "open-updates")}><Download size={15} />Zkontrolovat aktualizace</button>
          </div>
          <p class="hint">Nové verze hledá Stickwise sám při startu a každé 4 hodiny.</p>
        </section>
        <section>
          <div class="eyebrow">Přístup k disku</div>
          <div class="line">
            {#if fullDisk}
              <span class="ok"><CircleCheck size={16} />Plný přístup k disku je povolený.</span>
            {:else if fullDisk === false}
              <span class="warn"><ShieldAlert size={16} />Plný přístup k disku není povolený.</span>
              <button class="btn" onclick={() => api.openFullDiskSettings()}>Otevřít Nastavení systému</button>
            {/if}
          </div>
          <p class="hint">
            Bez něj se macOS ptá na hudbu na externích discích, flashky a knihovnu rekordboxu pořád dokola. Po zapnutí
            Stickwise restartuj.
          </p>
        </section>
      </div>
    {:else if section === "player"}
      <p class="lead">Tvoje My Settings pro CDJ/XDJ a mixer. Zapíšou se na flashku při synchronizaci a formátování.</p>
      <PlayerSettingsEditor stick={null} />
    {:else if section === "stick"}
      <div class="pane">
        <p class="lead">Výchozí volby pro synchronizaci a formátování flashky. U jednotlivé flashky je jde přepnout jen pro ten jednou.</p>
        <section class="opts">
          <SheetSwitch
            checked={p.stick.flat}
            onchange={(on) => set("stick", { flat: on })}
            label="Všechny playlisty rovnou nahoru, bez složek"
            desc="Na přehrávači se nemusíš proklikávat složkami. Stejné názvy dostanou do závorky jméno složky."
          />
          <SheetSwitch
            checked={p.stick.writeSettings}
            onchange={(on) => set("stick", { writeSettings: on })}
            label="Zapisovat moje nastavení přehrávače (My Settings)"
            desc="Při synchronizaci i formátování. Nastavení je v sekci Přehrávač."
          />
          <SheetSwitch
            checked={!!p.stick.engine}
            onchange={(on) => set("stick", { engine: on ? "3.0.1" : null })}
            label="Hybridní flashka i pro Denon"
            desc="Engine DJ knihovna nad stejnými soubory, hudba na flashce jen jednou. Flashka, která už Denon má, si svou volbu pamatuje."
          />
          {#if p.stick.engine}
            <div class="sub">
              <div class="seg" role="radiogroup" aria-label="Nejstarší Denon">
                {#each ENGINE as o (o.schema)}
                  <button role="radio" aria-checked={p.stick.engine === o.schema} class:on={p.stick.engine === o.schema} onclick={() => set("stick", { engine: o.schema })}>
                    {o.label}<span class="n">{o.note}</span>
                  </button>
                {/each}
              </div>
              <p class="hint">Nejstarší firmware Denonu, na kterém budeš hrát; novější ho přečtou taky.</p>
            </div>
          {/if}
        </section>
        <footer><button class="btn ghost" onclick={() => prefs.reset("stick")}>Obnovit výchozí</button></footer>
      </div>
    {:else if section === "planner"}
      <div class="pane">
        <p class="lead">S čím plánovač setu začne. V plánovači je jde změnit pro jeden plán.</p>
        <section class="rows">
          <div class="row">
            <span class="label">Pořadí</span>
            <div class="seg">
              <button class:on={p.planner.keepOrder} onclick={() => set("planner", { keepOrder: true })}>Moje</button>
              <button class:on={!p.planner.keepOrder} onclick={() => set("planner", { keepOrder: false })}>Navrhnout</button>
            </div>
          </div>
          <div class="row">
            <span class="label">Energie</span>
            <div class="seg">
              {#each PLAN_CURVES as [v, label] (v)}
                <button class:on={p.planner.curve === v} onclick={() => set("planner", { curve: v })}>{label}</button>
              {/each}
            </div>
          </div>
          <div class="row">
            <span class="label">Úroveň</span>
            <div class="seg">
              {#each PLAN_LEVELS as [v, label, hint] (v)}
                <button class:on={p.planner.level === v} onclick={() => set("planner", { level: v })} title={hint}>{label}</button>
              {/each}
            </div>
          </div>
          <div class="row">
            <span class="label">Cue přechodů</span>
            <div class="seg">
              {#each PLAN_CUES as [v, label, hint] (v)}
                <button class:on={p.planner.cueMode === v} onclick={() => set("planner", { cueMode: v })} title={hint}>{label}</button>
              {/each}
            </div>
          </div>
          <div class="row">
            <span class="label">Délka setu <span class="desc">když plánovač vybírá skladby</span></span>
            <div class="len">
              <input type="range" min="15" max="240" step="5" value={p.planner.minutes} oninput={(e) => set("planner", { minutes: Number(e.currentTarget.value) })} />
              <span class="readout">{p.planner.minutes} min</span>
            </div>
          </div>
        </section>
        <footer><button class="btn ghost" onclick={() => prefs.reset("planner")}>Obnovit výchozí</button></footer>
      </div>
    {:else if section === "export"}
      <div class="pane">
        <p class="lead">Jak export z flashky na disk pojmenuje a uloží soubory. Při exportu je jde změnit pro ten jednou.</p>
        <section class="opts">
          <SheetSwitch checked={p.export.numbered} onchange={(on) => set("export", { numbered: on })} label="Očíslovat podle pořadí (01 – …)" />
          <SheetSwitch checked={p.export.artistTitle} onchange={(on) => set("export", { artistTitle: on })} label="Pojmenovat Interpret – Název" />
          <SheetSwitch checked={p.export.m3u} onchange={(on) => set("export", { m3u: on })} label="Vytvořit .m3u8" />
          <SheetSwitch checked={p.export.history} onchange={(on) => set("export", { history: on })} label="Zahrnout historii" desc="Playlisty historie hraní z přehrávače." />
        </section>
        <section class="rows">
          <div class="row">
            <span class="label">Skladba ve více playlistech</span>
            <div class="seg" role="radiogroup" aria-label="Duplicity">
              {#each DUPLICATES as [v, label] (v)}
                <button role="radio" aria-checked={p.export.duplicates === v} class:on={p.export.duplicates === v} onclick={() => set("export", { duplicates: v })}>{label}</button>
              {/each}
            </div>
          </div>
        </section>
        <footer><button class="btn ghost" onclick={() => prefs.reset("export")}>Obnovit výchozí</button></footer>
      </div>
    {:else if section === "peer"}
      <div class="pane">
        <p class="lead">Párování, posílání a zrcadlení je v nástrojích (⌘K) → Sync mezi Macy.</p>
        <section>
          <div class="eyebrow">Hudba z jiných Maců se ukládá do</div>
          <div class="line">
            <span class="path"><Folder size={15} />{peer ? home(peer.musicDir) : "…"}</span>
            <span class="grow"></span>
            {#if peer && !peer.defaultMusicDir}
              <button class="btn ghost" onclick={() => musicDir(null)}>Výchozí</button>
            {/if}
            <button class="btn" onclick={chooseMusicDir} disabled={!peer}>Změnit…</button>
          </div>
          <p class="hint">Uvnitř podle interpreta a alba. Soubory, které už tu jsou, se nikdy nepřepíšou.</p>
        </section>
        {#if peer}
          <section>
            <SheetSwitch
              checked={peer.autoMirror}
              onchange={autoMirror}
              label="Zrcadlit samo, když je spárovaný Mac v síti"
              desc="Bez ptaní jen tehdy, když se nemá nic mazat; jinak ukáže, co se změní. Nejvýš jednou za 10 minut."
            />
          </section>
        {/if}
      </div>
    {/if}
  </main>
</div>

<style>
  .window {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
    height: 100%;
    background: var(--surface-1);
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 12px 16px;
    background: var(--surface-2);
    box-shadow: inset -1px 0 0 var(--hairline);
  }
  .drag {
    /* Room for the traffic lights. */
    height: 52px;
    flex: none;
  }
  nav button {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-control);
    background: none;
    color: var(--text-2);
    font-size: 13.5px;
    text-align: left;
    cursor: pointer;
  }
  nav button:hover {
    background: var(--surface-3);
    color: var(--text);
  }
  nav button.on {
    background: var(--surface-3);
    color: var(--text);
    font-weight: 600;
  }
  nav button :global(svg) {
    flex: none;
    color: var(--text-3);
  }
  nav button.on :global(svg) {
    color: var(--accent);
  }
  main {
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  header {
    flex: none;
    height: 52px;
    display: flex;
    align-items: center;
    padding: 0 28px;
    box-shadow: inset 0 -1px 0 var(--hairline);
  }
  h1 {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
  }
  .pane {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 20px 28px 24px;
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  .lead {
    margin: 0;
    color: var(--text-2);
    font-size: 13px;
    line-height: 1.5;
  }
  main > .lead {
    padding: 16px 28px 0;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .opts {
    gap: 16px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 34px;
  }
  .line > span:first-child {
    font-size: 13.5px;
  }
  .hint {
    margin: 0;
    color: var(--text-3);
    font-size: 12.5px;
    line-height: 1.5;
  }
  .ok,
  .warn,
  .path {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }
  .ok {
    color: var(--success);
  }
  .warn {
    color: var(--warning);
  }
  .path {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .grow {
    flex: 1;
  }
  .sub {
    margin-left: 46px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .seg .n {
    margin-left: 6px;
    color: var(--text-3);
    font-weight: 400;
  }
  .rows {
    gap: 0;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    min-height: 54px;
    box-shadow: inset 0 -1px 0 var(--hairline);
  }
  .row:last-child {
    box-shadow: none;
  }
  .label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 13.5px;
    font-weight: 500;
  }
  .desc {
    font-size: 12px;
    font-weight: 400;
    color: var(--text-3);
  }
  .len {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .len input {
    width: 200px;
  }
  .len .readout {
    min-width: 56px;
    text-align: right;
    font-size: 15px;
  }
  footer {
    display: flex;
    margin-top: auto;
  }
  .err {
    margin: 12px 28px 0;
    color: var(--danger);
  }
</style>
