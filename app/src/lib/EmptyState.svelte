<script lang="ts">
  import { FolderOpen, Usb } from "@lucide/svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { app } from "./state.svelte";

  async function openFolder() {
    const d = await open({ directory: true });
    if (typeof d === "string") await app.open(d);
  }
</script>

<div class="empty" data-tauri-drag-region>
  <div class="tile"><Usb size={34} strokeWidth={1.5} /></div>
  <span class="eyebrow">Flashka</span>
  <h1>Připoj flashku</h1>
  <p>Uvidíš její playlisty, stav a co na kterém přehrávači pojede.</p>
  <button class="btn lg" onclick={openFolder}><FolderOpen size={15} /> Otevřít složku…</button>
  <p class="small">Obraz flashky nebo složku se zálohou</p>
  {#if app.error}<p class="err">{app.error}</p>{/if}
</div>

<style>
  .empty {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 24px;
    text-align: center;
  }
  .tile {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 72px;
    height: 72px;
    margin-bottom: 14px;
    border-radius: 18px;
    background: var(--surface-2);
    color: var(--accent);
  }
  h1 {
    margin: 4px 0 2px;
    font-size: 28px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }
  p {
    margin: 0 0 18px;
    max-width: 380px;
    font-size: 13.5px;
    color: var(--text-2);
  }
  .small {
    margin: 10px 0 0;
    font-size: 12px;
    color: var(--text-3);
  }
  .err {
    margin-top: 16px;
    padding: 8px 12px;
    border-radius: var(--r-control);
    background: var(--danger-soft);
    color: var(--danger);
    max-width: 480px;
  }
</style>
