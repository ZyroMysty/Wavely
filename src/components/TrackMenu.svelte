<script>
  import { ui } from "../lib/ui.svelte.js";
  import { playNext, addToQueue } from "../lib/player.svelte.js";
  import Icon from "./Icon.svelte";

  let menu = $state();

  // Menü im Fenster halten
  const position = $derived.by(() => {
    if (!ui.menu) return { x: 0, y: 0 };
    const width = 230;
    const height = 96;
    return {
      x: Math.min(ui.menu.x, window.innerWidth - width - 8),
      y: Math.min(ui.menu.y, window.innerHeight - height - 8),
    };
  });

  function run(action) {
    action(ui.menu.track);
    ui.menu = null;
  }

  function close(event) {
    if (ui.menu && !menu?.contains(event.target)) ui.menu = null;
  }
</script>

<svelte:window
  onmousedown={close}
  onkeydown={(e) => e.key === "Escape" && (ui.menu = null)}
  onblur={() => (ui.menu = null)}
/>

{#if ui.menu}
  <div class="menu" bind:this={menu} style:left="{position.x}px" style:top="{position.y}px" role="menu">
    <button role="menuitem" onclick={() => run(playNext)}>
      <Icon name="playNext" size={18} /> Als Nächstes spielen
    </button>
    <button role="menuitem" onclick={() => run(addToQueue)}>
      <Icon name="addQueue" size={18} /> Zur Warteschlange hinzufügen
    </button>
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 100;
    min-width: 230px;
    padding: 6px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
  }
  button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    border-radius: 6px;
    text-align: left;
  }
  button:hover {
    background: var(--hover);
  }
</style>
