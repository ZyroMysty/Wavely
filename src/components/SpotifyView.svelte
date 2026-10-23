<script>
  import { openTransfer } from "../lib/transfer.svelte.js";
  import { untrack } from "svelte";
  import { spotify, login, logout, loadLibrary, fetchCollection, fetchArtist, search } from "../lib/spotify.svelte.js";
  import { ui } from "../lib/ui.svelte.js";
  import { player, playList, toggleShuffle } from "../lib/player.svelte.js";
  import { artworkSrc, isLetterboxed } from "../lib/artwork.js";
  import { isSavedCollection, progressOf, saveCollection, removeCollection } from "../lib/offline.svelte.js";
  import CollectionCard from "./CollectionCard.svelte";
  import TrackList from "./TrackList.svelte";
  import AccountMenu from "./AccountMenu.svelte";
  import Icon from "./Icon.svelte";

  import ArtistView from "./ArtistView.svelte";

  // Navigation innerhalb von Spotify (gleicher Aufbau wie YouTubeView); jede Seite trägt ihren Inhalt mit,
  // damit „Zurück“ die vorige Seite genau so wiederherstellt.
  // home | collection { collection } | search { results } | artist { artist }
  let page = $state({ type: "home" });
  let history = [];
  let query = $state("");
  let loading = $state(false);
  let error = $state(null);
  const collection = $derived(page.collection ?? null);
  const results = $derived(page.results ?? null);

  function go(next) {
    history.push(page);
    page = next;
  }

  // Album/Künstler von außerhalb öffnen (Songliste, Player-Leiste, Verlauf …)
  $effect(() => {
    const target = ui.spNav;
    if (!target) return;
    untrack(() => {
      ui.spNav = null;
      if (target.type === "artist") openArtistPage(target);
      else openCollection(target);
    });
  });

  function openArtistPage(target) {
    go({ type: "artist", artist: { id: target.id, name: target.title ?? "", loading: true } });
    const current = page;
    run(async () => {
      const artist = await fetchArtist(target.id);
      if (page === current) page.artist = artist;
    });
  }

  // Erneuter Klick auf „Spotify“ in der Seitenleiste: zurück zur Bibliothek
  let lastHome = ui.spHome;
  $effect(() => {
    if (ui.spHome === lastHome) return;
    lastHome = ui.spHome;
    page = { type: "home" };
    history = [];
    error = null;
  });

  function back() {
    page = history.pop() ?? { type: "home" };
  }

  async function run(task) {
    loading = true;
    error = null;
    try {
      await task();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function openCollection(item) {
    go({ type: "collection", collection: { ...item, tracks: [] } });
    const current = page;
    run(async () => {
      const detail = await fetchCollection(item.kind, item.id);
      if (page !== current) return;
      page.collection = {
        ...detail,
        title: item.title || detail.title,
        artworkUrl: item.artworkUrl || detail.artworkUrl,
      };
    });
  }

  function submitSearch(e) {
    e.preventDefault();
    const q = query.trim();
    if (!q) return;
    if (page.type !== "search") go({ type: "search", results: null });
    else page.results = null;
    const current = page;
    run(async () => {
      const found = await search(q);
      if (page === current) page.results = found;
    });
  }

  // Offline-Status der geöffneten Playlist
  const saved = $derived(collection ? isSavedCollection(collection.id) : false);
  const saveProgress = $derived(collection ? progressOf(collection.tracks.map((t) => t.id)) : null);

  function toggleOffline() {
    if (saved) removeCollection(collection.id);
    else saveCollection(collection, collection.tracks);
  }

  const likedCard = { id: "liked", kind: "playlist", title: "Lieblingssongs", subtitle: "Spotify", artworkUrl: null };

  function playAll(tracks, shuffle) {
    if (!tracks.length) return;
    if (player.shuffle !== shuffle) toggleShuffle();
    playList(tracks, shuffle ? Math.floor(Math.random() * tracks.length) : 0);
  }
</script>

<section>
  <header>
    <div class="left">
      {#if page.type !== "home"}
        <button class="ghost" title="Zurück" onclick={back}><Icon name="previous" size={18} /></button>
      {/if}
      <form onsubmit={submitSearch}>
        <label class="search">
          <Icon name="search" size={18} />
          <input placeholder="Auf Spotify suchen" bind:value={query} />
        </label>
      </form>
    </div>
    {#if spotify.status === "loggedIn"}
      <AccountMenu account={spotify.account} onlogout={logout} />
    {:else if spotify.status === "loggedOut"}
      <button class="primary" onclick={login} disabled={spotify.loggingIn}>
        {spotify.loggingIn ? "Anmeldefenster offen…" : "Anmelden"}
      </button>
    {/if}
  </header>

  {#if spotify.error}
    <p class="error">{spotify.error}</p>
    <p class="dim">Keine Internetverbindung? Deine gespeicherte Musik findest du unter „Offline“.</p>
  {/if}
  {#if error}
    <p class="error">{error}</p>
    <p class="dim">Keine Internetverbindung? Deine gespeicherte Musik findest du unter „Offline“.</p>
  {/if}

  {#if page.type === "home"}
    {#if spotify.status === "checking"}
      <p class="dim">Verbinde mit Spotify…</p>
    {:else if spotify.status === "loggedOut"}
      <div class="hero">
        <Icon name="spotify" size={56} />
        <h1>Spotify</h1>
        <p>
          Melde dich mit deinem Spotify-Konto an, um deine Playlists, Alben und Lieblingssongs zu sehen. Abgespielt
          wird über YouTube Music – Spotify Premium brauchst du dafür nicht.
        </p>
        <button class="primary" onclick={login} disabled={spotify.loggingIn}>
          {spotify.loggingIn ? "Anmeldefenster ist offen…" : "Mit Spotify anmelden"}
        </button>
      </div>
    {:else}
      <div class="title-row">
        <h1>Deine Bibliothek</h1>
        <button class="ghost" title="Aktualisieren" onclick={loadLibrary}><Icon name="refresh" size={18} /></button>
      </div>
      {#if spotify.libraryLoading && !spotify.library}
        <p class="dim">Bibliothek wird geladen…</p>
      {:else if spotify.library}
        <h2>Playlists</h2>
        <div class="grid">
          <CollectionCard item={likedCard} onopen={openCollection} />
          {#each spotify.library.playlists as item (item.id)}
            <CollectionCard {item} onopen={openCollection} />
          {/each}
        </div>
        {#if spotify.library.albums.length}
          <h2>Alben</h2>
          <div class="grid">
            {#each spotify.library.albums as item (item.id)}
              <CollectionCard {item} onopen={openCollection} />
            {/each}
          </div>
        {/if}
      {/if}
    {/if}
  {:else if page.type === "collection" && collection}
    <div class="collection-head">
      <div class="big-art">
        {#if collection.artworkUrl}<img src={artworkSrc(collection.artworkUrl)} alt="" class:letterboxed={isLetterboxed(collection.artworkUrl)} />{:else}<Icon name="music" size={64} />{/if}
      </div>
      <div>
        <p class="kind">{collection.kind === "album" ? "Album" : "Playlist"}</p>
        <h1>{collection.title}</h1>
        <p class="dim">
          {[collection.subtitle, loading ? "Wird geladen…" : `${collection.tracks.length} Titel`]
            .filter(Boolean)
            .join(" · ")}
        </p>
        <div class="buttons">
          <button class="primary" onclick={() => playAll(collection.tracks, false)} disabled={loading}>
            <Icon name="play" size={18} /> Abspielen
          </button>
          <button class="secondary" onclick={() => playAll(collection.tracks, true)} disabled={loading}>
            <Icon name="shuffle" size={18} /> Zufall
          </button>
          <button
            class="secondary"
            onclick={() => openTransfer(collection, "spotify")}
            disabled={loading || !collection.tracks.length}
            title="Als neue Playlist bei einem anderen Dienst anlegen"
          >
            <Icon name="transfer" size={18} /> Übertragen
          </button>
          <button
            class="secondary"
            class:saved
            onclick={toggleOffline}
            disabled={loading || !collection.tracks.length}
            title={saved ? "Gespeicherte Songs wieder löschen" : "Songs herunterladen, damit sie ohne Internet laufen"}
          >
            {#if saved && saveProgress.pending}
              <Icon name="download" size={18} /> {saveProgress.done} / {saveProgress.total} gespeichert…
            {:else if saved}
              <Icon name="downloaded" size={18} /> Offline gespeichert
            {:else}
              <Icon name="download" size={18} /> Offline speichern
            {/if}
          </button>
        </div>
      </div>
    </div>
    <TrackList tracks={collection.tracks} covers />
  {:else if page.type === "artist"}
    <ArtistView artist={page.artist} {loading} onopen={openCollection} onartist={openArtistPage} />
  {:else if page.type === "search"}
    {#if loading && !results}
      <p class="dim">Suche läuft…</p>
    {:else if results}
      {#if results.tracks.length}
        <h2>Songs</h2>
        <TrackList tracks={results.tracks} covers single />
      {/if}
      {#if results.albums.length}
        <h2>Alben</h2>
        <div class="grid">
          {#each results.albums as item (item.id)}
            <CollectionCard {item} onopen={openCollection} />
          {/each}
        </div>
      {/if}
      {#if results.playlists.length}
        <h2>Playlists</h2>
        <div class="grid">
          {#each results.playlists as item (item.id)}
            <CollectionCard {item} onopen={openCollection} />
          {/each}
        </div>
      {/if}
      {#if results.artists.length}
        <h2>Künstler</h2>
        <div class="grid">
          {#each results.artists as other (other.id)}
            <button class="artist-card" onclick={() => openArtistPage({ id: other.id, title: other.name })}>
              <div class="avatar">
                {#if other.imageUrl}<img src={artworkSrc(other.imageUrl)} alt="" loading="lazy" />{:else}<Icon name="music" size={36} />{/if}
              </div>
              <strong>{other.name}</strong>
              <small>Künstler</small>
            </button>
          {/each}
        </div>
      {/if}
      {#if !results.tracks.length && !results.albums.length && !results.playlists.length && !results.artists.length}
        <p class="dim">Keine Treffer.</p>
      {/if}
    {/if}
  {/if}
</section>

<style>
  .account {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .account img {
    width: 34px;
    height: 34px;
    border-radius: 50%;
    object-fit: cover;
  }
  .artist-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 12px;
    border-radius: var(--radius);
    text-align: center;
    min-width: 0;
  }
  .artist-card:hover {
    background: var(--surface-2);
  }
  .avatar {
    width: 100%;
    aspect-ratio: 1;
    border-radius: 50%;
    overflow: hidden;
    background: var(--surface-2);
    color: var(--text-dim);
    display: grid;
    place-items: center;
    margin-bottom: 6px;
  }
  .avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .artist-card strong,
  .artist-card small {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .artist-card small {
    color: var(--text-dim);
    font-size: 13px;
  }
  section {
    padding: 20px 32px 32px;
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    margin-bottom: 24px;
  }
  .left {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .search input {
    width: 280px;
  }
  h1 {
    margin: 0;
    font-size: 30px;
  }
  h2 {
    margin: 28px 0 12px;
    font-size: 20px;
  }
  .title-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 4px;
    margin: 0 -12px;
  }
  .hero {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 10px;
    padding: 70px 20px;
    color: var(--text-dim);
  }
  .hero h1 {
    color: var(--text);
  }
  .hero p {
    max-width: 420px;
    margin: 0 0 10px;
  }
  .collection-head {
    display: flex;
    align-items: flex-end;
    gap: 24px;
    margin-bottom: 28px;
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
  .secondary.saved {
    color: var(--accent);
  }
  .buttons {
    display: flex;
    gap: 8px;
    margin-top: 14px;
  }
  .dim {
    color: var(--text-dim);
  }
  .error {
    color: var(--accent);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
