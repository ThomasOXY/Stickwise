<script lang="ts">
  import { CircleAlert, CircleCheck, CircleSlash, LoaderCircle, X } from "@lucide/svelte";
  import { api, type JobEvent } from "./api";
  import { app } from "./state.svelte";

  /** The jobs, running ones first, then the finished ones newest first. */
  const running = $derived(app.jobs.filter((j) => j.state === "running"));
  const finished = $derived(app.jobs.filter((j) => j.state !== "running").reverse());

  const share = (j: JobEvent) =>
    Math.min(1, j.fraction ?? (j.filesTotal ? j.filesDone / j.filesTotal : j.bytesDone / Math.max(1, j.bytesTotal)));

  function clear() {
    app.jobs = app.jobs.filter((j) => j.state === "running");
  }

  const close = () => (app.jobsOpen = false);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && close()} />

<div class="panel" role="region" aria-label="Úlohy">
  <header>
    <h1>Úlohy</h1>
    {#if finished.length}<button class="btn ghost sm" onclick={clear}>Vymazat hotové</button>{/if}
    <button class="btn icon ghost" onclick={close} aria-label="Zpátky na playlisty" title="Zpátky na playlisty"><X size={15} /></button>
  </header>

  <div class="list">
    {#if app.jobs.length === 0}
      <p class="empty">Nic neběží.</p>
    {/if}
    {#each running as j (j.id)}
      <div class="job">
        <div class="row">
          <LoaderCircle size={15} class="spin run" />
          <span class="label">{j.label}</span>
          <span class="readout count">{j.filesDone} / {j.filesTotal}</span>
          <button class="btn ghost sm" onclick={() => api.cancelJob(j.id)}>Zastavit</button>
        </div>
        <div class="bar"><span style:width="{share(j) * 100}%"></span></div>
        {#if j.current}<p class="current">{j.current}</p>{/if}
      </div>
    {/each}
    {#each finished as j (j.id)}
      <div class="job done">
        <div class="row">
          {#if j.state === "done"}
            <CircleCheck size={15} class="ok" />
          {:else if j.state === "cancelled"}
            <CircleSlash size={15} class="muted" />
          {:else}
            <CircleAlert size={15} class="bad" />
          {/if}
          <span class="label">{j.label}</span>
        </div>
        {#if j.message}<p class="current" class:bad={j.state === "error"}>{j.message}</p>{/if}
      </div>
    {/each}
  </div>
</div>

<style>
  .panel {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  header {
    flex: none;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 4px 10px 10px;
  }
  h1 {
    flex: 1;
    margin: 0;
    font-size: 17px;
    font-weight: 600;
  }
  .list {
    overflow-y: auto;
    padding: 6px 0;
  }
  .empty {
    margin: 0;
    padding: 4px 10px;
    color: var(--text-3);
  }
  .job {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 10px;
  }
  .job + .job {
    box-shadow: inset 0 1px 0 var(--hairline);
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 28px;
  }
  .label {
    flex: 1;
    min-width: 0;
    font-size: 13.5px;
    font-weight: 500;
  }
  .done .label {
    color: var(--text-2);
  }
  .count {
    font-size: 15px;
    color: var(--text-2);
  }
  .bar {
    height: 5px;
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
  .current {
    margin: 0;
    font-size: 12px;
    color: var(--text-3);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .current.bad {
    white-space: normal;
    color: var(--danger);
  }
  .panel :global(.run) {
    color: var(--accent);
  }
  .panel :global(.ok) {
    color: var(--success);
  }
  .panel :global(.bad) {
    color: var(--danger);
  }
  .panel :global(.muted) {
    color: var(--text-3);
  }
</style>
