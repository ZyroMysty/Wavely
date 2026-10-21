<script>
  import { ui } from "../lib/ui.svelte.js";
  import Icon from "./Icon.svelte";
  import QueuePanel from "./QueuePanel.svelte";
  import LyricsPanel from "./LyricsPanel.svelte";
  import HistoryPanel from "./HistoryPanel.svelte";
</script>

<aside>
  <header>
    <div class="tabs">
      <button class:active={ui.panel === "queue"} onclick={() => (ui.panel = "queue")}>Warteschlange</button>
      <button class:active={ui.panel === "lyrics"} onclick={() => (ui.panel = "lyrics")}>Liedtext</button>
      <button class:active={ui.panel === "history"} onclick={() => (ui.panel = "history")}>Verlauf</button>
    </div>
    <button class="close" title="Schließen" onclick={() => (ui.panel = null)}>
      <Icon name="close" size={18} />
    </button>
  </header>
  <div class="content">
    {#if ui.panel === "queue"}
      <QueuePanel />
    {:else if ui.panel === "lyrics"}
      <LyricsPanel />
    {:else}
      <HistoryPanel />
    {/if}
  </div>
</aside>

<style>
  aside {
    grid-area: panel;
    width: 340px;
    background: var(--surface);
    border-left: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 12px 10px 16px;
  }
  .tabs {
    display: flex;
    gap: 4px;
    background: var(--surface-2);
    border-radius: 999px;
    padding: 3px;
  }
  .tabs button {
    padding: 6px 11px;
    border-radius: 999px;
    color: var(--text-dim);
    font-weight: 600;
    font-size: 13px;
  }
  .tabs button.active {
    background: var(--hover);
    color: var(--text);
  }
  .close {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--text-dim);
  }
  .close:hover {
    background: var(--hover);
    color: var(--text);
  }
  .content {
    flex: 1;
    overflow-y: auto;
    padding: 4px 8px 16px;
  }
</style>
