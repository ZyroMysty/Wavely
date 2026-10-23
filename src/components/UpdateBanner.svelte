<script>
  import { updater, installUpdate } from "../lib/updater.svelte.js";
  import Icon from "./Icon.svelte";

  const visible = $derived(
    !updater.dismissed && ["available", "downloading", "installing"].includes(updater.status),
  );
  const busy = $derived(updater.status === "downloading" || updater.status === "installing");
</script>

{#if visible}
  <div class="banner" role="status">
    <div class="text">
      <strong>Wavely {updater.version} ist da</strong>
      <small>
        {#if updater.status === "installing"}
          Wird installiert – Wavely startet gleich neu…
        {:else if updater.status === "downloading"}
          Wird heruntergeladen{updater.progress != null ? ` (${Math.round(updater.progress * 100)} %)` : "…"}
        {:else}
          Die Musik stoppt kurz, danach startet Wavely neu.
        {/if}
      </small>
      {#if updater.notes && !busy}<p class="notes">{updater.notes}</p>{/if}
      {#if updater.status === "downloading"}
        <div class="bar"><div style:width="{(updater.progress ?? 0) * 100}%"></div></div>
      {/if}
    </div>
    {#if !busy}
      <div class="actions">
        <button class="secondary" onclick={() => (updater.dismissed = true)}>Später</button>
        <button class="primary" onclick={installUpdate}><Icon name="download" size={18} /> Jetzt aktualisieren</button>
      </div>
    {/if}
  </div>
{/if}

<style>
  .banner {
    position: fixed;
    right: 20px;
    bottom: calc(var(--player-height) + 16px);
    z-index: 250;
    width: min(360px, calc(100vw - 32px));
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
  }
  .text {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  small {
    color: var(--text-dim);
    font-size: 13px;
  }
  .notes {
    margin: 4px 0 0;
    max-height: 120px;
    overflow-y: auto;
    font-size: 13px;
    line-height: 1.45;
    white-space: pre-line;
  }
  .bar {
    margin-top: 6px;
    height: 4px;
    border-radius: 2px;
    background: var(--border);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
