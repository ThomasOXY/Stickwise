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
  <Usb size={72} strokeWidth={1} color="var(--line)" />
  <h1>Připoj flashku</h1>
  <p>Uvidíš její playlisty, stav a co na kterém přehrávači pojede.</p>
  <button class="btn" onclick={openFolder}><FolderOpen size={14} /> Otevřít složku…</button>
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
    text-align: center;
  }
  h1 {
    margin: 18px 0 4px;
    font-size: 20px;
    font-weight: 600;
  }
  p {
    margin: 0 0 18px;
    color: var(--text-2);
  }
  .small {
    margin: 8px 0 0;
    font-size: 11px;
  }
  .err {
    margin-top: 16px;
    color: var(--fault);
    max-width: 480px;
  }
</style>
