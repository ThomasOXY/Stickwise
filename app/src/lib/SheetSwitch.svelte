<script lang="ts">
  import type { Snippet } from "svelte";

  /** On/off switch with a label and an optional line under it. */
  let {
    checked = $bindable(false),
    label,
    desc,
    onchange,
    extra,
  }: {
    checked?: boolean;
    label: string;
    desc?: string;
    onchange?: (on: boolean) => void;
    /** Inline content after the label (e.g. an "upravit" link). */
    extra?: Snippet;
  } = $props();
</script>

<label class="switch">
  <input
    type="checkbox"
    role="switch"
    bind:checked
    onchange={(e) => onchange?.(e.currentTarget.checked)}
  />
  <span class="track" aria-hidden="true"><span class="knob"></span></span>
  <span class="text">
    <span class="label">{label}{#if extra}{@render extra()}{/if}</span>
    {#if desc}<span class="desc">{desc}</span>{/if}
  </span>
</label>

<style>
  .switch {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    cursor: pointer;
  }
  input {
    position: absolute;
    opacity: 0;
    width: 1px;
    height: 1px;
    pointer-events: none;
  }
  .track {
    flex: none;
    position: relative;
    width: 34px;
    height: 20px;
    margin-top: 0;
    border-radius: 10px;
    background: var(--surface-3);
    transition: background 0.15s;
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--text-3);
    transition:
      transform 0.15s,
      background 0.15s;
  }
  input:checked + .track {
    background: var(--accent);
  }
  input:checked + .track .knob {
    transform: translateX(14px);
    background: var(--on-accent);
  }
  input:focus-visible + .track {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .label {
    font-size: 13.5px;
    line-height: 20px;
    color: var(--text);
  }
  .desc {
    font-size: 12px;
    color: var(--text-3);
    line-height: 1.4;
  }
</style>
