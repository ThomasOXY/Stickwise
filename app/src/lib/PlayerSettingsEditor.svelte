<script lang="ts">
  import { AudioWaveform, CircleCheck, Disc3, Lightbulb, Monitor, RotateCcw, SlidersHorizontal, SlidersVertical } from "@lucide/svelte";
  import type { Component } from "svelte";
  import { playerApi, type PlayerSettings, type SettingField, type SettingValues } from "./api";
  import { num } from "./format";
  import { t, type Key } from "./i18n.svelte";
  import { app } from "./state.svelte";

  /**
   * `stick`: a stick path, or `null` for the DJ's own settings kept in the app.
   * `onclose` shows a close button; `busy` and `source` are for the frame.
   */
  let {
    stick,
    onclose,
    busy = $bindable(false),
    source = $bindable(null),
  }: {
    stick: string | null;
    onclose?: () => void;
    busy?: boolean;
    source?: string | null;
  } = $props();

  let fields = $state<SettingField[]>([]);
  let loaded = $state<PlayerSettings | null>(null);
  let values = $state<SettingValues>({});
  let failed = $state<string | null>(null);
  let done = $state<string | null>(null);

  const stickName = $derived(stick ? (app.sticks.find((s) => s.path === stick)?.name ?? stick.split("/").pop()) : null);

  const GROUPS: [string, Key][] = [
    ["player", "player.groups.player"],
    ["waveform", "player.groups.waveform"],
    ["display", "player.groups.display"],
    ["light", "player.groups.light"],
    ["mixer", "player.groups.mixer"],
  ];

  const ICONS: Record<string, Component<{ size?: number }>> = {
    player: Disc3,
    waveform: AudioWaveform,
    display: Monitor,
    light: Lightbulb,
    mixer: SlidersVertical,
  };
  const groups = $derived(
    GROUPS.map(([g, title]) => ({ g, title, list: fields.filter((f) => f.group === g) })).filter((x) => x.list.length),
  );
  let active = $state("player");
  const current = $derived(groups.find((x) => x.g === active) ?? groups[0]);

  const SOURCES: Record<PlayerSettings["source"], Key> = {
    stick: "player.sources.stick",
    profile: "player.sources.profile",
    rekordbox: "player.sources.rekordbox",
    default: "player.sources.default",
  };

  $effect(() => {
    const s = stick;
    Promise.all([playerApi.schema(), playerApi.get(s)])
      .then(([f, l]) => {
        fields = f;
        loaded = l;
        values = { ...l.values };
      })
      .catch((e) => (failed = String(e)));
  });

  const changed = $derived(loaded ? Object.keys(values).filter((k) => values[k] !== loaded!.values[k]).length : 0);

  function set(key: string, v: number) {
    values = { ...values, [key]: v };
    done = null;
  }

  function defaults() {
    values = Object.fromEntries(fields.map((f) => [f.key, f.default]));
    done = null;
  }

  async function save() {
    busy = true;
    failed = null;
    try {
      if (stick) {
        done = await playerApi.write(stick, values);
      } else {
        await playerApi.save(values);
        done = t("player.saved");
      }
      loaded = { ...loaded!, values: { ...values }, profile: { ...values } };
    } catch (e) {
      failed = String(e);
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    source = loaded ? t(SOURCES[loaded.source]) : null;
  });
</script>

<div class="editor">
  <div class="cols">
    <nav aria-label={t("player.groupsLabel")}>
      {#each groups as x (x.g)}
        {@const Icon = ICONS[x.g] ?? SlidersHorizontal}
        <button class:on={current?.g === x.g} onclick={() => (active = x.g)}>
          <Icon size={16} />
          <span>{t(x.title)}</span>
          <em>{x.list.length}</em>
        </button>
      {/each}
    </nav>

    <div class="pane">
      {#if loaded?.playerSaved.length}
        <p class="note">
          {t("player.playerSaved", { list: loaded.playerSaved.join(", ") })}
        </p>
      {/if}
      {#if !loaded && !failed}
        <p class="wait">{t("common.loading")}</p>
      {/if}

      {#if current && loaded}
        <h3>{t(current.title)}</h3>
        <div class="rows">
          {#each current.list as f (f.key)}
            {@const differs = stick && loaded.source === "stick" && values[f.key] !== loaded.profile[f.key]}
            <div class="row">
              <span class="label" title={differs ? t("player.differs") : undefined}>
                {f.label}{#if differs}<span class="diff"></span>{/if}
              </span>
              {#if f.options.length > 5}
                <select value={values[f.key]} onchange={(e) => set(f.key, Number(e.currentTarget.value))} aria-label={f.label}>
                  {#each f.options as o (o.value)}
                    <option value={o.value}>{o.label}</option>
                  {/each}
                </select>
              {:else}
                <div class="seg" role="radiogroup" aria-label={f.label}>
                  {#each f.options as o (o.value)}
                    <button
                      role="radio"
                      aria-checked={values[f.key] === o.value}
                      class:on={values[f.key] === o.value}
                      onclick={() => set(f.key, o.value)}>{o.label}</button
                    >
                  {/each}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {/if}

      {#if failed}<p class="err">{failed}</p>{/if}
    </div>
  </div>

<footer>
  <button class="btn ghost lg" onclick={defaults} disabled={!loaded || busy}><RotateCcw size={15} /> {t("common.default")}</button>
  {#if done}
    <span class="status ok"><CircleCheck size={14} /> {done}</span>
  {:else if changed}
    <span class="status">{t("player.changes", { count: num(changed) })}</span>
  {/if}
  <span class="grow"></span>
  {#if onclose}<button class="btn ghost lg" onclick={onclose} disabled={busy}>{t("common.close")}</button>{/if}
  <button class="btn primary lg" onclick={save} disabled={!loaded || busy}>
    {stick ? t("player.writeTo", { name: stickName ?? "" }) : t("common.save")}
  </button>
</footer>
</div>

<style>
  .editor {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .cols {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr);
  }
  nav {
    background: var(--surface-2);
    padding: 16px 12px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    overflow-y: auto;
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
  nav span {
    flex: 1;
  }
  nav em {
    font-style: normal;
    font-family: var(--font-readout);
    font-weight: 600;
    font-size: 12px;
    color: var(--text-3);
  }
  .pane {
    overflow-y: auto;
    padding: 20px 28px 24px;
    min-height: 420px;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 16px;
    font-weight: 600;
  }
  .rows {
    display: flex;
    flex-direction: column;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    min-height: 58px;
    padding: 8px 0;
    box-shadow: inset 0 -1px 0 var(--hairline);
  }
  .row:last-child {
    box-shadow: none;
  }
  .label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13.5px;
    font-weight: 500;
  }
  .diff {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--accent);
  }
  .seg {
    flex: none;
  }
  select {
    height: 34px;
    min-width: 160px;
    padding: 0 10px;
    border: 0;
    border-radius: var(--r-control);
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
  }
  .note {
    margin: 0 0 14px;
    padding: 10px 14px;
    border-radius: var(--r-panel);
    background: var(--warning-soft);
    color: var(--warning);
    font-size: 12.5px;
    line-height: 1.5;
  }
  .wait {
    color: var(--text-3);
  }
  .err {
    color: var(--danger);
  }
  .status {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    font-size: 12.5px;
    color: var(--text-2);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .status.ok {
    color: var(--success);
  }
  footer {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    padding: 16px 24px;
    box-shadow: inset 0 1px 0 var(--hairline);
  }
  .grow {
    flex: 1;
  }
</style>
