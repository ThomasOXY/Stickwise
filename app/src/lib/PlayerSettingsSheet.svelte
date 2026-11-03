<script lang="ts">
  import { SlidersHorizontal } from "@lucide/svelte";
  import PlayerSettingsEditor from "./PlayerSettingsEditor.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import { app } from "./state.svelte";

  /** My Settings on a stick; the DJ's own are in the Settings window. */
  let { stick }: { stick: string } = $props();

  let busy = $state(false);
  let source = $state<string | null>(null);
  const stickName = $derived(app.sticks.find((s) => s.path === stick)?.name ?? stick.split("/").pop());
  const close = () => (app.settingsFor = undefined);
</script>

<svelte:window onkeydown={(e) => e.key === "Escape" && !busy && close()} />

<SheetFrame
  id="ps-title"
  icon={SlidersHorizontal}
  title="Nastavení přehrávače · {stickName}"
  sub="My Settings na flashce. CDJ/XDJ je po vložení flashky nabídne načíst (MENU → MY SETTINGS)."
  width={1000}
  onclose={busy ? undefined : close}
>
  {#snippet badge()}
    {#if source}<span class="src" class:stick={source === "Načteno z flashky"}>{source}</span>{/if}
  {/snippet}
  <PlayerSettingsEditor {stick} onclose={close} bind:busy bind:source />
</SheetFrame>

<style>
  .src {
    flex: none;
    height: 26px;
    line-height: 26px;
    padding: 0 12px;
    border-radius: 13px;
    font-size: 12px;
    font-weight: 500;
    color: var(--text-2);
    background: var(--surface-3);
  }
  .src.stick {
    color: var(--success);
    background: var(--success-soft);
  }
</style>
