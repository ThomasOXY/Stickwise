<script lang="ts">
  import { CircleCheck, RotateCw, Settings, ShieldCheck } from "@lucide/svelte";
  import { t } from "./i18n.svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import { access as a } from "./access.svelte";

  const step1 = $derived(t("shell.access.step1").split("{path}"));
  // "{app} … {plus} …": the pieces around both.
  const step2 = $derived(t("shell.access.step2").split(/\{app\}|\{plus\}/));
</script>

<SheetFrame id="access-title" icon={ShieldCheck} title={t("shell.access.title")} sub={t("shell.access.sub")} width={540} onclose={() => a.close()}>
  <div class="body">
    {#if a.granted}
      <p class="ok"><CircleCheck size={18} />{t("shell.access.granted")}</p>
    {:else}
      <p>{t("shell.access.why")}</p>
      <ol>
        <li>{step1[0]}<strong>{t("shell.access.step1Path")}</strong>{step1[1]}</li>
        <li>{step2[0]}<strong>Stickwise</strong>{step2[1]}<strong>+</strong>{step2[2]}</li>
        <li>{t("shell.access.step3")}</li>
      </ol>
      <p class="quiet">{t("shell.access.network")}</p>
    {/if}
  </div>

  {#snippet foot()}
    {#if a.granted}
      <button class="btn ghost lg" onclick={() => a.close()}>{t("common.done")}</button>
      <button class="btn primary lg" onclick={() => a.restart()}><RotateCw size={16} />{t("common.restart")}</button>
    {:else}
      <button class="btn ghost lg" onclick={() => a.close()}>{t("common.later")}</button>
      <button class="btn lg" onclick={() => a.restart()}><RotateCw size={16} />{t("common.restart")}</button>
      <button class="btn primary lg" onclick={() => a.settings()}><Settings size={16} />{t("shell.access.openSettings")}</button>
    {/if}
  {/snippet}
</SheetFrame>

<style>
  .body {
    padding: 20px 26px 24px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    font-size: 14px;
    line-height: 1.5;
    color: var(--text-2);
  }
  p {
    margin: 0;
  }
  ol {
    margin: 0;
    padding-left: 22px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    color: var(--text);
  }
  .ok {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--success);
    font-size: 15px;
  }
  .quiet {
    color: var(--text-3);
    font-size: 13px;
  }
</style>
