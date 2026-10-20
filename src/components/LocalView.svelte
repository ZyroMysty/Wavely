<script>
  import { library, addFolder, refreshLibrary } from "../lib/library.svelte.js";
  import { player, playList, toggleShuffle } from "../lib/player.svelte.js";
  import Icon from "./Icon.svelte";
  import TrackList from "./TrackList.svelte";

  let query = $state("");

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return library.tracks;
    return library.tracks.filter((t) =>
      [t.title, t.artist, t.album].some((v) => v?.toLowerCase().includes(q)),
    );
  });

  function playAll(shuffle) {
    if (!filtered.length) return;
    if (player.shuffle !== shuffle) toggleShuffle();
    playList(filtered, shuffle ? Math.floor(Math.random() * filtered.length) : 0);
  }
</script>

<section>
  <header>
    <div>
      <h1>Lokale Musik</h1>
      <p class="sub">
        {#if library.loading}Wird geladen…{:else}{library.tracks.length} Titel{/if}
      </p>
    </div>
    <div class="actions">
      <label class="search">
        <Icon name="search" size={18} />
        <input placeholder="Suchen" bind:value={query} />
      </label>
      <button class="ghost" title="Neu einlesen" onclick={refreshLibrary}>
        <Icon name="refresh" size={18} />
      </button>
      <button class="ghost" title="Zufallswiedergabe" onclick={() => playAll(true)}>
        <Icon name="shuffle" size={18} />
      </button>
      <button class="primary" onclick={() => playAll(false)}>
        <Icon name="play" size={18} /> Abspielen
      </button>
    </div>
  </header>

  {#if library.error}
    <p class="error">{library.error}</p>
  {/if}

  {#if !library.folders.length && !library.loading}
    <div class="empty">
      <Icon name="folder" size={48} />
      <h2>Füge deine Musik hinzu</h2>
      <p>Wähle einen Ordner mit MP3-, FLAC- oder M4A-Dateien aus.</p>
      <button class="primary" onclick={addFolder}><Icon name="plus" size={18} /> Ordner hinzufügen</button>
    </div>
  {:else}
    <TrackList tracks={filtered} />
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
    margin-bottom: 20px;
  }
  h1 {
    margin: 0;
    font-size: 30px;
  }
  .sub {
    margin: 4px 0 0;
    color: var(--text-dim);
  }
  .actions {
    display: flex;
    gap: 8px;
    align-items: center;
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
    margin: 0 0 12px;
  }
  .error {
    color: var(--accent);
  }
</style>
