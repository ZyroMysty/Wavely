<script>
  import { shortcuts, SHORTCUT_LIST, GLOBAL_LIST } from "../lib/shortcuts.svelte.js";
  import { settings } from "../lib/settings.svelte.js";

  const close = () => (shortcuts.help = false);
</script>

{#if shortcuts.help}
  <div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && close()}>
    <div class="dialog" role="dialog" aria-label="Tastenkürzel">
      <h2>Tastenkürzel</h2>
      <dl>
        {#each SHORTCUT_LIST as [keys, label] (keys)}
          <dt>{#each keys.split(" + ") as k, i (i)}{#if i}<span class="plus">+</span>{/if}<kbd>{k}</kbd>{/each}</dt>
          <dd>{label}</dd>
        {/each}
      </dl>
      <h3>Systemweit {settings.globalShortcuts ? "" : "(in den Einstellungen einschaltbar)"}</h3>
      <dl class:off={!settings.globalShortcuts}>
        {#each GLOBAL_LIST as [keys, label] (keys)}
          <dt>{#each keys.split(" + ") as k, i (i)}{#if i}<span class="plus">+</span>{/if}<kbd>{k}</kbd>{/each}</dt>
          <dd>{label}</dd>
        {/each}
      </dl>
      <div class="buttons"><button class="primary" onclick={close}>Schließen</button></div>
    </div>
  </div>
{/if}

<svelte:window onkeydown={(e) => e.key === "Escape" && shortcuts.help && close()} />

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
    background: rgba(0, 0, 0, 0.55);
    display: grid;
    place-items: center;
  }
  .dialog {
    width: min(520px, calc(100vw - 32px));
    max-height: calc(100vh - 48px);
    overflow-y: auto;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 24px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6);
  }
  h2 {
    margin: 0 0 14px;
  }
  h3 {
    margin: 18px 0 10px;
    font-size: 14px;
    color: var(--text-dim);
    font-weight: 600;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 18px;
    margin: 0;
    align-items: center;
  }
  dl.off {
    opacity: 0.5;
  }
  dt {
    display: flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }
  dd {
    margin: 0;
    color: var(--text-dim);
    font-size: 14px;
  }
  kbd {
    min-width: 26px;
    padding: 3px 7px;
    border-radius: 6px;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    background: var(--surface-2);
    font-family: inherit;
    font-size: 12px;
    text-align: center;
  }
  .plus {
    color: var(--text-dim);
    font-size: 12px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    margin-top: 18px;
  }
</style>
