<script>
  import { library, addFolder, refreshLibrary } from "../lib/library.svelte.js";
  import { player, playList, currentTrack, toggleShuffle } from "../lib/player.svelte.js";
  import { formatTime } from "../lib/format.js";
  import Icon from "./Icon.svelte";

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
    <div class="table">
      <div class="row head">
        <span>#</span><span>Titel</span><span>Album</span><span class="right">Dauer</span>
      </div>
      {#each filtered as track, i (track.id)}
        {@const active = currentTrack()?.id === track.id}
        <div
          class="row"
          class:active
          role="button"
          tabindex="-1"
          ondblclick={() => playList(filtered, i)}
          onkeydown={(e) => e.key === "Enter" && playList(filtered, i)}
        >
          <span class="num">
            {#if active && player.playing}<Icon name="volume" size={16} />{:else}{i + 1}{/if}
          </span>
          <span class="title">
            <strong>{track.title}</strong>
            <small>{track.artist ?? "Unbekannter Künstler"}</small>
          </span>
          <span class="dim ellipsis">{track.album ?? ""}</span>
          <span class="dim right">{formatTime(track.duration)}</span>
        </div>
      {/each}
    </div>
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
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    background: var(--surface-2);
    border-radius: 999px;
    padding: 0 14px;
    height: 38px;
    color: var(--text-dim);
  }
  .search input {
    background: none;
    border: none;
    outline: none;
    width: 180px;
  }
  .primary {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 38px;
    padding: 0 18px;
    border-radius: 999px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .primary:hover {
    filter: brightness(1.1);
  }
  .ghost {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--text-dim);
  }
  .ghost:hover {
    background: var(--hover);
    color: var(--text);
  }
  .table {
    display: flex;
    flex-direction: column;
  }
  .row {
    display: grid;
    grid-template-columns: 40px minmax(0, 2fr) minmax(0, 1fr) 70px;
    gap: 16px;
    align-items: center;
    padding: 8px 12px;
    border-radius: 8px;
  }
  .row:not(.head):hover {
    background: var(--surface-2);
  }
  .row.head {
    color: var(--text-dim);
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    border-bottom: 1px solid var(--border);
    border-radius: 0;
    margin-bottom: 6px;
  }
  .row.active {
    background: var(--accent-soft);
  }
  .row.active strong,
  .row.active .num {
    color: var(--accent);
  }
  .num {
    color: var(--text-dim);
    text-align: center;
    display: grid;
    place-items: center;
  }
  .title {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .title strong,
  .title small,
  .ellipsis {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title strong {
    font-weight: 500;
  }
  .title small {
    color: var(--text-dim);
    font-size: 13px;
  }
  .dim {
    color: var(--text-dim);
  }
  .right {
    text-align: right;
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
