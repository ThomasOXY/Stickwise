<script lang="ts">
  import { CircleCheck, CircleX, Eraser, HardDrive, LoaderCircle, RefreshCw, TriangleAlert, Usb } from "@lucide/svelte";
  import { playerApi, type Disk } from "./api";
  import { gb } from "./format";
  import SheetFrame from "./SheetFrame.svelte";
  import SheetSwitch from "./SheetSwitch.svelte";
  import { app } from "./state.svelte";

  let disks = $state<Disk[] | null>(null);
  let diskId = $state<string | null>(null);
  let fs = $state<"fat32" | "exfat">("fat32");
  let name = $state("");
  let confirm = $state("");
  let writeSettings = $state(true);
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
</script>

<svelte:window
  onkeydown={(e) => e.key === "Escape" && job?.state !== "running" && app.settingsFor === undefined && close()}
/>

<SheetFrame
  id="fmt-title"
  icon={Eraser}
  tone="danger"
  title={job?.state === "running" ? `Formátuji ${shownName}` : "Formátovat flashku"}
  sub="Flashka dostane jeden oddíl (MBR) a prázdnou rekordbox knihovnu, takže ji přehrávače hned poznají."
  width={760}
  onclose={job?.state === "running" ? undefined : close}
>
  {#if !job}
    <div class="body">
      <div class="sec-head">
        <span class="eyebrow">Disk</span>
        <button class="btn ghost icon sm" title="Načíst znovu" aria-label="Načíst znovu" onclick={load}><RefreshCw size={13} /></button>
      </div>
      {#if disks === null}
        <p class="none"><LoaderCircle size={14} class="spin" /> Hledám flashky…</p>
      {:else if disks.length === 0}
        <p class="none"><HardDrive size={14} /> Žádná USB flashka ani karta. Připoj ji a klikni na obnovit.</p>
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
                  {[d.bus, d.id, vols.map((v) => v.name || "Bez názvu").join(", ")].filter(Boolean).join(" · ")}
                </span>
                {#if d.blocked}<span class="why">{d.blocked}</span>{/if}
              </span>
              <span class="radio" aria-hidden="true"></span>
            </label>
          {/each}
        </div>
      {/if}

      {#if disk}
        <div class="eyebrow">Souborový systém</div>
        <div class="fs">
          <label class="card" class:on={fs === "fat32"}>
            <input type="radio" name="fs" value="fat32" bind:group={fs} />
            <span class="t">FAT32 <span class="tag">doporučeno</span></span>
            <span class="d">Přečtou ho všechny CDJ a XDJ (i CDJ-2000NXS, NXS2, XDJ-1000), Denon i rekordbox. Soubor max. 4 GB.</span>
          </label>
          <label class="card" class:on={fs === "exfat"}>
            <input type="radio" name="fs" value="exfat" bind:group={fs} />
            <span class="t">exFAT</span>
            <span class="d">CDJ-3000, CDJ-3000X, XDJ-AZ, OPUS-QUAD, Denon. CDJ-2000NXS2 a starší ji nepřečtou.</span>
          </label>
        </div>

        <label class="eyebrow" for="fmt-name">Název flashky</label>
        <input id="fmt-name" class="field" type="text" bind:value={name} maxlength="11" spellcheck="false" />
        <span class="hint" class:err={!nameOk}>
          {nameOk ? `Na přehrávači: ${shownName.trim()}` : "1–11 znaků: písmena bez diakritiky, číslice, mezera, _ -"}
        </span>

        <div class="sw">
          <SheetSwitch bind:checked={writeSettings} label="Nahrát moje nastavení přehrávače">
            {#snippet extra()}
              <button class="link" onclick={(e) => (e.preventDefault(), (app.settingsFor = null))}>upravit</button>
            {/snippet}
          </SheetSwitch>
        </div>

        <div class="erase-warn">
          <div class="msg">
            <TriangleAlert size={16} />
            <p>
              <strong>Všechno na {confirmName} ({gb(disk.size)} GB) se smaže</strong>: hudba, playlisty, historie hraní.
              Databáze a nastavení se předtím zazálohují do ~/RB-EXPORT/backups, hudba ne.
            </p>
          </div>
          {#if !disk.removable}
            <p class="ext">Tohle je externí disk, ne flashka. Zkontroluj, že je to opravdu ten správný.</p>
          {/if}
          <label class="confirm">
            <span>Pro potvrzení napiš název flashky: <b>{confirmName}</b></span>
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
      <p class="cur">Flashku nevytahuj, dokud to neskončí.</p>
    </div>
  {:else}
    <div class="body done">
      {#if job.state === "done"}
        <span class="done-ic"><CircleCheck size={30} /></span>
        <h3>Hotovo</h3>
      {:else}
        <span class="done-ic bad"><CircleX size={30} /></span>
        <h3>Formátování se nepovedlo</h3>
      {/if}
      <p>{job.message}</p>
    </div>
  {/if}

  {#snippet foot()}
    {#if !job}
      <button class="btn ghost lg" onclick={close}>Zrušit</button>
      <button class="btn danger lg" onclick={start} disabled={!ready}><Eraser size={16} /> Smazat a naformátovat</button>
    {:else if job.state !== "running"}
      {#if job.state === "done"}
        <button class="btn lg" onclick={() => (app.settingsFor = job.dest)}>Nastavení přehrávače</button>
      {/if}
      <button class="btn ghost lg" onclick={close}>Zavřít</button>
      {#if job.state === "done"}
        <button class="btn primary lg" onclick={sync}>Synchronizovat na flashku</button>
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
