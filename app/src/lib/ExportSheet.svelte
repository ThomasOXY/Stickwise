<script lang="ts">
  import { CircleCheck, FileMusic, FileText, Folder } from "@lucide/svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { api, type Playlist } from "./api";
  import { plural, size } from "./format";
  import { app } from "./state.svelte";

  const data = $derived(app.data!);
  let picked = $state<Set<number>>(new Set(app.data!.playlists.filter((p) => !p.isFolder).map((p) => p.id)));
  let dest = $state("");
  let numbered = $state(true);
  let artistTitle = $state(true);
  let m3u = $state(true);
  let history = $state(false);
  let duplicates = $state<"clone" | "copy" | "symlink">("clone");
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

  const preview = $derived.by(() => {
    const first = allPlaylists.find((p) => picked.has(p.id));
    if (!first) return null;
    const ts = first.trackIds.slice(0, 2).map((id) => app.trackById.get(id)).filter((t) => !!t);
    return {
      name: first.name,
      files: ts.map((t, i) => {
        const ext = t!.filePath.split(".").pop();
        const stem = artistTitle ? `${t!.artist} – ${t!.title}` : t!.filePath.split("/").pop()!.replace(/\.[^.]+$/, "");
        return `${numbered ? String(i + 1).padStart(2, "0") + " " : ""}${stem}.${ext}`;
      }),
    };
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
</script>

<svelte:window onkeydown={onKey} />

<div class="dim"></div>
<div class="sheet" role="dialog" aria-modal="true" aria-labelledby="export-title">
  {#if !job}
    <div class="body">
      <h2 id="export-title">Exportovat playlisty na disk</h2>
      <p class="sub">Z flashky {data.info.name} do složky na Macu</p>

      <div class="all">
        <button class="check {allState}" onclick={toggleAll} aria-label="Vybrat vše"></button>
        <span>Vybrat vše</span>
        <span class="totals">
          <span class="readout">{picked.size}</span> playlistů
          <span class="readout">{uniqueIds.size}</span> skladeb
          <span class="readout">{size(bytes)}</span>
        </span>
      </div>
      <div class="pick">
        {#each rows as { node, depth } (node.id)}
          <div class="pick-row" style:padding-left="{10 + depth * 22}px">
            <button class="check {checkState(node)}" onclick={() => toggle(node)} aria-label="Vybrat {node.name}"></button>
            {#if node.isFolder}<Folder size={14} />{/if}
            <span class="name">{node.name}</span>
            <span class="readout n">
              {node.isFolder ? leaves(node.id).reduce((s, l) => s + l.trackIds.length, 0) : node.trackIds.length}
            </span>
          </div>
        {/each}
      </div>

      <h3>Cíl</h3>
      <div class="dest">
        <div class="path"><Folder size={14} /><span>{dest.replace(/^\/Users\/[^/]+/, "~")}</span></div>
        <button class="btn" onclick={chooseDest}>Změnit…</button>
      </div>

      <h3>Možnosti</h3>
      <label class="opt"><input type="checkbox" bind:checked={numbered} /> Očíslovat podle pořadí (01 – …)</label>
      <label class="opt"><input type="checkbox" bind:checked={artistTitle} /> Pojmenovat Interpret – Název</label>
      <label class="opt"><input type="checkbox" bind:checked={m3u} /> Vytvořit .m3u8</label>
      <label class="opt"><input type="checkbox" bind:checked={history} /> Zahrnout historii</label>

      <div class="dupes">
        <div>
          <div class="strong">Duplicity</div>
          <div class="sub">{dupes} {plural(dupes, "skladba je", "skladby jsou", "skladeb je")} ve více playlistech</div>
        </div>
        <div class="seg" role="radiogroup" aria-label="Duplicity">
          {#each [["clone", "Klon (nezabírá místo)"], ["copy", "Kopie"], ["symlink", "Odkaz"]] as [v, label]}
            <button role="radio" aria-checked={duplicates === v} class:on={duplicates === v} onclick={() => (duplicates = v as typeof duplicates)}>{label}</button>
          {/each}
        </div>
      </div>

      {#if preview}
        <h3>Náhled</h3>
        <div class="preview">
          <div><Folder size={13} /> {dest.split("/").pop()}</div>
          <div class="in1"><Folder size={13} /> {preview.name}</div>
          {#each preview.files as f}<div class="in2"><FileMusic size={13} /> {f}</div>{/each}
          {#if m3u}<div class="in2"><FileText size={13} /> {preview.name}.m3u8</div>{/if}
        </div>
      {/if}
      {#if failed}<p class="err">{failed}</p>{/if}
    </div>
    <div class="foot">
      <button class="btn" onclick={close}>Zrušit</button>
      <button class="btn primary" onclick={start} disabled={picked.size === 0 && !history}>
        Exportovat {uniqueIds.size} {plural(uniqueIds.size, "skladbu", "skladby", "skladeb")}
      </button>
    </div>
  {:else if job.state === "running"}
    <div class="body progress">
      <h2>Exportuji playlisty</h2>
      <p class="sub">Z flashky {data.info.name} do {job.dest.replace(/^\/Users\/[^/]+/, "~")}</p>
      <div class="bigbar"><div style:width="{(job.bytesDone / Math.max(1, job.bytesTotal)) * 100}%"></div></div>
      <div class="stats">
        <span><span class="readout">{job.filesDone} / {job.filesTotal}</span> skladeb</span>
        <span><span class="readout">{size(job.bytesDone)} / {size(job.bytesTotal)}</span></span>
      </div>
      <p class="current">{job.current}</p>
    </div>
    <div class="foot">
      <button class="btn" onclick={() => api.cancelJob(job.id)}>Zastavit</button>
    </div>
  {:else}
    <div class="body done">
      {#if job.state === "done"}
        <CircleCheck size={36} color="var(--play)" />
        <h2>Hotovo</h2>
        <p>{job.filesTotal} {plural(job.filesTotal, "skladba je", "skladby jsou", "skladeb je")} v {job.dest.replace(/^\/Users\/[^/]+/, "~")}</p>
      {:else if job.state === "cancelled"}
        <h2>Export zastaven</h2>
        <p class="sub">Hotové soubory zůstaly na disku. Další export naváže tam, kde skončil.</p>
      {:else}
        <h2>Export skončil s chybami</h2>
      {/if}
      <p class="sub">{job.message}</p>
    </div>
    <div class="foot">
      <button class="btn" onclick={() => api.reveal(job.dest)}>Otevřít ve Finderu</button>
      <button class="btn primary" onclick={close}>Zavřít</button>
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
    width: 560px;
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
    padding: 20px 20px 16px;
    overflow-y: auto;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  h3 {
    margin: 16px 0 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .sub {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .strong {
    font-weight: 500;
  }
  .all {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 16px 0 8px;
  }
  .totals {
    margin-left: auto;
    font-size: 12px;
    color: var(--text-2);
    display: flex;
    gap: 6px;
    align-items: baseline;
  }
  .totals .readout {
    font-size: 14px;
    color: var(--text);
    margin-left: 6px;
  }
  .pick {
    max-height: 184px;
    overflow-y: auto;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--booth);
    padding: 4px 0;
  }
  .pick-row {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 26px;
    padding-right: 12px;
    color: var(--text);
  }
  .pick-row .name {
    flex: 1;
  }
  .n {
    font-size: 12px;
    color: var(--text-2);
  }
  .check {
    width: 16px;
    height: 16px;
    border-radius: 4px;
    border: 1px solid var(--text-3);
    background: transparent;
    padding: 0;
    flex: none;
    position: relative;
  }
  .check.on,
  .check.some {
    background: var(--cue);
    border-color: var(--cue);
  }
  .check.on::after {
    content: "";
    position: absolute;
    left: 5px;
    top: 2px;
    width: 4px;
    height: 8px;
    border: solid var(--booth);
    border-width: 0 2px 2px 0;
    transform: rotate(45deg);
  }
  .check.some::after {
    content: "";
    position: absolute;
    left: 3px;
    right: 3px;
    top: 6px;
    height: 2px;
    background: var(--booth);
  }
  .dest {
    display: flex;
    gap: 8px;
  }
  .path {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--booth);
    overflow: hidden;
    white-space: nowrap;
  }
  .opt {
    display: flex;
    align-items: center;
    gap: 10px;
    height: 25px;
  }
  .opt input {
    accent-color: var(--cue);
    width: 15px;
    height: 15px;
    margin: 0;
  }
  .dupes {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 12px;
  }
  .seg {
    display: flex;
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 2px;
    background: var(--booth);
  }
  .seg button {
    border: 0;
    background: none;
    height: 24px;
    padding: 0 10px;
    border-radius: 4px;
    color: var(--text-2);
    font-size: 12px;
  }
  .seg button.on {
    background: var(--panel-2);
    color: var(--text);
  }
  .preview {
    border: 1px solid var(--line);
    border-radius: 6px;
    background: var(--booth);
    padding: 10px 12px;
    font-size: 12px;
    color: var(--text-2);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .preview div {
    display: flex;
    align-items: center;
    gap: 6px;
    white-space: nowrap;
    overflow: hidden;
  }
  .in1 {
    padding-left: 18px;
  }
  .in2 {
    padding-left: 36px;
  }
  .err {
    color: var(--fault);
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--line);
  }
  .progress .bigbar {
    height: 8px;
    border-radius: 4px;
    background: var(--line);
    overflow: hidden;
    margin: 22px 0 10px;
  }
  .bigbar div {
    height: 100%;
    background: var(--cue);
    transition: width 0.15s linear;
  }
  .stats {
    display: flex;
    justify-content: space-between;
    color: var(--text-2);
  }
  .stats .readout {
    font-size: 16px;
    color: var(--text);
  }
  .current {
    margin: 10px 0 0;
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
    gap: 6px;
    padding: 30px 20px;
  }
</style>
