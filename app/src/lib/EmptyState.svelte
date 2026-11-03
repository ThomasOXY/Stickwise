<script lang="ts">
  import { FolderOpen, Usb } from "@lucide/svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { t } from "./i18n.svelte";
  import { app } from "./state.svelte";

  async function openFolder() {
    const d = await open({ directory: true });
    if (typeof d === "string") await app.open(d);
  }
</script>

<div class="empty" data-tauri-drag-region>
  <div class="tile"><Usb size={34} strokeWidth={1.5} /></div>
  <span class="eyebrow">{t("library.emptyState.eyebrow")}</span>
  <h1>{t("library.emptyState.title")}</h1>
  <p>{t("library.emptyState.lead")}</p>
  <button class="btn lg" onclick={openFolder}><FolderOpen size={15} /> {t("library.emptyState.openFolder")}</button>
  <p class="small">{t("library.emptyState.openHint")}</p>
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
