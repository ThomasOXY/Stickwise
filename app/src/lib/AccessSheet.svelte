<script lang="ts">
  import { CircleCheck, RotateCw, Settings, ShieldCheck } from "@lucide/svelte";
  import SheetFrame from "./SheetFrame.svelte";
  import { access as a } from "./access.svelte";
</script>

<SheetFrame id="access-title" icon={ShieldCheck} title="Povol Stickwise přístup k disku" sub="Jednou a natrvalo" width={540} onclose={() => a.close()}>
  <div class="body">
    {#if a.granted}
      <p class="ok"><CircleCheck size={18} />Přístup k disku je povolený.</p>
    {:else}
      <p>
        Hudbu čte Stickwise z externích disků, flashek a z knihovny rekordboxu. Bez plného přístupu k disku se macOS ptá
        pořád dokola, nebo mu soubory rovnou nedá.
      </p>
      <ol>
        <li>Otevři <strong>Nastavení › Soukromí a zabezpečení › Plný přístup k disku</strong>.</li>
        <li>Zapni <strong>Stickwise</strong>. Když v seznamu chybí, přidej ho tlačítkem <strong>+</strong> ze složky Aplikace.</li>
        <li>Restartuj Stickwise.</li>
      </ol>
      <p class="quiet">Síť (hledání dalších Maců) se zeptá jen jednou. Po povolení vydrží i přes aktualizace.</p>
    {/if}
  </div>

  {#snippet foot()}
    {#if a.granted}
      <button class="btn ghost lg" onclick={() => a.close()}>Hotovo</button>
      <button class="btn primary lg" onclick={() => a.restart()}><RotateCw size={16} />Restartovat</button>
    {:else}
      <button class="btn ghost lg" onclick={() => a.close()}>Později</button>
      <button class="btn lg" onclick={() => a.restart()}><RotateCw size={16} />Restartovat</button>
      <button class="btn primary lg" onclick={() => a.settings()}><Settings size={16} />Otevřít Nastavení</button>
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
