<script lang="ts">
  import { CircleCheck, CircleX, FileMusic, FileText, Folder, FolderOpen, HardDriveDownload, History, Lock } from "@lucide/svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, type Playlist } from "./api";
  import { num, size } from "./format";
  import { t } from "./i18n.svelte";
  import SheetCheck from "./SheetCheck.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import SheetSwitch from "./SheetSwitch.svelte";
  import { DUPLICATES, prefs } from "./prefs.svelte";
  import { app } from "./state.svelte";

  const data = $derived(app.data!);
  let picked = $state<Set<number>>(new Set(app.data!.playlists.filter((p) => !p.isFolder).map((p) => p.id)));
  let dest = $state("");
  // From the settings; changed here, they hold for this export.
  const d = prefs.value.export;
  let numbered = $state(d.numbered);
  let artistTitle = $state(d.artistTitle);
  let m3u = $state(d.m3u);
  let history = $state(d.history);
  let duplicates = $state<"clone" | "copy" | "symlink">(d.duplicates);
  let jobId = $state<number | null>(null);
  let failed = $state<string | null>(null);

  $effect(() => {
    api.homeDir().then((h) => (dest ||= `${h}/Music/${data.info.name}`));
  });

  const job = $derived(jobId != null ? app.jobs.find((j) => j.id === jobId) : undefined);

  const selectedTracks = $derived.by(() => {
    const ids: number[] = [];
    for (const p of data.playlists) if (picked.has(p.id)) ids.push(...p.trackIds);
    if (history) for (const h of data.history) ids.push(...h.trackIds);
    return ids;
  });
  const uniqueIds = $derived(new Set(selectedTracks));
  const bytes = $derived([...uniqueIds].reduce((s, id) => s + (app.trackById.get(id)?.fileSize ?? 0), 0));
  const dupes = $derived(selectedTracks.length - uniqueIds.size);
  const allPlaylists = $derived(data.playlists.filter((p) => !p.isFolder));

  function leaves(folder: number): Playlist[] {
    const out: Playlist[] = [];
    for (const c of app.sortedChildren(folder)) out.push(...(c.isFolder ? leaves(c.id) : [c]));
    return out;
  }

  function checkState(p: Playlist): "on" | "off" | "some" {
    const ls = p.isFolder ? leaves(p.id) : [p];
    const n = ls.filter((l) => picked.has(l.id)).length;
    return n === 0 ? "off" : n === ls.length ? "on" : "some";
  }

  function toggle(p: Playlist) {
    const ls = p.isFolder ? leaves(p.id) : [p];
    const on = checkState(p) !== "on";
    const next = new Set(picked);
    for (const l of ls) on ? next.add(l.id) : next.delete(l.id);
    picked = next;
  }

  const allState = $derived(picked.size === 0 ? "off" : picked.size === allPlaylists.length ? "on" : "some");
  function toggleAll() {
    picked = allState === "on" ? new Set() : new Set(allPlaylists.map((p) => p.id));
  }

  const rows = $derived.by(() => {
    const out: { node: Playlist; depth: number }[] = [];
    const walk = (parent: number, depth: number) => {
      for (const n of app.sortedChildren(parent)) {
        out.push({ node: n, depth });
        if (n.isFolder) walk(n.id, depth + 1);
      }
    };
    walk(0, 0);
    return out;
  });

  async function chooseDest() {
    const d = await open({ directory: true, defaultPath: dest });
    if (typeof d === "string") dest = d;
  }

  const fileName = (id: number, i: number) => {
    const t = app.trackById.get(id);
    if (!t) return null;
    const ext = t.filePath.split(".").pop();
    const stem = artistTitle ? `${t.artist} – ${t.title}` : t.filePath.split("/").pop()!.replace(/\.[^.]+$/, "");
    return { name: `${numbered ? String(i + 1).padStart(2, "0") + " " : ""}${stem}.${ext}`, size: t.fileSize };
  };

  // Folder tree the export will make: the first playlist opened, the rest collapsed.
  const preview = $derived.by(() => {
    const chosen = allPlaylists.filter((p) => picked.has(p.id));
    if (!chosen.length) return null;
    const first = chosen[0];
    const files = first.trackIds
      .slice(0, 3)
      .map((id, i) => fileName(id, i))
      .filter((f) => !!f);
    return { first, files, more: first.trackIds.length - files.length, rest: chosen.slice(1, 7), hidden: Math.max(0, chosen.length - 7) };
  });

  async function start() {
    failed = null;
    try {
      jobId = await api.startExtract({
        stick: data.info.path,
        dest,
        playlists: [...picked],
        numbered,
        artistTitle,
        m3u,
        history,
        duplicates,
      });
    } catch (e) {
      failed = String(e);
    }
  }

  function close() {
    app.exportOpen = false;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape" && (!job || job.state !== "running")) close();
  }

  const home = (path: string) => path.replace(/^\/Users\/[^/]+/, "~");
  /** "{progress} tracks", split so the numbers can sit in their own markup. */
  const progress = $derived(t("export.progress", { progress: "\u0000" }).split("\u0000"));
</script>

<svelte:window onkeydown={onKey} />

<SheetFrame
  id="export-title"
  icon={HardDriveDownload}
  title={job?.state === "running" ? t("export.titleRunning") : t("export.title")}
  sub={t("export.sub", { name: data.info.name })}
  width={job ? 620 : 1100}
  onclose={job?.state === "running" ? undefined : close}
>
  {#if !job}
    <div class="cols">
      <div class="left">
        <section>
          <div class="eyebrow">{t("export.where")}</div>
          <div class="dest">
            <Folder size={15} />
            <span class="path">{home(dest)}</span>
            <button class="btn" onclick={chooseDest}>{t("common.change")}</button>
          </div>
        </section>

        <section>
          <div class="sec-head">
            <span class="eyebrow">{t("export.playlists")}</span>
            <span class="count">
              {t("sync.playlists", { count: num(picked.size) })} · {t("common.tracks", { count: num(uniqueIds.size) })} · {size(bytes)}
            </span>
          </div>
          <div class="pick">
            <button class="row all" class:on={allState === "on"} onclick={toggleAll}>
              <SheetCheck state={allState} />
              <span class="pname">{t("export.selectAll")}</span>
            </button>
            {#each rows as { node, depth } (node.id)}
              {@const st = checkState(node)}
              <button class="row" class:on={st === "on"} style:padding-left="{10 + depth * 22}px" onclick={() => toggle(node)} aria-label={t("export.select", { name: node.name })}>
                <SheetCheck state={st} />
                {#if node.isFolder}<Folder size={14} />{:else}<FileMusic size={14} />{/if}
                <span class="pname">{node.name}</span>
                <span class="readout n">
                  {node.isFolder ? leaves(node.id).reduce((s, l) => s + l.trackIds.length, 0) : node.trackIds.length}
                </span>
              </button>
            {/each}
          </div>
        </section>

        <section>
          <div class="eyebrow">{t("export.naming")}</div>
          <div class="opts">
            <SheetSwitch bind:checked={numbered} label={t("export.numbered")} />
            <SheetSwitch bind:checked={artistTitle} label={t("export.artistTitle")} />
            <SheetSwitch bind:checked={m3u} label={t("export.m3u")} />
            <SheetSwitch bind:checked={history} label={t("export.history")} desc={t("export.historyHint")} />
          </div>
        </section>

        <section>
          <div class="eyebrow">{t("export.duplicates")}</div>
          <div class="dupes">
            <div class="seg" role="radiogroup" aria-label={t("export.duplicates")}>
              {#each DUPLICATES as [v, label] (v)}
                <button role="radio" aria-checked={duplicates === v} class:on={duplicates === v} onclick={() => (duplicates = v as typeof duplicates)}>{t(label)}</button>
              {/each}
            </div>
            <span class="count">{t("export.dupCount", { count: num(dupes) })}</span>
          </div>
        </section>
      </div>

      <aside class="right">
        <div class="eyebrow">{t("export.preview")}</div>
        <div class="tree">
          <div class="t0"><FolderOpen size={14} /><span>{dest.split("/").pop()}</span></div>
          {#if preview}
            <div class="t1"><FolderOpen size={14} /><span>{preview.first.name}</span><em>{preview.first.trackIds.length}</em></div>
            {#each preview.files as f}
              <div class="t2"><FileMusic size={14} /><span>{f!.name}</span><em>{size(f!.size)}</em></div>
            {/each}
            {#if preview.more > 0}<div class="t2 dimmed"><span>{t("export.moreFiles", { count: num(preview.more) })}</span></div>{/if}
            {#if m3u}<div class="t2"><FileText size={14} /><span>{preview.first.name}.m3u8</span></div>{/if}
            {#each preview.rest as p (p.id)}
              <div class="t1"><Folder size={14} /><span>{p.name}</span><em>{p.trackIds.length}</em></div>
            {/each}
            {#if preview.hidden}<div class="t1 dimmed"><span>{t("export.morePlaylists", { count: num(preview.hidden) })}</span></div>{/if}
          {:else}
            <div class="t1 dimmed"><span>{t("export.pickOne")}</span></div>
          {/if}
          {#if history}<div class="t1"><History size={14} /><span>{t("export.historyFolder")}</span></div>{/if}
        </div>

        <div class="sum">
          <div><span>{t("export.files")}</span><span class="readout">{uniqueIds.size}</span></div>
          <div><span>{t("export.diskSpace")}</span><span class="readout">{size(bytes)}</span></div>
          <div>
            <span>{duplicates === "clone" ? t("export.clones") : duplicates === "copy" ? t("export.copies") : t("export.links")}</span>
            <span class="readout">{dupes}</span>
          </div>
        </div>
        {#if failed}<div class="callout"><CircleX size={15} /><span>{failed}</span></div>{/if}

        <p class="safe"><Lock size={14} /> {t("export.readOnly")}</p>
      </aside>
    </div>
  {:else if job.state === "running"}
    <div class="body">
      <p class="sub">{t("export.running", { name: data.info.name, dest: home(job.dest) })}</p>
      <div class="bigbar"><div style:width="{(job.bytesDone / Math.max(1, job.bytesTotal)) * 100}%"></div></div>
      <div class="stats">
        <span>{progress[0]}<span class="readout">{job.filesDone} / {job.filesTotal}</span>{progress[1]}</span>
        <span><span class="readout">{size(job.bytesDone)} / {size(job.bytesTotal)}</span></span>
      </div>
      <p class="current">{job.current}</p>
    </div>
  {:else}
    <div class="body done">
      {#if job.state === "done"}
        <span class="done-ic"><CircleCheck size={30} /></span>
        <h3>{t("common.done")}</h3>
        <p>{t("export.doneText", { count: num(job.filesTotal), dest: home(job.dest) })}</p>
      {:else if job.state === "cancelled"}
        <h3>{t("export.stopped")}</h3>
        <p class="sub">{t("export.stoppedHint")}</p>
      {:else}
        <span class="done-ic bad"><CircleX size={30} /></span>
        <h3>{t("export.failed")}</h3>
      {/if}
      <p class="sub">{job.message}</p>
    </div>
  {/if}

  {#snippet foot()}
    {#if !job}
      <button class="btn ghost lg" onclick={close}>{t("common.cancel")}</button>
      <button class="btn primary lg" onclick={start} disabled={picked.size === 0 && !history}>
        <HardDriveDownload size={16} />
        {t("export.start", { count: num(uniqueIds.size) })}
      </button>
    {:else if job.state === "running"}
      <button class="btn lg" onclick={() => api.cancelJob(job.id)}>{t("common.stop")}</button>
    {:else}
      <button class="btn lg" onclick={() => api.reveal(job.dest)}>{t("export.reveal")}</button>
      <button class="btn primary lg" onclick={close}>{t("common.close")}</button>
    {/if}
  {/snippet}
</SheetFrame>

<style>
  .cols {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 400px;
  }
  .left {
    overflow-y: auto;
    padding: 22px 26px 26px;
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  .left .eyebrow {
    display: block;
    margin-bottom: 10px;
  }
  .sec-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-bottom: 10px;
  }
  .sec-head .eyebrow {
    margin: 0;
  }
  .count {
    font-size: 12px;
    color: var(--text-3);
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
  .pick {
    max-height: 280px;
    overflow-y: auto;
    border-radius: var(--r-panel);
    background: var(--surface-2);
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 36px;
    flex: none;
    padding-right: 12px;
    border: 0;
    border-radius: var(--r-control);
    background: none;
    color: var(--text-2);
    text-align: left;
    cursor: pointer;
  }
  .row:hover {
    background: var(--surface-3);
  }
  .row.on {
    color: var(--text);
    background: rgb(255 255 255 / 0.025);
  }
  .row.all {
    padding-left: 10px;
    font-weight: 500;
  }
  .row :global(svg) {
    flex: none;
    color: var(--text-3);
  }
  .pname {
    flex: 1;
    min-width: 0;
    font-size: 13.5px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .n {
    font-size: 13px;
    color: var(--text-3);
  }
  .opts {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .dupes {
    display: flex;
    align-items: center;
    gap: 14px;
    flex-wrap: wrap;
  }

  .right {
    background: var(--surface-2);
    padding: 22px 24px;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  .tree {
    border-radius: var(--r-panel);
    background: var(--surface-1);
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    font-size: 13px;
  }
  .tree > div {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    min-width: 0;
  }
  .tree span {
    flex: 1;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tree em {
    font-style: normal;
    font-family: var(--font-readout);
    font-weight: 600;
    font-size: 12px;
    color: var(--text-3);
  }
  .tree :global(svg) {
    flex: none;
    color: var(--accent);
  }
  .t1 {
    padding-left: 18px;
  }
  .t2 {
    padding-left: 38px;
    color: var(--text-2);
  }
  .t2 :global(svg) {
    color: var(--text-3) !important;
  }
  .dimmed {
    color: var(--text-3);
  }
  .sum {
    border-radius: var(--r-panel);
    background: var(--surface-1);
    padding: 8px 16px;
  }
  .sum div {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    height: 30px;
    line-height: 30px;
    font-size: 13px;
    color: var(--text-2);
  }
  .sum .readout {
    font-size: 17px;
    color: var(--text);
  }
  .callout {
    display: flex;
    gap: 10px;
    padding: 12px 14px;
    border-radius: var(--r-panel);
    background: var(--danger-soft);
    color: var(--danger);
    font-size: 12.5px;
  }
  .safe {
    margin: auto 0 0;
    padding-top: 10px;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12.5px;
    color: var(--text-2);
  }
  .safe :global(svg) {
    color: var(--success);
  }

  .body {
    padding: 24px 26px;
    overflow-y: auto;
  }
  .sub {
    margin: 0;
    font-size: 13px;
    color: var(--text-2);
  }
  .bigbar {
    height: 8px;
    border-radius: 4px;
    background: var(--surface-3);
    overflow: hidden;
    margin: 18px 0 12px;
  }
  .bigbar div {
    height: 100%;
    border-radius: 4px;
    background: var(--accent);
    transition: width 0.15s linear;
  }
  .stats {
    display: flex;
    justify-content: space-between;
    color: var(--text-3);
  }
  .stats .readout {
    font-size: 16px;
    color: var(--text);
  }
  .current {
    margin: 12px 0 0;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .done {
    display: flex;
    flex-direction: column;
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
