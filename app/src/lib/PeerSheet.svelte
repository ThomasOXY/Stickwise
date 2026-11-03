<script lang="ts">
  import { ArrowLeftRight, CircleCheck, Folder, Laptop, Link2Off, ListMusic, LoaderCircle, Send, Wifi, WifiOff } from "@lucide/svelte";
  import { listen } from "@tauri-apps/api/event";
  import { ask } from "@tauri-apps/plugin-dialog";
  import { onMount } from "svelte";
  import { peerApi, type PairResult, type Peer, type PeerList, type PeerSettings, type Playlist } from "./api";
  import { plural, size } from "./format";
  import SheetCheck from "./SheetCheck.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import { openSettings } from "./prefs.svelte";
  import { app } from "./state.svelte";

  let s = $state<PeerSettings | null>(null);
  let list = $state<PeerList | null>(null);
  let error = $state("");
  /** Pairing this Mac asked for: the code to compare, then the answer. */
  let pairing = $state<{ id: string; name: string; code: string; result?: PairResult } | null>(null);
  /** Connection checks: peer id → "…", milliseconds or the error. */
  let checks = $state<Record<string, string>>({});
  /** Choosing what to send to this Mac. */
  let sendTo = $state<Peer | null>(null);
  let picked = $state<Set<number>>(new Set());

  const lists = $derived(app.library?.playlists ?? []);
  const children = (parent: number) => lists.filter((p) => p.parentId === parent).sort((a, b) => a.sortOrder - b.sortOrder);

  /** The library's playlists as a tree, sets first. */
  const rows = $derived.by(() => {
    const out: { p: Playlist; depth: number }[] = [];
    const walk = (parent: number, depth: number) => {
      for (const p of children(parent)) {
        out.push({ p, depth });
        walk(p.id, depth + 1);
      }
    };
    const top = children(0);
    for (const p of [...top.filter((p) => p.isSet), ...top.filter((p) => !p.isSet)]) {
      out.push({ p, depth: 0 });
      walk(p.id, 1);
    }
    return out;
  });

  const ancestors = (p: Playlist): number[] => {
    const up: number[] = [];
    let at = lists.find((x) => x.id === p.parentId);
    while (at) {
      up.push(at.id);
      at = lists.find((x) => x.id === at!.parentId);
    }
    return up;
  };
  const descendants = (id: number): Playlist[] => children(id).flatMap((c) => [c, ...descendants(c.id)]);

  function rowState(p: Playlist): "on" | "off" | "some" {
    if (picked.has(p.id) || ancestors(p).some((a) => picked.has(a))) return "on";
    return descendants(p.id).some((d) => picked.has(d.id)) ? "some" : "off";
  }

  function toggle(p: Playlist) {
    if (ancestors(p).some((a) => picked.has(a))) return;
    const next = new Set(picked);
    if (next.has(p.id)) next.delete(p.id);
    else {
      next.add(p.id);
      for (const d of descendants(p.id)) next.delete(d.id);
    }
    picked = next;
  }

  const sendIds = $derived.by(() => {
    const ids = new Set<number>();
    for (const id of picked) {
      const p = lists.find((x) => x.id === id);
      if (!p) continue;
      for (const l of [p, ...descendants(id)]) for (const t of l.trackIds) ids.add(t);
    }
    return ids;
  });
  const sendBytes = $derived.by(() => {
    const sizes = new Map((app.library?.tracks ?? []).map((t) => [t.id, t.fileSize]));
    let n = 0;
    for (const id of sendIds) n += sizes.get(id) ?? 0;
    return n;
  });

  async function mirror(p: Peer) {
    error = "";
    try {
      await peerApi.mirror(p.id);
      app.notice = `Porovnávám knihovnu s ${p.name}. Než se cokoli změní, ukážu, co se stane.`;
      app.peerOpen = false;
    } catch (e) {
      error = String(e);
    }
  }

  async function startSend() {
    if (!sendTo) return;
    error = "";
    try {
      await peerApi.send(sendTo.id, [...picked], []);
      app.notice = `Posílám na ${sendTo.name}. Průběh je v úlohách, ${sendTo.name} to musí přijmout.`;
      app.peerOpen = false;
    } catch (e) {
      error = String(e);
    }
  }

  /** Nothing left to fingerprint now: the rest is skipped or on a drive not plugged in. */
  const ready = $derived(!!s && s.fingerprinted + s.skipped + s.waiting >= s.total);
  const pct = $derived(s && s.total ? ((s.fingerprinted + s.skipped) / s.total) * 100 : 0);

  /** Playlists chosen from a playlist's menu, kept until a Mac is picked. */
  const preset = app.peerSendPreset;
  app.peerSendPreset = null;
  const presetNames = $derived((preset ?? []).map((id) => lists.find((p) => p.id === id)?.name).filter(Boolean).join(", "));

  const refresh = () =>
    peerApi.list().then((v) => {
      list = v;
      // One Mac to send to: straight to it.
      const ready = v.peers.filter((p) => p.paired && p.online);
      if (preset && !sendTo && ready.length === 1) choose(ready[0]);
    });

  function choose(p: Peer) {
    sendTo = p;
    picked = new Set(preset ?? []);
  }

  onMount(() => {
    peerApi.settings().then((v) => (s = v), (e) => (error = String(e)));
    refresh();
    const offs = [
      listen<{ fingerprinted: number; total: number; skipped: number; waiting: number }>("peer-fingerprints", (e) => {
        if (s) s = { ...s, ...e.payload };
      }),
      listen("peer-peers", refresh),
      listen<PairResult>("peer-pair-result", (e) => {
        if (pairing?.id === e.payload.peer.id) pairing = { ...pairing, result: e.payload };
        refresh();
      }),
    ];
    return () => offs.forEach((o) => o.then((f) => f()));
  });

  async function pair(id: string, name: string) {
    error = "";
    try {
      pairing = { id, name, code: await peerApi.pair(id) };
    } catch (e) {
      error = String(e);
    }
  }

  async function check(id: string) {
    checks[id] = "…";
    try {
      checks[id] = `Spojení funguje · ${await peerApi.ping(id)} ms`;
    } catch (e) {
      checks[id] = String(e);
    }
  }

  async function unpair(id: string, name: string) {
    const sure = await ask(`Knihovny se pak nebudou synchronizovat, dokud Macy znovu nespárujete.`, {
      title: `Zrušit spárování s ${name}?`,
      kind: "warning",
      okLabel: "Zrušit spárování",
      cancelLabel: "Nechat",
    });
    if (!sure) return;
    await peerApi.unpair(id);
    delete checks[id];
    refresh();
  }
</script>

<SheetFrame
  id="peer-title"
  icon={Laptop}
  title="Sync mezi Macy"
  sub={list ? `Tento Mac: ${list.name}` : "Knihovna s dalšími Stickwise ve stejné síti"}
  width={620}
  onclose={() => (app.peerOpen = false)}
>
  <div class="body">
    {#if sendTo}
      <section class="send">
        <div class="sec-head">
          <span class="eyebrow">Co poslat na {sendTo.name}</span>
          <span class="count">
            {sendIds.size} {plural(sendIds.size, "skladba", "skladby", "skladeb")} · {size(sendBytes)}
          </span>
        </div>
        {#if rows.length}
          <div class="pick">
            {#each rows as { p, depth } (p.id)}
              <button class="row" style:padding-left="{10 + depth * 18}px" onclick={() => toggle(p)}>
                <SheetCheck state={rowState(p)} />
                {#if p.isFolder}<Folder size={14} />{:else}<ListMusic size={14} />{/if}
                <span class="pname">{p.name}</span>
                {#if p.isSet}<span class="tag">set</span>{/if}
                <span class="pcount">{p.isFolder ? "" : p.trackIds.length}</span>
              </button>
            {/each}
          </div>
        {:else}
          <div class="empty">V knihovně nejsou žádné playlisty ani sety.</div>
        {/if}
        <p class="hint">
          Druhý Mac pošle jen to, co mu chybí. Skladby, které už má, se nepřenášejí; přijdou k nim jen novější cue, gridy a tagy.
        </p>
      </section>
    {:else}
      <section>
        <p class="more">
          Kam se ukládá hudba z jiných Maců a automatické zrcadlení najdeš v<button class="link" onclick={() => openSettings("peer")}
            >Nastavení</button
          >.
        </p>
      </section>

      <section>
        <div class="eyebrow">Příprava knihovny</div>
        {#if s}
          {#if ready && s.fingerprinted >= s.total}
            <p class="ok"><CircleCheck size={16} />Všech {s.total} {plural(s.total, "skladba má", "skladby mají", "skladeb má")} otisk, knihovna je připravená.</p>
          {:else if ready}
            <p class="ok"><CircleCheck size={16} />Knihovna je připravená, otisk má {s.fingerprinted} z {s.total} skladeb.</p>
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
          {#if s.skipped}
            <p class="hint">
              {s.skipped} {plural(s.skipped, "skladba se nedá přečíst a je přeskočená", "skladby se nedají přečíst a jsou přeskočené", "skladeb se nedá přečíst a jsou přeskočené")}.
              Zkusí se znovu, když se soubor změní nebo po restartu.
            </p>
          {/if}
          {#if s.waiting}
            <p class="hint">
              {s.waiting} {plural(s.waiting, "skladba čeká", "skladby čekají", "skladeb čeká")} na připojení disku.
            </p>
          {/if}
        {/if}
      </section>

      <section>
        <div class="eyebrow">Macy</div>
        {#if preset}
          <p class="hint">Vyber Mac, kam poslat „{presetNames}“. Posílat jde jen na spárovaný Mac ve stejné síti.</p>
        {/if}
        {#if pairing}
          <div class="pairing">
            {#if !pairing.result}
              <div class="code readout">{pairing.code}</div>
              <p><LoaderCircle size={14} class="spin" />Na <strong>{pairing.name}</strong> potvrď, že ukazuje stejný kód.</p>
            {:else}
              <p class:ok={pairing.result.accepted} class:err={!pairing.result.accepted}>
                {#if pairing.result.accepted}<CircleCheck size={16} />{/if}{pairing.result.message}
              </p>
              <button class="btn" onclick={() => (pairing = null)}>OK</button>
            {/if}
          </div>
        {/if}
        {#if list?.error}
          <div class="empty err"><WifiOff size={18} />{list.error}</div>
        {:else if list && list.peers.length === 0}
          <div class="empty">
            <LoaderCircle size={18} class="spin" />Hledám další Macy… Na druhém Macu otevři Stickwise ve stejné síti.
          </div>
        {:else if list}
          <div class="peers">
            {#each list.peers as p (p.id)}
              <div class="peer">
                <span class="icon" class:off={!p.online}><Laptop size={18} /></span>
                <div class="who">
                  <span class="name">{p.name}</span>
                  <span class="status">
                    {#if checks[p.id]}{checks[p.id]}{:else if p.paired}Spárováno · {p.online ? "v síti" : "mimo síť"}{:else}Nespárováno{/if}
                  </span>
                </div>
                {#if p.paired}
                  {#if p.online}
                    <button class="btn icon ghost" title="Ověřit spojení" aria-label="Ověřit spojení" onclick={() => check(p.id)} disabled={checks[p.id] === "…"}>
                      <Wifi size={15} />
                    </button>
                    <button class="btn" title="Obě knihovny budou stejné" onclick={() => mirror(p)}><ArrowLeftRight size={14} />Zrcadlit</button>
                    <button class="btn primary" onclick={() => choose(p)}><Send size={14} />Poslat…</button>
                  {/if}
                  <button class="btn icon ghost" title="Zrušit spárování" aria-label="Zrušit spárování" onclick={() => unpair(p.id, p.name)}>
                    <Link2Off size={15} />
                  </button>
                {:else}
                  <button class="btn primary" onclick={() => pair(p.id, p.name)} disabled={!!pairing && !pairing.result}>Spárovat</button>
                {/if}
              </div>
            {/each}
          </div>
        {/if}
      </section>

    {/if}

    {#if error}<p class="err">{error}</p>{/if}
  </div>

  {#snippet foot()}
    {#if sendTo}
      <button class="btn ghost lg" onclick={() => (sendTo = null)}>Zpět</button>
      <button class="btn primary lg" onclick={startSend} disabled={sendIds.size === 0 && picked.size === 0}>
        <Send size={16} />Poslat na {sendTo.name}
      </button>
    {:else}
      <button class="btn primary lg" onclick={() => (app.peerOpen = false)}>Hotovo</button>
    {/if}
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
  .more {
    margin: 0;
    color: var(--text-3);
    font-size: 12.5px;
  }
  .link {
    border: 0;
    background: none;
    color: var(--accent);
    padding: 0 0 0 4px;
    font: inherit;
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
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
  .sec-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
  }
  .sec-head .eyebrow {
    margin: 0;
  }
  .count {
    font-size: 12px;
    color: var(--text-3);
  }
  .pick {
    max-height: 380px;
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
    height: 34px;
    flex: none;
    padding-right: 12px;
    border: 0;
    border-radius: var(--r-control);
    background: none;
    color: var(--text-2);
    font: inherit;
    font-size: 13.5px;
    text-align: left;
    cursor: pointer;
  }
  .row:hover {
    background: var(--surface-3);
  }
  .pname {
    flex: 1;
    min-width: 0;
    color: var(--text);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tag {
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11px;
  }
  .pcount {
    font-size: 12px;
    color: var(--text-3);
  }
  .peers {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px;
    border-radius: var(--r-panel);
    background: var(--surface-2);
  }
  .peer {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 8px 8px 10px;
  }
  .peer .icon {
    color: var(--accent);
  }
  .peer .icon.off {
    color: var(--text-3);
  }
  .who {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .name {
    font-size: 14px;
    color: var(--text);
  }
  .status {
    font-size: 12px;
    color: var(--text-3);
  }
  .pairing {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 18px;
    border-radius: var(--r-panel);
    background: var(--surface-2);
  }
  .pairing p {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13.5px;
    color: var(--text-2);
  }
  .pairing strong {
    color: var(--text);
  }
  .code {
    font-size: 34px;
    letter-spacing: 0.08em;
    color: var(--accent);
  }
  .empty.err {
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
