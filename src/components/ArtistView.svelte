<script>
  import { player, playList, toggleShuffle } from "../lib/player.svelte.js";
  import { artworkSrc } from "../lib/artwork.js";
  import CollectionCard from "./CollectionCard.svelte";
  import TrackList from "./TrackList.svelte";
  import Icon from "./Icon.svelte";

  /** artist: Daten aus ytm_artist (während des Ladens nur { id, name, loading }) */
  let { artist, loading, onopen, onartist } = $props();

  let showFullBio = $state(false);

  const subscribers = $derived.by(() => {
    const n = artist?.subscribers;
    if (!n) return null;
    const text =
      n >= 1_000_000
        ? `${(n / 1_000_000).toLocaleString("de-DE", { maximumFractionDigits: 1 })} Mio.`
        : n >= 1_000
          ? `${(n / 1_000).toLocaleString("de-DE", { maximumFractionDigits: 1 })} Tsd.`
          : n.toLocaleString("de-DE");
    return `${text} Abonnenten`;
  });

  function playAll(shuffle) {
    const tracks = artist?.tracks ?? [];
    if (!tracks.length) return;
    if (player.shuffle !== shuffle) toggleShuffle();
    playList(tracks, shuffle ? Math.floor(Math.random() * tracks.length) : 0);
  }
</script>

{#snippet grid(title, items)}
  {#if items?.length}
    <h2>{title}</h2>
    <div class="grid">
      {#each items as item (item.id)}
        <CollectionCard {item} {onopen} />
      {/each}
    </div>
  {/if}
{/snippet}

<div class="artist">
  <div class="hero" class:no-image={!artist?.imageUrl}>
    {#if artist?.imageUrl}
      <img src={artworkSrc(artist.imageUrl)} alt="" />
    {/if}
    <div class="hero-text">
      <p class="kind">Künstler</p>
      <h1>{artist?.name || "Künstler"}</h1>
      {#if subscribers}<p class="sub">{subscribers}</p>{/if}
    </div>
  </div>

  {#if artist?.loading || loading}
    <p class="dim">Wird geladen…</p>
  {:else if artist}
    <div class="buttons">
      <button class="primary" onclick={() => playAll(false)} disabled={!artist.tracks?.length}>
        <Icon name="play" size={18} /> Abspielen
      </button>
      <button class="secondary" onclick={() => playAll(true)} disabled={!artist.tracks?.length}>
        <Icon name="shuffle" size={18} /> Zufall
      </button>
      {#if artist.allSongsPlaylistId}
        <button
          class="secondary"
          onclick={() =>
            onopen({
              id: artist.allSongsPlaylistId,
              kind: "playlist",
              title: `${artist.name} – alle Songs`,
              artworkUrl: artist.imageUrl,
            })}
        >
          <Icon name="queue" size={18} /> Alle Songs
        </button>
      {/if}
    </div>

    {#if artist.tracks?.length}
      <h2>Beliebte Songs</h2>
      <TrackList tracks={artist.tracks} covers />
    {/if}

    {@render grid("Alben", artist.albums)}
    {@render grid("Singles & EPs", artist.singles)}
    {@render grid("Playlists", artist.playlists)}

    {#if artist.similar?.length}
      <h2>Ähnliche Künstler</h2>
      <div class="grid">
        {#each artist.similar as other (other.id)}
          <button class="artist-card" onclick={() => onartist({ id: other.id, title: other.name })}>
            <div class="avatar">
              {#if other.imageUrl}
                <img src={artworkSrc(other.imageUrl)} alt="" loading="lazy" />
              {:else}
                <Icon name="music" size={36} />
              {/if}
            </div>
            <strong>{other.name}</strong>
            <small>Künstler</small>
          </button>
        {/each}
      </div>
    {/if}

    {#if artist.description}
      <h2>Über {artist.name}</h2>
      <p class="bio" class:clamped={!showFullBio}>{artist.description}</p>
      <button class="more" onclick={() => (showFullBio = !showFullBio)}>
        {showFullBio ? "Weniger anzeigen" : "Mehr anzeigen"}
      </button>
    {/if}
  {/if}
</div>

<style>
  .hero {
    position: relative;
    height: 260px;
    margin: -4px 0 20px;
    border-radius: 14px;
    overflow: hidden;
    background: linear-gradient(135deg, var(--accent), #8b5cf6);
    display: flex;
    align-items: flex-end;
  }
  .hero img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
    object-position: center 25%;
  }
  .hero::after {
    content: "";
    position: absolute;
    inset: 0;
    background: linear-gradient(to top, rgba(0, 0, 0, 0.75), rgba(0, 0, 0, 0.05) 60%);
  }
  .hero-text {
    position: relative;
    z-index: 1;
    padding: 24px 28px;
    color: #fff;
  }
  .kind {
    margin: 0 0 4px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    opacity: 0.85;
  }
  h1 {
    margin: 0;
    font-size: 48px;
    line-height: 1.05;
  }
  .sub {
    margin: 6px 0 0;
    opacity: 0.85;
  }
  .buttons {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-bottom: 8px;
  }
  h2 {
    margin: 28px 0 12px;
    font-size: 20px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 4px;
    margin: 0 -12px;
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
  .bio {
    margin: 0;
    color: var(--text-dim);
    line-height: 1.6;
    white-space: pre-line;
    max-width: 760px;
  }
  .bio.clamped {
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .more {
    margin-top: 6px;
    color: var(--text-dim);
    font-weight: 600;
  }
  .more:hover {
    color: var(--text);
  }
  .dim {
    color: var(--text-dim);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
