<script lang="ts">
  import { getVersion } from "@tauri-apps/api/app";
  import { emitTo, listen } from "@tauri-apps/api/event";
  import { open } from "@tauri-apps/plugin-dialog";
  import { CircleAlert, CircleCheck, Download, FileOutput, Folder, Laptop, Route, Settings, ShieldAlert, SlidersHorizontal, Usb } from "@lucide/svelte";
  import type { Component } from "svelte";
  import { onMount } from "svelte";
  import { api, peerApi, toolsApi, type JobEvent, type PeerSettings, type ToolsStatus } from "./lib/api";
  import PlayerSettingsEditor from "./lib/PlayerSettingsEditor.svelte";
  import { i18n, t, type Key, type LangChoice } from "./lib/i18n.svelte";
  import { DUPLICATES, ENGINE, PLAN_CUES, PLAN_CURVES, PLAN_LEVELS, prefs, type Prefs } from "./lib/prefs.svelte";
  import SheetSwitch from "./lib/SheetSwitch.svelte";

  const SECTIONS: { id: string; title: Key; icon: Component<{ size?: number }> }[] = [
    { id: "general", title: "settings.sections.general", icon: Settings },
    { id: "player", title: "settings.sections.player", icon: SlidersHorizontal },
    { id: "stick", title: "settings.sections.stick", icon: Usb },
    { id: "planner", title: "settings.sections.planner", icon: Route },
    { id: "export", title: "settings.sections.export", icon: FileOutput },
    { id: "peer", title: "settings.sections.peer", icon: Laptop },
  ];
  /** Languages by their own name, so anyone finds theirs. */
  const LANGUAGES: [LangChoice, string][] = [
    ["cs", "Čeština"],
    ["en", "English"],
    ["de", "Deutsch"],
  ];
  const known = (s: string | null) => (SECTIONS.some((x) => x.id === s) ? s! : "general");

  let section = $state(known(new URLSearchParams(location.search).get("section")));
  let version = $state("");
  let fullDisk = $state<boolean | null>(null);
  let peer = $state<PeerSettings | null>(null);
  let error = $state("");
  let tools = $state<ToolsStatus | null>(null);
  /** The installation's last line, or why it failed. */
  let toolsLine = $state("");
  let toolsFailed = $state(false);

  const toolsReady = $derived(!!tools && tools.python && tools.cueModel);
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
    refreshTools();
    const offJob = listen<JobEvent>("job", (e) => {
      if (e.payload.phase !== "tools") return;
      if (e.payload.state === "running") {
        toolsLine = e.payload.current;
      } else {
        toolsFailed = e.payload.state === "error";
        toolsLine = e.payload.message;
        refreshTools();
      }
    });
    return () => {
      clearInterval(poll);
      off.then((f) => f());
      offJob.then((f) => f());
    };
  });

  // The language: at start and when picked here or in the other window.
  let applied: LangChoice | null = null;
  $effect(() => {
    const l = prefs.value.general.language;
    if (prefs.loaded && l !== applied) {
      applied = l;
      i18n.apply(l);
    }
  });

  function refreshTools() {
    toolsApi.status().then((v) => (tools = v), (e) => (error = String(e)));
  }

  async function installTools() {
    error = "";
    toolsFailed = false;
    try {
      const again = toolsReady;
      const job = await toolsApi.install(again);
      if (tools) tools = { ...tools, job };
    } catch (e) {
      error = String(e);
    }
  }

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
        <span>{t(s.title)}</span>
      </button>
    {/each}
  </nav>

  <main>
    <header data-tauri-drag-region>
      <h1 data-tauri-drag-region>{t(SECTIONS.find((s) => s.id === section)?.title ?? "settings.sections.general")}</h1>
    </header>

    {#if error}<p class="err">{error}</p>{/if}

    {#if section === "general"}
      <div class="pane">
        <section>
          <div class="eyebrow">{t("settings.language")}</div>
          <div class="seg" role="radiogroup" aria-label={t("settings.language")}>
            <button role="radio" aria-checked={p.general.language === "auto"} class:on={p.general.language === "auto"} onclick={() => set("general", { language: "auto" })}>
              {t("settings.languageAuto")}
            </button>
            {#each LANGUAGES as [v, name] (v)}
              <button role="radio" aria-checked={p.general.language === v} class:on={p.general.language === v} onclick={() => set("general", { language: v })}>{name}</button>
            {/each}
          </div>
        </section>
        <section>
          <div class="eyebrow">{t("settings.helpers")}</div>
          <SheetSwitch
            checked={p.general.helpers}
            onchange={(on) => set("general", { helpers: on })}
            label={t("settings.helpersLabel")}
            desc={t("settings.helpersHint")}
          />
        </section>
        <section>
          <div class="eyebrow">{t("settings.version")}</div>
          <div class="line">
            <span>Stickwise <strong class="readout">{version}</strong></span>
            <button class="btn" onclick={() => emitTo("main", "open-updates")}><Download size={15} />{t("settings.checkUpdates")}</button>
          </div>
          <p class="hint">{t("settings.updatesHint")}</p>
        </section>
        <section>
          <div class="eyebrow">{t("settings.tools")}</div>
          {#snippet tool(label: string, ok: boolean)}
            <div class="line tool">
              <span>{label}</span>
              <span class="grow"></span>
              {#if ok}
                <span class="ok"><CircleCheck size={16} />{t("settings.toolsReady")}</span>
              {:else}
                <span class="warn"><CircleAlert size={16} />{t("settings.toolsMissing")}</span>
              {/if}
            </div>
          {/snippet}
          {#if tools}
            {@render tool(t("settings.toolsPython"), tools.python)}
            {@render tool(t("settings.toolsCueModel"), tools.cueModel)}
            {@render tool(t("settings.toolsBeatModel"), tools.beatModel)}
            <div class="line">
              <button class="btn" disabled={tools.job != null || !tools.installable} onclick={installTools}>
                <Download size={15} />
                {tools.job != null ? t("settings.toolsInstalling") : toolsReady ? t("settings.toolsReinstall") : t("settings.toolsInstall")}
              </button>
            </div>
            {#if toolsLine}<p class="hint" class:failed={toolsFailed}>{toolsLine}</p>{/if}
            {#if !tools.installable}<p class="hint failed">{t("settings.toolsNoUv")}</p>{/if}
          {/if}
          <p class="hint">{t("settings.toolsHint")}</p>
        </section>
        <section>
          <div class="eyebrow">{t("settings.diskAccess")}</div>
          <div class="line">
            {#if fullDisk}
              <span class="ok"><CircleCheck size={16} />{t("settings.diskOn")}</span>
            {:else if fullDisk === false}
              <span class="warn"><ShieldAlert size={16} />{t("settings.diskOff")}</span>
              <button class="btn" onclick={() => api.openFullDiskSettings()}>{t("settings.openSystemSettings")}</button>
            {/if}
          </div>
          <p class="hint">{t("settings.diskHint")}</p>
        </section>
      </div>
    {:else if section === "player"}
      <p class="lead">{t("settings.playerLead")}</p>
      <PlayerSettingsEditor stick={null} />
    {:else if section === "stick"}
      <div class="pane">
        <p class="lead">{t("settings.stickLead")}</p>
        <section class="opts">
          <SheetSwitch
            checked={p.stick.flat}
            onchange={(on) => set("stick", { flat: on })}
            label={t("stick.flat")}
            desc={t("stick.flatHint")}
          />
          <SheetSwitch
            checked={p.stick.writeSettings}
            onchange={(on) => set("stick", { writeSettings: on })}
            label={t("settings.writeSettings")}
            desc={t("settings.writeSettingsHint")}
          />
          <SheetSwitch
            checked={!!p.stick.engine}
            onchange={(on) => set("stick", { engine: on ? "3.0.1" : null })}
            label={t("stick.hybrid")}
            desc={t("settings.hybridHint")}
          />
          {#if p.stick.engine}
            <div class="sub">
              <div class="seg" role="radiogroup" aria-label={t("stick.oldestDenon")}>
                {#each ENGINE as o (o.schema)}
                  <button role="radio" aria-checked={p.stick.engine === o.schema} class:on={p.stick.engine === o.schema} onclick={() => set("stick", { engine: o.schema })}>
                    {o.label}<span class="n">{t(o.note)}</span>
                  </button>
                {/each}
              </div>
              <p class="hint">{t("settings.engineHint")}</p>
            </div>
          {/if}
        </section>
        <footer><button class="btn ghost" onclick={() => prefs.reset("stick")}>{t("common.resetDefaults")}</button></footer>
      </div>
    {:else if section === "planner"}
      <div class="pane">
        <p class="lead">{t("settings.plannerLead")}</p>
        <section class="rows">
          <div class="row">
            <span class="label">{t("planner.order.title")}</span>
            <div class="seg">
              <button class:on={p.planner.keepOrder} onclick={() => set("planner", { keepOrder: true })} title={t("planner.order.mineHint")}>{t("planner.order.mine")}</button>
              <button class:on={!p.planner.keepOrder} onclick={() => set("planner", { keepOrder: false })} title={t("planner.order.suggestHint")}>{t("planner.order.suggest")}</button>
            </div>
          </div>
          <div class="row">
            <span class="label">{t("planner.energy")}</span>
            <div class="seg">
              {#each PLAN_CURVES as [v, label] (v)}
                <button class:on={p.planner.curve === v} onclick={() => set("planner", { curve: v })}>{t(label)}</button>
              {/each}
            </div>
          </div>
          <div class="row">
            <span class="label">{t("planner.level")}</span>
            <div class="seg">
              {#each PLAN_LEVELS as [v, label, hint] (v)}
                <button class:on={p.planner.level === v} onclick={() => set("planner", { level: v })} title={t(hint)}>{t(label)}</button>
              {/each}
            </div>
          </div>
          <div class="row">
            <span class="label">{t("planner.cues")}</span>
            <div class="seg">
              {#each PLAN_CUES as [v, label, hint] (v)}
                <button class:on={p.planner.cueMode === v} onclick={() => set("planner", { cueMode: v })} title={t(hint)}>{t(label)}</button>
              {/each}
            </div>
          </div>
          <div class="row">
            <span class="label">{t("planner.length")} <span class="desc">{t("planner.lengthHint")}</span></span>
            <div class="len">
              <input type="range" min="15" max="240" step="5" value={p.planner.minutes} oninput={(e) => set("planner", { minutes: Number(e.currentTarget.value) })} />
              <span class="readout">{t("common.minutes", { count: p.planner.minutes })}</span>
            </div>
          </div>
        </section>
        <footer><button class="btn ghost" onclick={() => prefs.reset("planner")}>{t("common.resetDefaults")}</button></footer>
      </div>
    {:else if section === "export"}
      <div class="pane">
        <p class="lead">{t("settings.exportLead")}</p>
        <section class="opts">
          <SheetSwitch checked={p.export.numbered} onchange={(on) => set("export", { numbered: on })} label={t("export.numbered")} />
          <SheetSwitch checked={p.export.artistTitle} onchange={(on) => set("export", { artistTitle: on })} label={t("export.artistTitle")} />
          <SheetSwitch checked={p.export.m3u} onchange={(on) => set("export", { m3u: on })} label={t("export.m3u")} />
          <SheetSwitch checked={p.export.history} onchange={(on) => set("export", { history: on })} label={t("export.history")} desc={t("export.historyHint")} />
        </section>
        <section class="rows">
          <div class="row">
            <span class="label">{t("export.duplicates")}</span>
            <div class="seg" role="radiogroup" aria-label={t("export.duplicates")}>
              {#each DUPLICATES as [v, label] (v)}
                <button role="radio" aria-checked={p.export.duplicates === v} class:on={p.export.duplicates === v} onclick={() => set("export", { duplicates: v })}>{t(label)}</button>
              {/each}
            </div>
          </div>
        </section>
        <footer><button class="btn ghost" onclick={() => prefs.reset("export")}>{t("common.resetDefaults")}</button></footer>
      </div>
    {:else if section === "peer"}
      <div class="pane">
        <p class="lead">{t("settings.peerLead")}</p>
        <section>
          <div class="eyebrow">{t("settings.musicDir")}</div>
          <div class="line">
            <span class="path"><Folder size={15} />{peer ? home(peer.musicDir) : "…"}</span>
            <span class="grow"></span>
            {#if peer && !peer.defaultMusicDir}
              <button class="btn ghost" onclick={() => musicDir(null)}>{t("common.default")}</button>
            {/if}
            <button class="btn" onclick={chooseMusicDir} disabled={!peer}>{t("common.change")}</button>
          </div>
          <p class="hint">{t("settings.musicDirHint")}</p>
        </section>
        {#if peer}
          <section>
            <SheetSwitch
              checked={peer.autoMirror}
              onchange={autoMirror}
              label={t("settings.autoMirror")}
              desc={t("settings.autoMirrorHint")}
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
  .hint.failed {
    color: var(--danger);
    overflow-wrap: anywhere;
  }
  .tool {
    min-height: 24px;
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
