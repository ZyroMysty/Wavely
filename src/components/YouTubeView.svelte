<script>
  import { ytm, login, logout, loadLibrary, fetchCollection, search } from "../lib/ytm.svelte.js";
  import { player, playList, toggleShuffle } from "../lib/player.svelte.js";
  import { artworkSrc, isLetterboxed } from "../lib/artwork.js";
  import CollectionCard from "./CollectionCard.svelte";
  import TrackList from "./TrackList.svelte";
  import Icon from "./Icon.svelte";

  // Navigation innerhalb von YouTube Music
  let page = $state({ type: "home" }); // home | collection | search
  let history = [];
  let query = $state("");
  let loading = $state(false);
  let error = $state(null);
  let collection = $state(null);
  let results = $state(null);

  function go(next) {
    history.push(page);
    page = next;
  }

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
    go({ type: "collection" });
    collection = { ...item, tracks: [] };
    run(async () => {
      const detail = await fetchCollection(item.kind, item.id);
      collection = {
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
    if (page.type !== "search") go({ type: "search" });
    results = null;
    run(async () => {
      results = await search(q);
    });
  }

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
          <input placeholder="Songs, Alben, Playlists suchen" bind:value={query} />
        </label>
      </form>
    </div>
    {#if ytm.status === "loggedIn"}
      <button class="secondary" onclick={logout}>Abmelden</button>
    {/if}
  </header>

  {#if ytm.error}<p class="error">{ytm.error}</p>{/if}
  {#if error}<p class="error">{error}</p>{/if}

  {#if page.type === "home"}
    {#if ytm.status === "checking"}
      <p class="dim">Verbinde mit YouTube Music…</p>
    {:else if ytm.status === "loggedOut"}
      <div class="hero">
        <Icon name="youtube" size={56} />
        <h1>YouTube Music</h1>
        <p>Melde dich an, um deine Playlists, Alben und Lieblingssongs in Wavely zu sehen. Suchen geht auch ohne Anmeldung.</p>
        <button class="primary" onclick={login} disabled={ytm.loggingIn}>
          {ytm.loggingIn ? "Anmeldefenster ist offen…" : "Mit Google anmelden"}
        </button>
      </div>
    {:else}
      <div class="title-row">
        <h1>Deine Bibliothek</h1>
        <button class="ghost" title="Aktualisieren" onclick={loadLibrary}><Icon name="refresh" size={18} /></button>
      </div>
      {#if ytm.libraryLoading && !ytm.library}
        <p class="dim">Bibliothek wird geladen…</p>
      {:else if ytm.library}
        <h2>Playlists</h2>
        <div class="grid">
          {#each ytm.library.playlists as item (item.id)}
            <CollectionCard {item} onopen={openCollection} />
          {/each}
        </div>
        {#if ytm.library.albums.length}
          <h2>Alben</h2>
          <div class="grid">
            {#each ytm.library.albums as item (item.id)}
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
        </div>
      </div>
    </div>
    <TrackList tracks={collection.tracks} covers />
  {:else if page.type === "search"}
    {#if loading && !results}
      <p class="dim">Suche läuft…</p>
    {:else if results}
      {#if results.tracks.length}
        <h2>Songs</h2>
        <TrackList tracks={results.tracks} covers />
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
      {#if !results.tracks.length && !results.albums.length && !results.playlists.length}
        <p class="dim">Keine Treffer.</p>
      {/if}
    {/if}
  {/if}
</section>

<style>
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
