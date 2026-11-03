<script lang="ts">
  /** Visual tri-state checkbox square; the row around it handles the click. */
  let { state }: { state: "on" | "off" | "some" } = $props();
</script>

<span class="check {state}" aria-hidden="true"></span>

<style>
  .check {
    flex: none;
    position: relative;
    width: 16px;
    height: 16px;
    border-radius: 4px;
    box-shadow: inset 0 0 0 1.5px var(--text-3);
  }
  .check.on,
  .check.some {
    background: var(--accent);
    box-shadow: none;
  }
  .check.on::after {
    content: "";
    position: absolute;
    left: 5px;
    top: 2px;
    width: 4px;
    height: 8px;
    border: solid var(--on-accent);
    border-width: 0 2px 2px 0;
    transform: rotate(45deg);
  }
  .check.some {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1.5px var(--accent);
  }
  .check.some::after {
    content: "";
    position: absolute;
    left: 4px;
    right: 4px;
    top: 7px;
    height: 2px;
    border-radius: 1px;
    background: var(--accent);
  }
</style>
