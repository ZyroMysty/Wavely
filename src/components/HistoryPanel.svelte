<script>
  import { history, groupedByDay, removeEntry, clearHistory } from "../lib/history.svelte.js";
  import { playList } from "../lib/player.svelte.js";
  import { openTrackMenu } from "../lib/ui.svelte.js";
  import Cover from "./Cover.svelte";
  import Icon from "./Icon.svelte";

  const groups = $derived(groupedByDay(history.entries));
  let confirmClear = $state(false);

  const time = (ts) => new Date(ts).toLocaleTimeString("de-DE", { hour: "2-digit", minute: "2-digit" });
</script>

<div class="history">
  {#if !history.entries.length}
    <p class="empty">Noch nichts gehört. Gespielte Songs erscheinen hier.</p>
  {:else}
    <div class="actions">
      {#if confirmClear}
        <span>Verlauf wirklich löschen?</span>
        <button class="link" onclick={() => (confirmClear = false)}>Nein</button>
        <button
          class="link danger"
          onclick={() => {
            clearHistory();
            confirmClear = false;
          }}>Ja, löschen</button
        >
      {:else}
        <button class="link" onclick={() => (confirmClear = true)}>Verlauf löschen</button>
      {/if}
    </div>

    {#each groups as group (group.label)}
      <h3>{group.label}</h3>
      {#each group.items as { entry, index } (entry.playedAt + ":" + index)}
        <div class="row" role="listitem" oncontextmenu={(e) => openTrackMenu(e, entry.track)}>
          <button class="main" title="Abspielen" onclick={() => playList([entry.track])}>
            <Cover track={entry.track} size={40} />
            <span class="text">
              <strong>{entry.track.title}</strong>
              <small>{entry.track.artist ?? "Unbekannter Künstler"}</small>
            </span>
            <span class="time">{time(entry.playedAt)}</span>
          </button>
          <button class="remove" title="Aus dem Verlauf entfernen" onclick={() => removeEntry(index)}>
            <Icon name="close" size={16} />
          </button>
        </div>
      {/each}
    {/each}
  {/if}
</div>

<style>
  .history {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 10px;
    padding: 0 8px;
    font-size: 13px;
    color: var(--text-dim);
  }
  .link {
    color: var(--text-dim);
    font-size: 13px;
    text-decoration: underline;
  }
  .link:hover {
    color: var(--text);
  }
  .link.danger {
    color: var(--accent);
  }
  h3 {
    margin: 16px 8px 8px;
    font-size: 13px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
  }
  .row {
    display: flex;
    align-items: center;
    border-radius: 8px;
  }
  .row:hover {
    background: var(--surface-2);
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
    flex: 1;
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
  .text small,
  .time {
    color: var(--text-dim);
    font-size: 13px;
  }
  .time {
    font-variant-numeric: tabular-nums;
    flex-shrink: 0;
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
  .empty {
    margin: 20px 8px;
    color: var(--text-dim);
    font-size: 13px;
    line-height: 1.5;
  }
</style>
