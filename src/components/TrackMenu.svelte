<script>
  import { ui, showToast } from "../lib/ui.svelte.js";
  import { playNext, addToQueue } from "../lib/player.svelte.js";
  import { offline, isOffline, saveTrack, removeTrack } from "../lib/offline.svelte.js";
  import { ytm, isLiked, toggleLike, editablePlaylists, addToPlaylist } from "../lib/ytm.svelte.js";
  import Icon from "./Icon.svelte";

  let menu = $state();
  /** "main" oder "playlists" (Auswahl beim Hinzufügen) */
  let page = $state("main");

  const track = $derived(ui.menu?.track);
  const youtube = $derived(track?.source === "youtube");
  /** Offline speichern geht für YouTube- und Spotify-Songs */
  const downloadable = $derived(track?.source === "youtube" || track?.source === "spotify");
  const loggedIn = $derived(ytm.status === "loggedIn");

  // Menü im Fenster halten
  const position = $derived.by(() => {
    if (!ui.menu) return { x: 0, y: 0 };
    const width = 250;
    const height = page === "playlists" ? 380 : 260;
    return {
      x: Math.max(8, Math.min(ui.menu.x, window.innerWidth - width - 8)),
      y: Math.max(8, Math.min(ui.menu.y, window.innerHeight - height - 8)),
    };
  });

  $effect(() => {
    if (ui.menu) page = "main";
  });

  function close() {
    ui.menu = null;
  }

  function run(action) {
    action(track);
    close();
  }

  async function like() {
    const t = track;
    close();
    try {
      const wasLiked = isLiked(t.id);
      await toggleLike(t);
      showToast(wasLiked ? "Aus „Musik, die ich mag“ entfernt" : "Zu „Musik, die ich mag“ hinzugefügt");
    } catch (e) {
      showToast(`Like fehlgeschlagen: ${e}`, true);
    }
  }

  async function addTo(playlist) {
    const t = track;
    close();
    try {
      await addToPlaylist(playlist.id, [t]);
      showToast(`Zu „${playlist.title}“ hinzugefügt`);
    } catch (e) {
      showToast(`Hinzufügen fehlgeschlagen: ${e}`, true);
    }
  }

  function newPlaylist() {
    const t = track;
    close();
    ui.newPlaylist = { tracks: [t] };
  }

  function onOutside(event) {
    if (ui.menu && !menu?.contains(event.target)) close();
  }
</script>

<svelte:window onmousedown={onOutside} onkeydown={(e) => e.key === "Escape" && close()} onblur={close} />

{#if ui.menu}
  <div class="menu" bind:this={menu} style:left="{position.x}px" style:top="{position.y}px" role="menu">
    {#if page === "main"}
      <button role="menuitem" onclick={() => run(playNext)}>
        <Icon name="playNext" size={18} /> Als Nächstes spielen
      </button>
      <button role="menuitem" onclick={() => run(addToQueue)}>
        <Icon name="addQueue" size={18} /> Zur Warteschlange hinzufügen
      </button>

      {#if youtube && loggedIn}
        <hr />
        <button role="menuitem" onclick={like}>
          <Icon name={isLiked(track.id) ? "heart" : "heartOutline"} size={18} />
          {isLiked(track.id) ? "Gefällt mir nicht mehr" : "Gefällt mir"}
        </button>
        <button role="menuitem" onclick={() => (page = "playlists")}>
          <Icon name="playlistAdd" size={18} /> Zu Playlist hinzufügen
          <span class="chevron"><Icon name="chevron" size={18} /></span>
        </button>
      {/if}

      {#if downloadable}
        <hr />
        {#if isOffline(track.id)}
          <button role="menuitem" onclick={() => run((t) => removeTrack(t.id))}>
            <Icon name="trash" size={18} /> Offline entfernen
          </button>
        {:else if offline.pending[track.id]}
          <button role="menuitem" disabled><Icon name="download" size={18} /> Wird gespeichert…</button>
        {:else}
          <button role="menuitem" onclick={() => run(saveTrack)}>
            <Icon name="download" size={18} /> Offline speichern
          </button>
        {/if}
      {/if}
    {:else}
      <button class="back" onclick={() => (page = "main")}>
        <Icon name="previous" size={16} /> Zu Playlist hinzufügen
      </button>
      <hr />
      <button role="menuitem" onclick={newPlaylist}>
        <Icon name="plus" size={18} /> Neue Playlist…
      </button>
      <div class="list">
        {#each editablePlaylists() as playlist (playlist.id)}
          <button role="menuitem" onclick={() => addTo(playlist)}>
            <Icon name="queue" size={18} /> <span class="label">{playlist.title}</span>
          </button>
        {:else}
          <p class="hint">Keine eigenen Playlists gefunden.</p>
        {/each}
      </div>
    {/if}
  </div>
{/if}

<style>
  .menu {
    position: fixed;
    z-index: 100;
    width: 250px;
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
  button:hover:not(:disabled) {
    background: var(--hover);
  }
  button:disabled {
    color: var(--text-dim);
    cursor: default;
  }
  .chevron {
    margin-left: auto;
    display: grid;
    color: var(--text-dim);
  }
  .back {
    color: var(--text-dim);
    font-size: 13px;
    font-weight: 600;
  }
  .list {
    max-height: 260px;
    overflow-y: auto;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  hr {
    border: none;
    border-top: 1px solid var(--border);
    margin: 4px 6px;
  }
  .hint {
    margin: 6px 10px;
    color: var(--text-dim);
    font-size: 13px;
  }
</style>
