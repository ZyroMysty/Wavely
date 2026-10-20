<script>
  import { player, jumpTo, removeFromQueue, toggleAutoplay } from "../lib/player.svelte.js";
  import Cover from "./Cover.svelte";
  import Icon from "./Icon.svelte";

  /** Sehr lange Warteschlangen nur teilweise anzeigen */
  const LIMIT = 150;

  const current = $derived(player.queue[player.index] ?? null);
  const upcoming = $derived(
    player.queue
      .map((track, index) => ({ track, index }))
      .slice(player.index + 1),
  );
  const manual = $derived(upcoming.filter((u) => !u.track.auto));
  const auto = $derived(upcoming.filter((u) => u.track.auto));
</script>

{#snippet row(track, index, removable)}
  <div class="row" class:current={index === player.index}>
    <button class="main" onclick={() => index !== player.index && jumpTo(index)} title="Abspielen">
      <Cover {track} size={40} />
      <span class="text">
        <strong>{track.title}</strong>
        <small>{track.artist ?? "Unbekannter Künstler"}</small>
      </span>
    </button>
    {#if removable}
      <button class="remove" title="Aus Warteschlange entfernen" onclick={() => removeFromQueue(index)}>
        <Icon name="close" size={16} />
      </button>
    {/if}
  </div>
{/snippet}

<div class="queue">
  {#if !current}
    <p class="empty">Die Warteschlange ist leer. Spiel einen Song ab oder füge per Rechtsklick Songs hinzu.</p>
  {:else}
    <h3>Läuft gerade</h3>
    {@render row(current, player.index, false)}

    <h3>Als Nächstes</h3>
    {#each manual.slice(0, LIMIT) as { track, index } (track.qid)}
      {@render row(track, index, true)}
    {:else}
      <p class="hint">Nichts mehr in der Warteschlange.</p>
    {/each}
    {#if manual.length > LIMIT}
      <p class="hint">… und {manual.length - LIMIT} weitere</p>
    {/if}

    <div class="autoplay-head">
      <div>
        <h3>Autoplay</h3>
        <p class="hint">Danach passende Songs abspielen</p>
      </div>
      <button
        class="switch"
        class:on={player.autoplay}
        role="switch"
        aria-checked={player.autoplay}
        aria-label="Autoplay"
        onclick={toggleAutoplay}
      ><span></span></button>
    </div>
    {#if player.autoplay}
      {#each auto.slice(0, LIMIT) as { track, index } (track.qid)}
        {@render row(track, index, true)}
      {:else}
        <p class="hint">Vorschläge werden gegen Ende der Warteschlange geladen.</p>
      {/each}
    {/if}
  {/if}
</div>

<style>
  .queue {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  h3 {
    margin: 18px 8px 8px;
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
  }
  h3:first-child {
    margin-top: 4px;
  }
  .row {
    display: flex;
    align-items: center;
    border-radius: 8px;
  }
  .row:hover {
    background: var(--surface-2);
  }
  .row.current strong {
    color: var(--accent);
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 8px;
    text-align: left;
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .text strong,
  .text small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .text strong {
    font-weight: 500;
  }
  .text small {
    color: var(--text-dim);
    font-size: 13px;
  }
  .remove {
    width: 30px;
    height: 30px;
    margin-right: 6px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--text-dim);
    opacity: 0;
  }
  .row:hover .remove {
    opacity: 1;
  }
  .remove:hover {
    background: var(--hover);
    color: var(--text);
  }
  .hint,
  .empty {
    margin: 0 8px 6px;
    color: var(--text-dim);
    font-size: 13px;
  }
  .empty {
    margin-top: 20px;
    line-height: 1.5;
  }
  .autoplay-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-top: 10px;
    padding-right: 8px;
  }
  .autoplay-head h3 {
    margin-bottom: 2px;
  }
  .switch {
    width: 38px;
    height: 22px;
    border-radius: 11px;
    background: var(--hover);
    position: relative;
    flex-shrink: 0;
    transition: background 0.15s;
  }
  .switch span {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--text);
    transition: transform 0.15s;
  }
  .switch.on {
    background: var(--accent);
  }
  .switch.on span {
    transform: translateX(16px);
  }
</style>
