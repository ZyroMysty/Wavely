<script>
  import { offline, tracksOf, allTracks, progressOf, removeCollection } from "../lib/offline.svelte.js";
  import { player, playList, toggleShuffle } from "../lib/player.svelte.js";
  import { artworkSrc, isLetterboxed } from "../lib/artwork.js";
  import CollectionCard from "./CollectionCard.svelte";
  import TrackList from "./TrackList.svelte";
  import Icon from "./Icon.svelte";

  /** null = Übersicht, sonst die geöffnete Playlist-ID */
  let openId = $state(null);

  const songs = $derived(allTracks());
  const open = $derived(offline.collections.find((c) => c.id === openId) ?? null);
  const openTracks = $derived(open ? tracksOf(open) : []);
  const pendingCount = $derived(Object.keys(offline.pending).length);

  function playAll(tracks, shuffle) {
    if (!tracks.length) return;
    if (player.shuffle !== shuffle) toggleShuffle();
    playList(tracks, shuffle ? Math.floor(Math.random() * tracks.length) : 0);
  }

  async function remove(id) {
    openId = null;
    await removeCollection(id);
  }
</script>

<section>
  {#if open}
    <button class="ghost back" title="Zurück" onclick={() => (openId = null)}>
      <Icon name="previous" size={18} />
    </button>
    <div class="collection-head">
      <div class="big-art">
        {#if open.artworkUrl}
          <img src={artworkSrc(open.artworkUrl)} alt="" class:letterboxed={isLetterboxed(open.artworkUrl)} />
        {:else}
          <Icon name="music" size={64} />
        {/if}
      </div>
      <div>
        <p class="kind">Offline · {open.kind === "album" ? "Album" : "Playlist"}</p>
        <h1>{open.title}</h1>
        <p class="dim">
          {#if progressOf(open.trackIds).pending}
            {progressOf(open.trackIds).done} von {open.trackIds.length} gespeichert…
          {:else}
            {openTracks.length} Titel
          {/if}
        </p>
        <div class="buttons">
          <button class="primary" onclick={() => playAll(openTracks, false)}>
            <Icon name="play" size={18} /> Abspielen
          </button>
          <button class="secondary" onclick={() => playAll(openTracks, true)}>
            <Icon name="shuffle" size={18} /> Zufall
          </button>
          <button class="secondary" onclick={() => remove(open.id)}>
            <Icon name="trash" size={18} /> Offline entfernen
          </button>
        </div>
      </div>
    </div>
    <TrackList tracks={openTracks} covers />
  {:else}
    <header>
      <div>
        <h1>Offline</h1>
        <p class="sub">
          {songs.length} Songs gespeichert{#if pendingCount} · {pendingCount} werden geladen…{/if}
        </p>
      </div>
      {#if songs.length}
        <div class="buttons">
          <button class="secondary" onclick={() => playAll(songs, true)}>
            <Icon name="shuffle" size={18} /> Zufall
          </button>
          <button class="primary" onclick={() => playAll(songs, false)}>
            <Icon name="play" size={18} /> Alle abspielen
          </button>
        </div>
      {/if}
    </header>

    {#if offline.error}<p class="error">{offline.error}</p>{/if}

    {#if !songs.length && !offline.collections.length && !pendingCount}
      <div class="empty">
        <Icon name="download" size={48} />
        <h2>Noch nichts offline gespeichert</h2>
        <p>
          Öffne eine Playlist oder ein Album bei YouTube Music und klicke auf „Offline speichern“ – oder speichere
          einzelne Songs per Rechtsklick. Die Musik läuft dann auch ohne Internet.
        </p>
      </div>
    {:else}
      {#if offline.collections.length}
        <h2>Playlists & Alben</h2>
        <div class="grid">
          {#each offline.collections as c (c.id)}
            {@const p = progressOf(c.trackIds)}
            <CollectionCard
              item={{ ...c, subtitle: p.pending ? `${p.done} / ${p.total} gespeichert…` : `${p.done} Titel` }}
              onopen={() => (openId = c.id)}
            />
          {/each}
        </div>
      {/if}
      {#if songs.length}
        <h2>Alle Songs</h2>
        <TrackList tracks={songs} covers />
      {/if}
    {/if}
  {/if}
</section>

<style>
  section {
    padding: 28px 32px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: 16px;
    flex-wrap: wrap;
  }
  h1 {
    margin: 0;
    font-size: 30px;
  }
  h2 {
    margin: 28px 0 12px;
    font-size: 20px;
  }
  .sub {
    margin: 4px 0 0;
    color: var(--text-dim);
  }
  .buttons {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 4px;
    margin: 0 -12px;
  }
  .back {
    margin-bottom: 12px;
  }
  .collection-head {
    display: flex;
    align-items: flex-end;
    gap: 24px;
    margin-bottom: 28px;
  }
  .collection-head .buttons {
    margin-top: 14px;
  }
  .big-art {
    width: 200px;
    height: 200px;
    flex-shrink: 0;
    border-radius: var(--radius);
    background: var(--surface-2);
    color: var(--text-dim);
    display: grid;
    place-items: center;
    overflow: hidden;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.45);
  }
  .big-art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .big-art img.letterboxed {
    transform: scale(1.34);
  }
  .kind {
    margin: 0 0 4px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
  }
  .collection-head h1 {
    font-size: 36px;
  }
  .dim {
    color: var(--text-dim);
  }
  .error {
    color: var(--accent);
  }
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 8px;
    padding: 80px 20px;
    color: var(--text-dim);
  }
  .empty h2 {
    margin: 8px 0 0;
    color: var(--text);
  }
  .empty p {
    max-width: 440px;
    line-height: 1.5;
  }
</style>
