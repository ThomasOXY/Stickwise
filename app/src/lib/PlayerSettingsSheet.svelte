<script lang="ts">
  import { CircleCheck, RotateCcw } from "@lucide/svelte";
  import { playerApi, type PlayerSettings, type SettingField, type SettingValues } from "./api";
  import { app } from "./state.svelte";

  /** A stick path, or `null` for the DJ's own settings kept in the app. */
  let { stick }: { stick: string | null } = $props();

  let fields = $state<SettingField[]>([]);
  let loaded = $state<PlayerSettings | null>(null);
  let values = $state<SettingValues>({});
  let busy = $state(false);
  let failed = $state<string | null>(null);
  let done = $state<string | null>(null);

  const stickName = $derived(stick ? (app.sticks.find((s) => s.path === stick)?.name ?? stick.split("/").pop()) : null);

  const GROUPS: [string, string][] = [
    ["player", "Přehrávání"],
    ["waveform", "Waveform a tónina"],
    ["display", "Displej"],
    ["light", "Osvětlení"],
    ["mixer", "Mixer (DJM)"],
  ];

  const SOURCES: Record<PlayerSettings["source"], string> = {
    stick: "Načteno z flashky",
    profile: "Tvoje uložené nastavení",
    rekordbox: "Převzato z rekordboxu",
    default: "Výchozí hodnoty",
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
        done = "Uloženo. Zapíše se na flashku při synchronizaci a formátování.";
      }
      loaded = { ...loaded!, values: { ...values }, profile: { ...values } };
    } catch (e) {
      failed = String(e);
    } finally {
      busy = false;
    }
  }

  const close = () => (app.settingsFor = undefined);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !busy && close()} />

<div class="dim"></div>
<div class="sheet" role="dialog" aria-modal="true" aria-labelledby="ps-title">
  <div class="body">
    <h2 id="ps-title">Nastavení přehrávače{stickName ? ` · ${stickName}` : ""}</h2>
    <p class="sub">
      {#if stick}
        My Settings na flashce. CDJ/XDJ je po vložení flashky nabídne načíst (MENU → MY SETTINGS).
      {:else}
        Tvoje nastavení CDJ a mixeru. Zapíše se na flashku při synchronizaci a formátování.
      {/if}
      {#if loaded}<span class="src">{SOURCES[loaded.source]}</span>{/if}
    </p>
    {#if loaded?.playerSaved.length}
      <p class="note">
        Část nastavení na flashce uložil přímo přehrávač ({loaded.playerSaved.join(", ")}). Zápisem ho přepíšeš,
        stará verze zůstane v záloze.
      </p>
    {/if}

    {#if !loaded && !failed}
      <p class="sub">Načítám…</p>
    {/if}

    {#each GROUPS as [group, title] (group)}
      {@const list = fields.filter((f) => f.group === group)}
      {#if list.length && loaded}
        <h3>{title}</h3>
        <div class="grid">
          {#each list as f (f.key)}
            {@const differs = stick && loaded.source === "stick" && values[f.key] !== loaded.profile[f.key]}
            <div class="field">
              <span class="label" title={differs ? "Liší se od tvého uloženého nastavení" : undefined}>
                {f.label}{#if differs}<span class="diff"></span>{/if}
              </span>
              {#if f.options.length > 5}
                <select value={values[f.key]} onchange={(e) => set(f.key, Number(e.currentTarget.value))}>
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
    {/each}

    {#if failed}<p class="err">{failed}</p>{/if}
  </div>
  <div class="foot">
    <button class="btn" onclick={defaults} disabled={!loaded || busy}><RotateCcw size={14} /> Výchozí</button>
    {#if done}
      <span class="done"><CircleCheck size={14} /> {done}</span>
    {:else if changed}
      <span class="changes">{changed} {changed === 1 ? "změna" : changed < 5 ? "změny" : "změn"}</span>
    {/if}
    <button class="btn" onclick={close} disabled={busy}>Zavřít</button>
    <button class="btn primary" onclick={save} disabled={!loaded || busy}>
      {stick ? "Zapsat na flashku" : "Uložit"}
    </button>
  </div>
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
    width: 760px;
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
    margin: 18px 0 8px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .sub {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--text-2);
  }
  .src {
    margin-left: 6px;
    font-size: 10px;
    color: var(--cue);
    border: 1px solid rgb(242 163 58 / 0.5);
    border-radius: 3px;
    padding: 0 4px;
  }
  .note {
    margin: 10px 0 0;
    font-size: 11px;
    line-height: 1.5;
    color: var(--warn);
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px 18px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 0;
  }
  .label {
    display: flex;
    align-items: center;
    gap: 6px;
    font-family: var(--font-readout);
    font-size: 10.5px;
    letter-spacing: 0.04em;
    color: var(--text-2);
  }
  .diff {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--cue);
  }
  .seg {
    display: flex;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
    background: var(--booth);
  }
  .seg button {
    flex: 1;
    min-width: 0;
    border: 0;
    border-right: 1px solid var(--line);
    background: none;
    color: var(--text-2);
    font-size: 11px;
    padding: 5px 4px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .seg button:last-child {
    border-right: 0;
  }
  .seg button:hover {
    color: var(--text);
  }
  .seg button.on {
    background: var(--panel-2);
    color: var(--cue);
    font-weight: 600;
  }
  select {
    background: var(--booth);
    color: var(--text);
    border: 1px solid var(--line);
    border-radius: 6px;
    padding: 5px 8px;
    font-size: 12px;
  }
  .err {
    color: var(--fault);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--line);
  }
  .foot .btn:nth-of-type(2) {
    margin-left: auto;
  }
  .done,
  .changes {
    margin-left: 8px;
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-2);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .done {
    color: var(--play);
  }
</style>
