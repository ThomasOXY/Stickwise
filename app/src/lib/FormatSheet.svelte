<script lang="ts">
  import { CircleCheck, HardDrive, LoaderCircle, RefreshCw, TriangleAlert } from "@lucide/svelte";
  import { playerApi, type Disk } from "./api";
  import { gb } from "./format";
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

<div class="dim"></div>
<div class="sheet" role="dialog" aria-modal="true" aria-labelledby="fmt-title">
  {#if !job}
    <div class="body">
      <h2 id="fmt-title">Formátovat flashku</h2>
      <p class="sub">Flashka dostane jeden oddíl (MBR) a prázdnou rekordbox knihovnu, takže ji přehrávače hned poznají.</p>

      <h3>
        Flashka
        <button class="icon-btn" title="Načíst znovu" onclick={load}><RefreshCw size={12} /></button>
      </h3>
      {#if disks === null}
        <p class="none"><LoaderCircle size={14} class="spin" /> Hledám flashky…</p>
      {:else if disks.length === 0}
        <p class="none"><HardDrive size={14} /> Žádná USB flashka ani karta. Připoj ji a klikni na obnovit.</p>
      {:else}
        <div class="list">
          {#each disks as d (d.id)}
            <label class="disk" class:on={diskId === d.id} class:blocked={!!d.blocked}>
              <input type="radio" name="disk" checked={diskId === d.id} disabled={!!d.blocked} onchange={() => choose(d)} />
              <span class="dname">{d.volumes.filter((v) => v.content !== "EFI").map((v) => v.name).filter(Boolean).join(", ") || "Bez názvu"}</span>
              <span class="meta">{d.media} · {gb(d.size)} GB · {d.id}</span>
              {#if d.blocked}<span class="why">{d.blocked}</span>{/if}
            </label>
          {/each}
        </div>
      {/if}

      {#if disk}
        <h3>Souborový systém</h3>
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

        <div class="row2">
          <label class="lbl">
            Název flashky
            <input type="text" bind:value={name} maxlength="11" spellcheck="false" />
            <span class="hint" class:err={!nameOk}>
              {nameOk ? `Na přehrávači: ${shownName.trim()}` : "1–11 znaků: písmena bez diakritiky, číslice, mezera, _ -"}
            </span>
          </label>
          <label class="opt">
            <input type="checkbox" bind:checked={writeSettings} />
            <span>
              Nahrát moje nastavení přehrávače
              <button class="link" onclick={(e) => (e.preventDefault(), (app.settingsFor = null))}>upravit</button>
            </span>
          </label>
        </div>

        <div class="erase-warn">
          <TriangleAlert size={16} />
          <div>
            <p>
              <strong>Všechno na {confirmName} ({gb(disk.size)} GB) se smaže</strong>: hudba, playlisty, historie hraní.
              Databáze a nastavení se předtím zazálohují do ~/RB-EXPORT/backups, hudba ne.
            </p>
            {#if !disk.removable}
              <p class="warn">Tohle je externí disk, ne flashka. Zkontroluj, že je to opravdu ten správný.</p>
            {/if}
            <label class="lbl">
              Pro potvrzení napiš název flashky: <span class="readout">{confirmName}</span>
              <input type="text" bind:value={confirm} spellcheck="false" autocomplete="off" />
            </label>
          </div>
        </div>
      {/if}
      {#if failed}<p class="err">{failed}</p>{/if}
    </div>
    <div class="foot">
      <button class="btn" onclick={close}>Zrušit</button>
      <button class="btn danger" onclick={start} disabled={!ready}>Smazat a naformátovat</button>
    </div>
  {:else if job.state === "running"}
    <div class="body">
      <h2>Formátuji {shownName}</h2>
      <p class="sub">{job.current}</p>
      <div class="bigbar"><div style:width="{(job.fraction ?? 0) * 100}%"></div></div>
      <p class="sub">Flashku nevytahuj, dokud to neskončí.</p>
    </div>
  {:else}
    <div class="body done">
      {#if job.state === "done"}
        <CircleCheck size={36} color="var(--play)" />
        <h2>Hotovo</h2>
      {:else}
        <h2>Formátování se nepovedlo</h2>
      {/if}
      <p>{job.message}</p>
    </div>
    <div class="foot">
      {#if job.state === "done"}
        <button class="btn" onclick={() => (app.settingsFor = job.dest)}>Nastavení přehrávače</button>
      {/if}
      <button class="btn" onclick={close}>Zavřít</button>
      {#if job.state === "done"}
        <button class="btn primary" onclick={sync}>Synchronizovat na flashku</button>
      {/if}
    </div>
  {/if}
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
    width: 600px;
    max-height: calc(100vh - 72px);
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: 0 24px 60px rgb(0 0 0 / 0.5);
    z-index: 21;
    animation: drop 0.18s ease-out;
  }
  @keyframes drop {
    from {
      transform: translate(-50%, -12px);
      opacity: 0;
    }
  }
  .body {
    padding: 20px;
    overflow-y: auto;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 16px 0 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .icon-btn {
    border: 0;
    background: none;
    color: var(--text-3);
    padding: 2px;
    display: flex;
  }
  .icon-btn:hover {
    color: var(--text);
  }
  .sub {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .none {
    display: flex;
    gap: 8px;
    align-items: center;
    color: var(--text-2);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .disk {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 2px 10px;
    padding: 8px 10px;
    border: 1px solid var(--line);
    border-radius: 6px;
  }
  .disk.on {
    border-color: var(--fault);
  }
  .disk.blocked {
    color: var(--text-3);
  }
  .disk input {
    accent-color: var(--fault);
    margin: 0;
  }
  .dname {
    font-weight: 600;
  }
  .meta {
    font-size: 12px;
    color: var(--text-2);
  }
  .why {
    grid-column: 2 / 4;
    font-size: 11px;
    color: var(--text-3);
  }
  .fs {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px;
  }
  .card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-radius: 6px;
    cursor: pointer;
  }
  .card.on {
    border-color: var(--cue);
    background: var(--booth);
  }
  .card input {
    display: none;
  }
  .t {
    font-weight: 600;
  }
  .d {
    font-size: 11px;
    line-height: 1.45;
    color: var(--text-2);
  }
  .tag {
    font-size: 10px;
    font-weight: 500;
    color: var(--cue);
    border: 1px solid rgb(242 163 58 / 0.5);
    border-radius: 3px;
    padding: 0 4px;
  }
  .row2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
    margin-top: 16px;
    align-items: start;
  }
  .lbl {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .lbl input {
    font-weight: 400;
  }
  .hint {
    font-weight: 400;
    color: var(--text-3);
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 20px;
  }
  .opt input {
    accent-color: var(--cue);
    width: 15px;
    height: 15px;
    margin: 0;
  }
  .link {
    border: 0;
    background: none;
    color: var(--cue);
    padding: 0 0 0 4px;
    font: inherit;
    text-decoration: underline;
  }
  .erase-warn {
    display: flex;
    gap: 10px;
    margin-top: 18px;
    padding: 12px;
    border: 1px solid rgb(229 72 77 / 0.45);
    border-radius: 6px;
    background: rgb(229 72 77 / 0.06);
    color: var(--fault);
  }
  .erase-warn p {
    margin: 0 0 10px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--text);
  }
  .erase-warn .warn {
    color: var(--warn);
  }
  .erase-warn .readout {
    color: var(--text);
  }
  .err {
    color: var(--fault);
  }
  .hint.err {
    color: var(--fault);
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--line);
  }
  .bigbar {
    height: 8px;
    border-radius: 4px;
    background: var(--line);
    overflow: hidden;
    margin: 22px 0 10px;
  }
  .bigbar div {
    height: 100%;
    background: var(--fault);
    transition: width 0.3s ease-out;
  }
  .done {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 6px;
    padding: 30px 20px;
  }
  .done p {
    max-width: 480px;
    color: var(--text-2);
  }
</style>
