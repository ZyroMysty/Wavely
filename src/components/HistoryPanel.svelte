<script>
  import { history, groupedByDay, removeEntry, clearHistory } from "../lib/history.svelte.js";
  import { playList } from "../lib/player.svelte.js";
  import { openTrackMenu } from "../lib/ui.svelte.js";
  import { invoke } from "@tauri-apps/api/core";
  import { ytm } from "../lib/ytm.svelte.js";
  import Cover from "./Cover.svelte";
  import Icon from "./Icon.svelte";

  const groups = $derived(groupedByDay(history.entries));
  let confirmClear = $state(false);

  /** "wavely": in Wavely gespielt · "youtube": Verlauf aus dem YouTube-Music-Konto */
  let source = $state("wavely");
  let yt = $state({ loading: false, error: null, groups: null });

  async function loadYouTube() {
    yt.loading = true;
    yt.error = null;
    try {
      const entries = await invoke("ytm_history");
      // Aufeinanderfolgende Einträge mit gleicher Gruppe („Heute“, „Diese Woche“ …) zusammenfassen
      const groups = [];
      for (const { track, group } of entries) {
        const label = group ?? "Früher";
        if (groups.at(-1)?.label !== label) groups.push({ label, tracks: [] });
        groups.at(-1).tracks.push({ ...track, source: "youtube" });
      }
      yt.groups = groups;
    } catch (e) {
      yt.error = String(e);
    } finally {
      yt.loading = false;
    }
  }

  function selectSource(value) {
    source = value;
    if (value === "youtube" && !yt.groups && !yt.loading) loadYouTube();
  }

  const time = (ts) => new Date(ts).toLocaleTimeString("de-DE", { hour: "2-digit", minute: "2-digit" });
</script>

{#snippet trackRow(track, timeText, onremove)}
  <div class="row" role="listitem" oncontextmenu={(e) => openTrackMenu(e, track)}>
    <button class="main" title="Abspielen" onclick={() => playList([track])}>
      <Cover {track} size={40} />
      <span class="text">
        <strong>{track.title}</strong>
        <small>{track.artist ?? "Unbekannter Künstler"}</small>
      </span>
      {#if timeText}<span class="time">{timeText}</span>{/if}
    </button>
    {#if onremove}
      <button class="remove" title="Aus dem Verlauf entfernen" onclick={onremove}>
        <Icon name="close" size={16} />
      </button>
    {/if}
  </div>
{/snippet}

<div class="history">
  {#if ytm.status === "loggedIn"}
    <div class="switch-row">
      <div class="segmented">
        <button class:active={source === "wavely"} onclick={() => selectSource("wavely")}>Wavely</button>
        <button class:active={source === "youtube"} onclick={() => selectSource("youtube")}>YouTube</button>
      </div>
      {#if source === "youtube"}
        <button class="refresh" title="Neu laden" onclick={loadYouTube} disabled={yt.loading}>
          <Icon name="refresh" size={16} />
        </button>
      {/if}
    </div>
  {/if}

  {#if source === "youtube"}
    {#if yt.loading && !yt.groups}
      <p class="empty">YouTube-Verlauf wird geladen…</p>
    {:else if yt.error}
      <p class="empty error">{yt.error}</p>
    {:else if yt.groups && !yt.groups.length}
      <p class="empty">Dein YouTube-Music-Verlauf ist leer.</p>
    {:else if yt.groups}
      <p class="note">Songs, die du in Wavely hörst, erscheinen hier nicht – nur, was du direkt bei YouTube Music gehört hast.</p>
      {#each yt.groups as group, gi (gi)}
        <h3>{group.label}</h3>
        {#each group.tracks as track, ti (track.id + ":" + ti)}
          {@render trackRow(track, null, null)}
        {/each}
      {/each}
    {/if}
  {:else if !history.entries.length}
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
        {@render trackRow(entry.track, time(entry.playedAt), () => removeEntry(index))}
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
  .switch-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 8px 8px;
  }
  .segmented {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 999px;
    background: var(--surface-2);
  }
  .segmented button {
    padding: 5px 14px;
    border-radius: 999px;
    color: var(--text-dim);
    font-weight: 600;
    font-size: 13px;
  }
  .segmented button.active {
    background: var(--hover);
    color: var(--text);
  }
  .refresh {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--text-dim);
  }
  .refresh:hover {
    background: var(--hover);
    color: var(--text);
  }
  .note {
    margin: 0 8px 4px;
    color: var(--text-dim);
    font-size: 12px;
    line-height: 1.4;
  }
  .error {
    color: var(--accent);
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
