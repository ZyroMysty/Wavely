<script>
  import { ui, showToast } from "../lib/ui.svelte.js";
  import { getPlaylist, renamePlaylist, deletePlaylist, removeItem, moveItem } from "../lib/playlists.svelte.js";
  import { playList, toggleShuffle, player } from "../lib/player.svelte.js";
  import { openTransfer } from "../lib/transfer.svelte.js";
  import { formatTime } from "../lib/format.js";
  import PlaylistCover from "./PlaylistCover.svelte";
  import TrackList from "./TrackList.svelte";
  import Icon from "./Icon.svelte";

  const playlist = $derived(getPlaylist(ui.playlistId));
  const tracks = $derived(playlist ? playlist.items.map((i) => i.track) : []);
  const totalSeconds = $derived(tracks.reduce((sum, t) => sum + (t.duration || 0), 0));
  const sources = $derived(new Set(tracks.map((t) => t.source)).size);

  let editing = $state(false);
  let name = $state("");
  let input = $state();
  let confirmDelete = $state(false);

  // Neu angelegt: Namen sofort bearbeiten
  $effect(() => {
    if (ui.renamePlaylist && playlist) {
      ui.renamePlaylist = false;
      startEdit();
    }
  });

  function startEdit() {
    name = playlist.name;
    editing = true;
    queueMicrotask(() => {
      input?.focus();
      input?.select();
    });
  }

  function finishEdit() {
    if (editing) renamePlaylist(playlist.id, name);
    editing = false;
  }

  function play(shuffle) {
    if (!tracks.length) return;
    if (shuffle !== player.shuffle) toggleShuffle();
    playList(tracks, shuffle ? Math.floor(Math.random() * tracks.length) : 0);
  }

  function remove() {
    const title = playlist.name;
    deletePlaylist(playlist.id);
    ui.playlistId = null;
    ui.view = "home";
    showToast(`Playlist „${title}“ gelöscht`);
  }

  function duration(seconds) {
    const h = Math.floor(seconds / 3600);
    const m = Math.round((seconds % 3600) / 60);
    return h ? `${h} Std. ${m} Min.` : `${m} Min.`;
  }
</script>

{#if playlist}
  <div class="page">
    <div class="collection-head">
      <PlaylistCover {playlist} size={200} />
      <div class="info">
        <p class="kind">Wavely-Playlist</p>
        {#if editing}
          <input
            class="title-input"
            bind:this={input}
            bind:value={name}
            maxlength="120"
            onblur={finishEdit}
            onkeydown={(e) => {
              if (e.key === "Enter") finishEdit();
              if (e.key === "Escape") editing = false;
            }}
          />
        {:else}
          <button class="title" title="Umbenennen" onclick={startEdit}><h1>{playlist.name}</h1></button>
        {/if}
        <p class="dim">
          {tracks.length} {tracks.length === 1 ? "Song" : "Songs"}{totalSeconds ? ` · ${duration(totalSeconds)}` : ""}{sources > 1
            ? ` · aus ${sources} Quellen`
            : ""}
        </p>
        <div class="buttons">
          <button class="primary" onclick={() => play(false)} disabled={!tracks.length}>
            <Icon name="play" size={18} /> Abspielen
          </button>
          <button class="secondary" onclick={() => play(true)} disabled={!tracks.length}>
            <Icon name="shuffle" size={18} /> Zufall
          </button>
          <button
            class="secondary"
            onclick={() => openTransfer({ title: playlist.name, tracks }, "wavely")}
            disabled={!tracks.length}
            title="Als Playlist bei YouTube Music oder Spotify anlegen"
          >
            <Icon name="transfer" size={18} /> Übertragen
          </button>
          {#if confirmDelete}
            <button class="secondary danger" onclick={remove}>Wirklich löschen?</button>
            <button class="secondary" onclick={() => (confirmDelete = false)}>Abbrechen</button>
          {:else}
            <button class="secondary icon" title="Playlist löschen" onclick={() => (confirmDelete = true)}>
              <Icon name="trash" size={18} />
            </button>
          {/if}
        </div>
      </div>
    </div>

    {#if tracks.length}
      <TrackList
        {tracks}
        covers
        onremove={(i) => removeItem(playlist.id, i)}
        onmove={(from, to) => moveItem(playlist.id, from, to)}
      />
      <p class="hint">Tipp: Songs per Ziehen umsortieren. Neue Songs fügst du per Rechtsklick auf einen Song hinzu – aus jeder Quelle.</p>
    {:else}
      <div class="empty">
        <p>Diese Playlist ist noch leer.</p>
        <p class="dim">
          Klick mit der rechten Maustaste auf einen Song – egal ob lokal, YouTube Music, Spotify oder SoundCloud – und wähle
          „Zu Wavely-Playlist hinzufügen“.
        </p>
      </div>
    {/if}
  </div>
{:else}
  <div class="page"><p class="dim">Diese Playlist gibt es nicht mehr.</p></div>
{/if}

<style>
  .page {
    padding: 32px 40px 48px;
  }
  .collection-head {
    display: flex;
    align-items: flex-end;
    gap: 24px;
    margin-bottom: 28px;
  }
  .info {
    min-width: 0;
  }
  .kind {
    margin: 0 0 4px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
  }
  .title {
    text-align: left;
    max-width: 100%;
  }
  h1 {
    margin: 0;
    font-size: 36px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title:hover h1 {
    text-decoration: underline dotted var(--text-dim);
  }
  .title-input {
    font-size: 32px;
    font-weight: 700;
    width: min(600px, 100%);
    padding: 2px 8px;
    border-radius: 8px;
    border: 1px solid var(--accent);
    background: var(--surface-2);
    color: var(--text);
    outline: none;
  }
  .dim {
    color: var(--text-dim);
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin-top: 14px;
  }
  .icon {
    width: 40px;
    padding: 0;
    justify-content: center;
  }
  .danger {
    color: var(--accent);
  }
  .empty {
    padding: 40px 0;
    max-width: 520px;
    line-height: 1.5;
  }
  .empty p {
    margin: 0 0 8px;
  }
  .hint {
    margin-top: 16px;
    color: var(--text-dim);
    font-size: 12px;
  }
</style>
