<script>
  import Icon from "./Icon.svelte";

  /** Knopf in der Player-Leiste mit aufklappendem Menü darüber */
  let { icon, title, active = false, badge = null, width = 300, onopen = undefined, children } = $props();

  let open = $state(false);
  let root = $state();

  function toggle() {
    open = !open;
    if (open) onopen?.();
  }

  export function close() {
    open = false;
  }

  function outside(event) {
    if (open && !root?.contains(event.target)) open = false;
  }
</script>

<svelte:window onmousedown={outside} onkeydown={(e) => e.key === "Escape" && (open = false)} />

<div class="popover" bind:this={root}>
  <button class="small" class:on={active} class:wide={badge} {title} onclick={toggle}>
    <Icon name={icon} size={20} />
    {#if badge}<span class="badge">{badge}</span>{/if}
  </button>
  {#if open}
    <div class="menu" style:width="{width}px">
      {@render children({ close })}
    </div>
  {/if}
</div>

<style>
  .popover {
    position: relative;
  }
  .small {
    height: 32px;
    min-width: 32px;
    border-radius: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    color: var(--text-dim);
  }
  .small.wide {
    padding: 0 8px;
  }
  .small:hover {
    color: var(--text);
  }
  .small.on {
    color: var(--accent);
  }
  .badge {
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .menu {
    position: absolute;
    right: -40px;
    bottom: calc(100% + 12px);
    z-index: 60;
    max-height: calc(100vh - var(--player-height) - 40px);
    overflow-y: auto;
    padding: 8px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
  }
  .menu :global(h4) {
    margin: 6px 8px 8px;
    font-size: 13px;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .menu :global(.item) {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-radius: 6px;
    text-align: left;
  }
  .menu :global(.item:hover) {
    background: var(--hover);
  }
  .menu :global(.item.active) {
    color: var(--accent);
  }
  .menu :global(.item small) {
    display: block;
    color: var(--text-dim);
    font-size: 12px;
  }
  .menu :global(.hint) {
    margin: 8px 10px 4px;
    color: var(--text-dim);
    font-size: 12px;
    line-height: 1.4;
  }
  .menu :global(.hint.warn) {
    color: var(--accent);
  }
  .menu :global(hr) {
    border: none;
    border-top: 1px solid var(--border);
    margin: 8px 4px;
  }
</style>
