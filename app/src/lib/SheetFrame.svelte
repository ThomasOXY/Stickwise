<script lang="ts">
  import { X } from "@lucide/svelte";
  import type { Component, Snippet } from "svelte";

  /** The v2 sheet: scrim, card, header with an icon tile, body, footer. */
  let {
    id,
    icon: Icon,
    title,
    sub,
    width = 640,
    tone = "accent",
    onclose,
    badge,
    children,
    foot,
  }: {
    id: string;
    icon: Component<{ size?: number }>;
    title: string;
    sub?: string;
    width?: number;
    tone?: "accent" | "danger";
    /** Shows the close button; leave out while something runs. */
    onclose?: () => void;
    badge?: Snippet;
    children: Snippet;
    foot?: Snippet;
  } = $props();
</script>

<div class="scrim"></div>
<div class="sheet" role="dialog" aria-modal="true" aria-labelledby={id} style:width="min({width}px, calc(100vw - 48px))">
  <header>
    <span class="tile {tone}"><Icon size={18} /></span>
    <div class="titles">
      <h2 {id}>{title}</h2>
      {#if sub}<p>{sub}</p>{/if}
    </div>
    {#if badge}{@render badge()}{/if}
    {#if onclose}
      <button class="btn icon close" onclick={onclose} aria-label="Zavřít"><X size={16} /></button>
    {/if}
  </header>
  {@render children()}
  {#if foot}<footer>{@render foot()}</footer>{/if}
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    background: rgb(5 6 8 / 0.6);
    backdrop-filter: blur(6px);
    -webkit-backdrop-filter: blur(6px);
    z-index: 20;
    animation: fade 0.18s ease-out;
  }
  .sheet {
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    max-height: calc(100vh - 64px);
    display: flex;
    flex-direction: column;
    background: var(--surface-1);
    border-radius: 18px;
    box-shadow: var(--shadow-pop), inset 0 0 0 1px var(--hairline);
    z-index: 21;
    overflow: hidden;
    animation: drop 0.2s ease-out;
  }
  @keyframes drop {
    from {
      transform: translate(-50%, calc(-50% - 10px));
      opacity: 0;
    }
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  header {
    flex: none;
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 22px 26px 20px;
    box-shadow: inset 0 -1px 0 var(--hairline);
  }
  .tile {
    flex: none;
    width: 40px;
    height: 40px;
    border-radius: 10px;
    display: grid;
    place-items: center;
    background: var(--surface-3);
    color: var(--accent);
  }
  .tile.danger {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .titles p {
    margin: 3px 0 0;
    font-size: 13px;
    color: var(--text-2);
  }
  .close {
    flex: none;
    background: var(--surface-2);
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
</style>
