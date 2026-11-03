<script lang="ts">
  import { ArrowRight, CircleCheck, Download, LoaderCircle, RotateCw, Sparkles, TriangleAlert } from "@lucide/svelte";
  import { date } from "./format";
  import { t } from "./i18n.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import { updates as u } from "./updates.svelte";

  type Inline = { text: string; bold?: boolean; code?: boolean };
  type Block = { kind: "h" | "li" | "p"; parts: Inline[] };

  /** The bit of Markdown release notes use: headings, bullets, **bold**, `code`. No HTML. */
  function parse(md: string): Block[] {
    const blocks: Block[] = [];
    for (const raw of md.split("\n")) {
      let line = raw.trim();
      if (!line || line.startsWith("**Full Changelog**")) continue;
      // GitHub's generated notes: "feat: x by @who in https://…/pull/12".
      line = line.replace(/ by @[\w-]+ in https?:\/\/\S+$/, "").replace(/ in https?:\/\/\S+$/, "");
      let kind: Block["kind"] = "p";
      if (/^#{1,6} /.test(line)) {
        kind = "h";
        line = line.replace(/^#+ /, "");
      } else if (/^[-*] /.test(line)) {
        kind = "li";
        line = line.slice(2);
      }
      const parts: Inline[] = [];
      for (const [i, seg] of line.split(/(`[^`]+`)/).entries()) {
        if (i % 2) parts.push({ text: seg.slice(1, -1), code: true });
        else for (const [j, s] of seg.split(/\*\*(.+?)\*\*/).entries()) if (s) parts.push({ text: s, bold: j % 2 === 1 });
      }
      blocks.push({ kind, parts });
    }
    return blocks;
  }

  const mb = (b: number) => (b / 1024 / 1024).toFixed(1);

  const notes = $derived(parse(u.installed?.notes ?? u.update?.body ?? ""));
  const pct = $derived(u.total ? Math.min(100, (u.got / u.total) * 100) : 0);
  const busy = $derived(u.phase === "downloading");

  const title = $derived(
    u.installed
      ? t("shell.update.updatedTo", { version: u.installed.version })
      : u.update && u.available
        ? t("shell.update.newVersion", { version: u.update.version })
        : t("shell.update.title"),
  );
  const sub = $derived(
    u.installed
      ? t("shell.update.whatsNew")
      : u.update && u.available
        ? [t("shell.update.youHave", { version: u.current }), date(u.update.date ?? "")].filter(Boolean).join(" · ")
        : `Stickwise ${u.current}`,
  );
</script>

<SheetFrame id="update-title" icon={Sparkles} {title} {sub} width={560} onclose={busy ? undefined : () => u.close()}>
  {#snippet badge()}
    {#if u.update && u.available && !u.installed}
      <span class="versions readout">{u.current}<ArrowRight size={13} />{u.update.version}</span>
    {/if}
  {/snippet}

  <div class="body">
    {#if u.installed || (u.update && u.available)}
      {#if notes.length}
        <div class="notes">
          {#each notes as b, i (i)}
            <svelte:element this={b.kind === "h" ? "h3" : b.kind === "li" ? "div" : "p"} class:li={b.kind === "li"}>
              {#each b.parts as p, k (k)}{#if p.code}<code>{p.text}</code>{:else if p.bold}<strong>{p.text}</strong>{:else}{p.text}{/if}{/each}
            </svelte:element>
          {/each}
        </div>
      {:else}
        <p class="quiet">{t("shell.update.noNotes")}</p>
      {/if}

      {#if u.phase === "downloading"}
        <div class="progress">
          <div class="bar"><span style:width="{u.total ? pct : 100}%" class:indet={!u.total}></span></div>
          <span class="readout">{u.total ? `${mb(u.got)} / ${mb(u.total)} MB` : `${mb(u.got)} MB`}</span>
        </div>
      {:else if u.phase === "ready"}
        <p class="ok"><CircleCheck size={16} />{t("shell.update.installed")}</p>
      {/if}
      {#if u.error && !u.installed}
        <p class="err"><TriangleAlert size={16} />{u.error}</p>
      {/if}
    {:else if u.phase === "checking"}
      <p class="state"><LoaderCircle size={18} class="spin" />{t("shell.update.checking")}</p>
    {:else if u.phase === "error"}
      <p class="state err"><TriangleAlert size={18} />{t("shell.update.checkFailed")}</p>
      <p class="quiet small">{u.error}</p>
    {:else}
      <p class="state"><CircleCheck size={18} class="good" />{t("shell.update.upToDate")}</p>
    {/if}
  </div>

  {#snippet foot()}
    {#if u.installed}
      <button class="btn primary lg" onclick={() => u.close()}>{t("common.continue")}</button>
    {:else if u.phase === "available"}
      <button class="btn ghost lg" onclick={() => u.skip()}>{t("common.later")}</button>
      <button class="btn primary lg" onclick={() => u.install()}><Download size={16} />{t("shell.update.install")}</button>
    {:else if u.phase === "downloading"}
      <button class="btn primary lg" disabled><LoaderCircle size={16} class="spin" />{t("shell.update.downloading")}</button>
    {:else if u.phase === "ready"}
      <button class="btn ghost lg" onclick={() => u.close()}>{t("common.later")}</button>
      <button class="btn primary lg" onclick={() => u.restart()}><RotateCw size={16} />{t("common.restart")}</button>
    {:else}
      <button class="btn lg" onclick={() => u.check()} disabled={u.phase === "checking"}><RotateCw size={16} />{t("common.checkAgain")}</button>
      <button class="btn primary lg" onclick={() => u.close()}>{t("common.done")}</button>
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
    gap: 16px;
  }
  .versions {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 15px;
  }
  .notes {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 14px;
    line-height: 1.5;
    color: var(--text);
  }
  .notes h3 {
    margin: 10px 0 2px;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .notes h3:first-child {
    margin-top: 0;
  }
  .notes p {
    margin: 0;
    color: var(--text-2);
  }
  .li {
    position: relative;
    padding-left: 18px;
  }
  .li::before {
    content: "";
    position: absolute;
    left: 4px;
    top: 0.62em;
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: var(--accent);
  }
  code {
    padding: 1px 5px;
    border-radius: 4px;
    background: var(--surface-3);
    font-size: 12.5px;
  }
  .progress {
    display: flex;
    align-items: center;
    gap: 12px;
    color: var(--text-2);
    font-size: 15px;
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
    transition: width 0.2s linear;
  }
  .bar span.indet {
    width: 30% !important;
    animation: slide 1.1s ease-in-out infinite;
  }
  @keyframes slide {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
  .state,
  .ok,
  .err {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .state {
    font-size: 15px;
    padding: 8px 0;
  }
  .ok {
    color: var(--success);
    font-size: 13px;
  }
  .err {
    color: var(--danger);
    font-size: 13px;
  }
  .quiet {
    margin: 0;
    color: var(--text-3);
  }
  .small {
    font-size: 12px;
    word-break: break-word;
  }
  .body :global(.good) {
    color: var(--success);
  }
</style>
