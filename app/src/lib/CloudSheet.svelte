<script lang="ts">
  import { Check, CircleCheck, CircleX, Cloud, Copy, LoaderCircle } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { api, cloudApi, type CloudAccount, type CloudPlaylist } from "./api";
  import { t } from "./i18n.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import { app } from "./state.svelte";

  const playlistId = app.cloudFor!;
  const list = $derived(app.library?.playlists.find((p) => p.id === playlistId));
  const trackCount = $derived(list?.trackIds.length ?? 0);

  /** `undefined` while asking. */
  let account = $state<CloudAccount | null | undefined>(undefined);
  let shared = $state<CloudPlaylist | null | undefined>(undefined);
  let busy = $state(false);
  let failed = $state<string | null>(null);
  let notice = $state<string | null>(null);

  let url = $state("");
  let username = $state("");
  let signInPassword = $state("");
  let password = $state("");
  let asking = $state(false);
  let copied = $state(false);
  let linkInput = $state<HTMLInputElement>();

  let jobId = $state<number | null>(null);
  const job = $derived(jobId != null ? app.jobs.find((j) => j.id === jobId) : undefined);
  const running = $derived(job?.state === "running");
  const host = (address: string) => address.replace(/^https?:\/\//, "");

  onMount(load);

  async function load() {
    failed = null;
    shared = undefined;
    try {
      account = await cloudApi.status();
      if (account) shared = await cloudApi.playlist(playlistId);
    } catch (e) {
      failed = String(e);
      // A refused sign-in is over: back to the form.
      account = await cloudApi.status().catch(() => null);
    }
  }

  /** Run a request; while it runs the buttons wait, and an error shows. */
  async function attempt(work: () => Promise<void>) {
    busy = true;
    failed = null;
    notice = null;
    try {
      await work();
    } catch (e) {
      failed = String(e);
      account = await cloudApi.status().catch(() => null);
    } finally {
      busy = false;
    }
  }

  const signIn = () =>
    attempt(async () => {
      account = await cloudApi.login(url, username, signInPassword);
      signInPassword = "";
      shared = await cloudApi.playlist(playlistId);
    });

  const signOut = () =>
    attempt(async () => {
      await cloudApi.logout();
      url = account?.url ?? "";
      username = account?.username ?? "";
      account = null;
      shared = undefined;
    });

  const start = () =>
    attempt(async () => {
      // A new playlist takes its password here; a shared one keeps what it has.
      jobId = await cloudApi.share(playlistId, shared ? null : password || null);
      password = "";
    });

  const setPassword = (value: string | null) =>
    attempt(async () => {
      shared = await cloudApi.setPassword(playlistId, value);
      password = "";
      notice = t(value ? "cloud.passwordSet" : "cloud.passwordRemoved");
    });

  const unshare = () =>
    attempt(async () => {
      await cloudApi.unshare(playlistId);
      shared = null;
      asking = false;
      notice = t("cloud.unshared");
    });

  async function copy() {
    if (!shared) return;
    try {
      await navigator.clipboard.writeText(shared.url);
      copied = true;
      setTimeout(() => (copied = false), 1800);
    } catch {
      // No clipboard for us: leave it selected for ⌘C.
      linkInput?.select();
    }
  }

  // The job ended: show what the server has now.
  let refreshed = false;
  $effect(() => {
    if (job && job.state !== "running" && !refreshed) {
      refreshed = true;
      cloudApi.playlist(playlistId).then((p) => (shared = p), (e) => (failed = String(e)));
    }
  });

  function back() {
    jobId = null;
    refreshed = false;
  }
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !running && (app.cloudFor = null)} />

<SheetFrame
  id="cloud-title"
  icon={Cloud}
  title={running ? t("cloud.titleRunning", { name: list?.name ?? "" }) : t("cloud.title")}
  sub={running ? undefined : list?.name}
  width={560}
  onclose={running ? undefined : () => (app.cloudFor = null)}
>
  <div class="body">
    {#if job}
      {#if job.state === "running"}
        <div class="bar"><div style:width="{(job.fraction ?? 0) * 100}%"></div></div>
        <p class="current">{job.current}</p>
        <p class="hint">{t("cloud.progress", { done: Math.min(job.filesDone + 1, job.filesTotal), total: job.filesTotal })}</p>
      {:else}
        <div class="result">
          {#if job.state === "done"}
            <span class="ic ok"><CircleCheck size={22} /></span>
          {:else if job.state === "error"}
            <span class="ic bad"><CircleX size={22} /></span>
            <h3>{t("cloud.failed")}</h3>
          {:else}
            <h3>{t("cloud.stoppedTitle")}</h3>
          {/if}
          <p>{job.message}</p>
        </div>
        {#if job.state === "done" && shared}
          {@render link(shared)}
        {/if}
      {/if}
    {:else if account === null}
      <p class="lead">{t("cloud.signInSub")}</p>
      <form id="cloud-sign-in" onsubmit={(e) => (e.preventDefault(), signIn())}>
        <label>
          <span>{t("cloud.address")}</span>
          <input type="text" bind:value={url} placeholder={t("cloud.addressHint")} autocomplete="off" autocapitalize="off" spellcheck="false" required />
        </label>
        <label>
          <span>{t("cloud.username")}</span>
          <input type="text" bind:value={username} autocomplete="off" autocapitalize="off" spellcheck="false" required />
        </label>
        <label>
          <span>{t("cloud.password")}</span>
          <input type="password" bind:value={signInPassword} required />
        </label>
      </form>
    {:else if account === undefined || shared === undefined}
      {#if !failed}<p class="wait"><LoaderCircle size={14} class="spin" /></p>{/if}
    {:else if shared === null}
      <p class="lead">{t("cloud.newSub")}</p>
      <label>
        <span>{t("cloud.newPassword")}</span>
        <input type="password" bind:value={password} autocomplete="new-password" />
      </label>
      <p class="hint">{t("cloud.newPasswordHint")}</p>
    {:else}
      {@render link(shared)}
      <section>
        <p class="lead">{t("cloud.sharedSub", { count: shared.tracks })}</p>
        <button class="btn" disabled={busy || trackCount === 0} onclick={start}>{t("cloud.update")}</button>
        <p class="hint">{t("cloud.updateHint")}</p>
      </section>
      <section>
        <p class="lead">{t(shared.hasPassword ? "cloud.passwordOn" : "cloud.passwordOff")}</p>
        <form class="inline" onsubmit={(e) => (e.preventDefault(), setPassword(password))}>
          <input type="password" bind:value={password} placeholder={t("cloud.newPasswordPlaceholder")} autocomplete="new-password" aria-label={t("cloud.newPasswordPlaceholder")} />
          <button class="btn" type="submit" disabled={busy || !password}>{t("cloud.setPassword")}</button>
          {#if shared.hasPassword}
            <button class="btn ghost" type="button" disabled={busy} onclick={() => setPassword(null)}>{t("cloud.removePassword")}</button>
          {/if}
        </form>
      </section>
      <section>
        {#if asking}
          <p class="lead">{t("cloud.unshareAsk")}</p>
          <div class="inline">
            <button class="btn danger" disabled={busy} onclick={unshare}>{t("cloud.unshareConfirm")}</button>
            <button class="btn ghost" disabled={busy} onclick={() => (asking = false)}>{t("common.cancel")}</button>
          </div>
        {:else}
          <button class="btn ghost" disabled={busy} onclick={() => (asking = true)}>{t("cloud.unshare")}</button>
        {/if}
      </section>
    {/if}
    {#if notice}<p class="notice" role="status">{notice}</p>{/if}
    {#if failed}<p class="error" role="alert">{failed}</p>{/if}
  </div>

  {#snippet foot()}
    {#if job}
      {#if job.state === "running"}
        <button class="btn lg" onclick={() => api.cancelJob(job.id)}>{t("common.stop")}</button>
      {:else}
        {#if job.state !== "done"}<button class="btn ghost lg" onclick={back}>{t("cloud.backToShare")}</button>{/if}
        <button class="btn primary lg" onclick={() => (app.cloudFor = null)}>{t("common.close")}</button>
      {/if}
    {:else}
      {#if account}
        <span class="who">{t("cloud.signedInAs", { name: account.username, host: host(account.url) })}</span>
        <button class="btn ghost" disabled={busy} onclick={signOut}>{t("cloud.signOut")}</button>
      {/if}
      {#if account === null}
        <button class="btn primary lg" type="submit" form="cloud-sign-in" disabled={busy}>{t("cloud.signIn")}</button>
      {:else if account && shared === null}
        <button class="btn primary lg" disabled={busy || trackCount === 0} onclick={start}>
          {t("cloud.share", { count: trackCount })}
        </button>
      {:else if account && shared}
        <button class="btn primary lg" onclick={() => (app.cloudFor = null)}>{t("common.close")}</button>
      {/if}
    {/if}
  {/snippet}
</SheetFrame>

{#snippet link(p: CloudPlaylist)}
  <label>
    <span>{t("cloud.link")}</span>
    <span class="inline">
      <input class="url" type="text" readonly value={p.url} bind:this={linkInput} onfocus={(e) => e.currentTarget.select()} />
      <button class="btn primary" onclick={copy}>
        {#if copied}<Check size={15} />{t("cloud.copied")}{:else}<Copy size={15} />{t("cloud.copy")}{/if}
      </button>
    </span>
  </label>
{/snippet}

<style>
  .body {
    display: flex;
    flex-direction: column;
    gap: 14px;
    padding: 22px 26px 24px;
    overflow-y: auto;
  }
  section {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding-top: 14px;
    box-shadow: inset 0 1px 0 var(--hairline);
  }
  form:not(.inline) {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  label > span:first-child {
    font-size: 12.5px;
    color: var(--text-2);
  }
  input {
    width: 100%;
    height: 36px;
  }
  .inline {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
    width: 100%;
  }
  .inline input {
    flex: 1;
    width: auto;
    min-width: 180px;
  }
  .url {
    user-select: text;
    -webkit-user-select: text;
    font-size: 12.5px;
  }
  .lead,
  .hint,
  .current,
  .notice,
  .error {
    margin: 0;
    line-height: 1.5;
  }
  .lead {
    font-size: 13px;
    color: var(--text-2);
  }
  .hint {
    font-size: 12px;
    color: var(--text-3);
  }
  .notice {
    font-size: 12.5px;
    color: var(--success);
  }
  .error {
    font-size: 12.5px;
    color: var(--danger);
    user-select: text;
    -webkit-user-select: text;
  }
  .wait {
    display: flex;
    justify-content: center;
    margin: 12px 0;
    color: var(--text-3);
  }
  .bar {
    height: 8px;
    border-radius: 4px;
    background: var(--surface-3);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    border-radius: 4px;
    background: var(--accent);
    transition: width 0.15s linear;
  }
  .current {
    font-size: 13px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .result {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    text-align: center;
  }
  .result h3 {
    margin: 0;
    font-size: 16px;
  }
  .result p {
    margin: 0;
    color: var(--text-2);
    line-height: 1.5;
    user-select: text;
    -webkit-user-select: text;
  }
  .ic {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    background: var(--success-soft);
    color: var(--success);
  }
  .ic.bad {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .who {
    margin-right: auto;
    min-width: 0;
    overflow: hidden;
    font-size: 12px;
    color: var(--text-3);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
