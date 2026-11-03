<script lang="ts">
  import { CircleCheck, CircleX, Eraser, HardDrive, LoaderCircle, RefreshCw, TriangleAlert, Usb } from "@lucide/svelte";
  import { playerApi, type Disk } from "./api";
  import { gb } from "./format";
  import { t } from "./i18n.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import SheetSwitch from "./SheetSwitch.svelte";
  import { openSettings, prefs } from "./prefs.svelte";
  import { app } from "./state.svelte";

  let disks = $state<Disk[] | null>(null);
  let diskId = $state<string | null>(null);
  let fs = $state<"fat32" | "exfat">("fat32");
  let name = $state("");
  let confirm = $state("");
  let writeSettings = $state(prefs.value.stick.writeSettings);
  let jobId = $state<number | null>(null);
  let failed = $state<string | null>(null);

  const job = $derived(jobId != null ? app.jobs.find((j) => j.id === jobId) : undefined);
  const disk = $derived(disks?.find((d) => d.id === diskId));
  const confirmName = $derived(disk ? (disk.volumes.find((v) => v.name)?.name ?? disk.id) : "");

  async function load() {
    failed = null;
    try {
      disks = await playerApi.listDisks();
      if (!disks.some((d) => d.id === diskId)) {
        // The stick open in the app, else the first one that may be formatted.
        const open = app.source === "stick" ? app.stickPath : null;
        const pick =
          disks.find((d) => !d.blocked && d.volumes.some((v) => v.mountPoint === open)) ?? disks.find((d) => !d.blocked);
        pick?.id && choose(pick);
      }
    } catch (e) {
      failed = String(e);
    }
  }
  load();

  function cleanName(s: string): string {
    return s
      .normalize("NFD")
      .replace(/[̀-ͯ]/g, "")
      .replace(/[^A-Za-z0-9 _-]/g, "")
      .slice(0, 11)
      .trim();
  }

  function choose(d: Disk) {
    if (d.blocked) return;
    diskId = d.id;
    confirm = "";
    name = cleanName(d.volumes.find((v) => v.name)?.name ?? "") || "REKORDBOX";
  }

  const shownName = $derived(fs === "fat32" ? name.toUpperCase() : name);
  const nameOk = $derived(/^[A-Za-z0-9 _-]{1,11}$/.test(name.trim()));
  const ready = $derived(!!disk && !disk.blocked && nameOk && confirm === confirmName);

  async function start() {
    if (!disk) return;
    failed = null;
    try {
      jobId = await playerApi.formatStart(disk.id, fs, name, confirm, writeSettings);
    } catch (e) {
      failed = String(e);
    }
  }

  function close() {
    app.formatOpen = false;
    app.refreshSticks();
  }

  function sync() {
    app.formatOpen = false;
    app.refreshSticks().then(() => (app.syncOpen = true));
  }

  /** The erase warning, its `**bold**` part apart: [before, bold, after]. */
  const eraseText = $derived(disk ? t("sync.format.erase", { name: confirmName, gb: gb(disk.size) }).split("**") : []);
  const confirmText = $derived(t("sync.format.confirm", { name: "\u0000" }).split("\u0000"));
</script>

<svelte:window
  onkeydown={(e) => e.key === "Escape" && job?.state !== "running" && app.settingsFor === undefined && close()}
/>

<SheetFrame
  id="fmt-title"
  icon={Eraser}
  tone="danger"
  title={job?.state === "running" ? t("sync.format.titleRunning", { name: shownName }) : t("sync.format.title")}
  sub={t("sync.format.sub")}
  width={760}
  onclose={job?.state === "running" ? undefined : close}
>
  {#if !job}
    <div class="body">
      <div class="sec-head">
        <span class="eyebrow">{t("sync.format.disk")}</span>
        <button class="btn ghost icon sm" title={t("sync.format.reload")} aria-label={t("sync.format.reload")} onclick={load}><RefreshCw size={13} /></button>
      </div>
      {#if disks === null}
        <p class="none"><LoaderCircle size={14} class="spin" /> {t("sync.format.searching")}</p>
      {:else if disks.length === 0}
        <p class="none"><HardDrive size={14} /> {t("sync.format.none")}</p>
      {:else}
        <div class="list">
          {#each disks as d (d.id)}
            {@const vols = d.volumes.filter((v) => v.content !== "EFI")}
            <label class="disk" class:on={diskId === d.id} class:blocked={!!d.blocked}>
              <input type="radio" name="disk" checked={diskId === d.id} disabled={!!d.blocked} onchange={() => choose(d)} />
              <span class="dic">{#if d.removable}<Usb size={17} />{:else}<HardDrive size={17} />{/if}</span>
              <span class="dtext">
                <span class="dname">{d.media} · {gb(d.size)} GB</span>
                <span class="meta">
                  {[d.bus, d.id, vols.map((v) => v.name || t("sync.format.untitled")).join(", ")].filter(Boolean).join(" · ")}
                </span>
                {#if d.blocked}<span class="why">{d.blocked}</span>{/if}
              </span>
              <span class="radio" aria-hidden="true"></span>
            </label>
          {/each}
        </div>
      {/if}

      {#if disk}
        <div class="eyebrow">{t("sync.format.filesystem")}</div>
        <div class="fs">
          <label class="card" class:on={fs === "fat32"}>
            <input type="radio" name="fs" value="fat32" bind:group={fs} />
            <span class="t">FAT32 <span class="tag">{t("stick.engine.recommended")}</span></span>
            <span class="d">{t("sync.format.fat32Hint")}</span>
          </label>
          <label class="card" class:on={fs === "exfat"}>
            <input type="radio" name="fs" value="exfat" bind:group={fs} />
            <span class="t">exFAT</span>
            <span class="d">{t("sync.format.exfatHint")}</span>
          </label>
        </div>

        <label class="eyebrow" for="fmt-name">{t("sync.format.name")}</label>
        <input id="fmt-name" class="field" type="text" bind:value={name} maxlength="11" spellcheck="false" />
        <span class="hint" class:err={!nameOk}>
          {nameOk ? t("sync.format.onPlayer", { name: shownName.trim() }) : t("sync.format.nameRule")}
        </span>

        <div class="sw">
          <SheetSwitch bind:checked={writeSettings} label={t("sync.format.writeSettings")}>
            {#snippet extra()}
              <button class="link" onclick={(e) => (e.preventDefault(), openSettings("player"))}>{t("stick.edit")}</button>
            {/snippet}
          </SheetSwitch>
        </div>

        <div class="erase-warn">
          <div class="msg">
            <TriangleAlert size={16} />
            <p>
              {eraseText[0]}<strong>{eraseText[1]}</strong>{eraseText[2]}
            </p>
          </div>
          {#if !disk.removable}
            <p class="ext">{t("sync.format.external")}</p>
          {/if}
          <label class="confirm">
            <span>{confirmText[0]}<b>{confirmName}</b>{confirmText[1]}</span>
            <input class="field" type="text" bind:value={confirm} spellcheck="false" autocomplete="off" />
          </label>
        </div>
      {/if}
      {#if failed}<p class="fail"><CircleX size={15} /> {failed}</p>{/if}
    </div>
  {:else if job.state === "running"}
    <div class="body">
      <p class="cur">{job.current}</p>
      <div class="bigbar"><div style:width="{(job.fraction ?? 0) * 100}%"></div></div>
      <p class="cur">{t("sync.format.dontUnplug")}</p>
    </div>
  {:else}
    <div class="body done">
      {#if job.state === "done"}
        <span class="done-ic"><CircleCheck size={30} /></span>
        <h3>{t("common.done")}</h3>
      {:else}
        <span class="done-ic bad"><CircleX size={30} /></span>
        <h3>{t("sync.format.failed")}</h3>
      {/if}
      <p>{job.message}</p>
    </div>
  {/if}

  {#snippet foot()}
    {#if !job}
      <button class="btn ghost lg" onclick={close}>{t("common.cancel")}</button>
      <button class="btn danger lg" onclick={start} disabled={!ready}><Eraser size={16} /> {t("sync.format.start")}</button>
    {:else if job.state !== "running"}
      {#if job.state === "done"}
        <button class="btn lg" onclick={() => (app.settingsFor = job.dest)}>{t("player.title")}</button>
      {/if}
      <button class="btn ghost lg" onclick={close}>{t("common.close")}</button>
      {#if job.state === "done"}
        <button class="btn primary lg" onclick={sync}>{t("sync.title")}</button>
      {/if}
    {/if}
  {/snippet}
</SheetFrame>

<style>
  .body {
    padding: 22px 26px 24px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
  }
  .eyebrow {
    display: block;
    margin: 20px 0 10px;
  }
  .sec-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 6px;
  }
  .sec-head .eyebrow {
    margin: 0;
  }
  .btn.sm {
    width: 26px;
    height: 26px;
  }
  .none {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 0;
    color: var(--text-2);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .disk {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 14px 16px;
    border-radius: var(--r-panel);
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--hairline);
    cursor: pointer;
  }
  .disk.on {
    box-shadow: inset 0 0 0 1.5px var(--danger);
    background: var(--danger-soft);
  }
  .disk.blocked {
    cursor: default;
    opacity: 0.55;
  }
  .disk input,
  .card input {
    position: absolute;
    opacity: 0;
    pointer-events: none;
  }
  .disk:has(input:focus-visible),
  .card:has(input:focus-visible) {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .dic {
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: var(--r-control);
    display: grid;
    place-items: center;
    background: var(--surface-3);
    color: var(--text-2);
  }
  .dtext {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .dname {
    font-size: 14px;
    font-weight: 600;
  }
  .meta,
  .why {
    font-size: 12px;
    color: var(--text-3);
  }
  .why {
    color: var(--warning);
  }
  .radio {
    flex: none;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1.5px var(--text-3);
  }
  .disk.on .radio {
    box-shadow: inset 0 0 0 1.5px var(--danger);
    background: radial-gradient(circle, var(--danger) 0 4.5px, transparent 5px);
  }
  .fs {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 14px 16px;
    border-radius: var(--r-panel);
    background: var(--surface-2);
    box-shadow: inset 0 0 0 1px var(--hairline);
    cursor: pointer;
  }
  .card:hover {
    background: var(--surface-3);
  }
  .card.on {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .t {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 16px;
    font-weight: 600;
  }
  .d {
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text-2);
  }
  .tag {
    font-size: 11px;
    font-weight: 600;
    color: var(--on-accent);
    background: var(--accent);
    border-radius: 6px;
    padding: 1px 7px;
  }
  .field {
    width: 100%;
    height: 40px;
    padding: 0 14px;
    font-size: 14px;
    border-radius: var(--r-control);
  }
  .hint {
    margin-top: 6px;
    font-size: 12px;
    color: var(--text-3);
  }
  .hint.err {
    color: var(--danger);
  }
  .sw {
    margin-top: 18px;
  }
  .link {
    border: 0;
    background: none;
    color: var(--accent);
    padding: 0 0 0 8px;
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
  }
  .erase-warn {
    margin-top: 18px;
    padding: 14px 16px 16px;
    border-radius: var(--r-panel);
    background: var(--danger-soft);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .msg {
    display: flex;
    gap: 10px;
    color: var(--danger);
  }
  .msg :global(svg) {
    flex: none;
    margin-top: 1px;
  }
  .msg p {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: #f2a4a6;
  }
  .msg strong {
    color: var(--danger);
  }
  .ext {
    margin: 0;
    font-size: 12.5px;
    color: var(--warning);
  }
  .confirm {
    display: flex;
    flex-direction: column;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .confirm b {
    margin-left: 4px;
    color: var(--text);
  }
  .confirm .field {
    background: var(--surface-1);
    border-color: rgb(240 88 92 / 0.35);
  }
  .confirm .field:focus {
    border-color: var(--danger);
  }
  .fail {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 14px 0 0;
    color: var(--danger);
  }
  .cur {
    margin: 0;
    color: var(--text-2);
  }
  .bigbar {
    height: 8px;
    border-radius: 4px;
    background: var(--surface-3);
    overflow: hidden;
    margin: 18px 0 14px;
  }
  .bigbar div {
    height: 100%;
    border-radius: 4px;
    background: var(--danger);
    transition: width 0.3s ease-out;
  }
  .done {
    align-items: center;
    text-align: center;
    gap: 8px;
    padding: 34px 26px;
  }
  .done h3 {
    margin: 0;
    font-size: 18px;
  }
  .done p {
    margin: 0;
    max-width: 480px;
    color: var(--text-2);
    line-height: 1.5;
  }
  .done-ic {
    width: 56px;
    height: 56px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    background: var(--success-soft);
    color: var(--success);
    margin-bottom: 4px;
  }
  .done-ic.bad {
    background: var(--danger-soft);
    color: var(--danger);
  }
</style>
