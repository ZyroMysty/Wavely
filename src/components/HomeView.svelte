<script>
  import { onMount } from "svelte";
  import { home, loadCharts, recentTracks, topTracks, topArtists, greeting } from "../lib/home.svelte.js";
  import { history } from "../lib/history.svelte.js";
  import { playList, playMix } from "../lib/player.svelte.js";
  import { ytm } from "../lib/ytm.svelte.js";
  import { spotify } from "../lib/spotify.svelte.js";
  import { soundcloud } from "../lib/soundcloud.svelte.js";
  import { openIn, openArtist, showToast } from "../lib/ui.svelte.js";
  import Cover from "./Cover.svelte";
  import CollectionCard from "./CollectionCard.svelte";
  import TrackList from "./TrackList.svelte";
  import Icon from "./Icon.svelte";
  import { artworkSrc } from "../lib/artwork.js";
  import { playlists as wavelyPlaylists, coversOf } from "../lib/playlists.svelte.js";
  import { ui } from "../lib/ui.svelte.js";

  onMount(() => loadCharts());

  // Neu berechnen, wenn sich der Verlauf ändert
  const recent = $derived((history.entries, recentTracks(12)));
  const top = $derived((history.entries, topTracks(30, 10)));
  const artists = $derived((history.entries, topArtists(30, 6)));
  const sourceIcon = { youtube: "youtube", spotify: "spotify", soundcloud: "soundcloud", local: "folder" };

  // Playlists aus allen verbundenen Diensten (die ersten paar je Dienst)
  const playlists = $derived(
    [
      ...wavelyPlaylists.list.slice(0, 6).map((p) => ({
        id: p.id,
        title: p.name,
        subtitle: `Wavely · ${p.items.length} ${p.items.length === 1 ? "Song" : "Songs"}`,
        artworkUrl: coversOf(p)[0]?.artworkUrl ?? null,
        source: "wavely",
      })),
      ...(ytm.library?.playlists ?? []).slice(0, 6).map((p) => ({ ...p, source: "youtube" })),
      ...(spotify.library?.playlists ?? []).slice(0, 6).map((p) => ({ ...p, source: "spotify" })),
      ...(soundcloud.library?.playlists ?? []).slice(0, 4).map((p) => ({ ...p, source: "soundcloud" })),
    ].slice(0, 12),
  );

  let mixLoading = $state(null);
  async function startMix(artist) {
    mixLoading = artist.name;
    try {
      await playMix(artist.seed);
    } catch (e) {
      showToast(`Mix konnte nicht gestartet werden: ${e}`, true);
    } finally {
      mixLoading = null;
    }
  }

  const openPlaylist = (item) =>
    item.source === "wavely" ? ((ui.playlistId = item.id), (ui.view = "playlist")) : openIn(item.source, { type: "collection", kind: item.kind ?? "playlist", id: item.id, title: item.title });
  const openChartsAlbum = (item) => openIn("youtube", { type: "collection", kind: "album", id: item.id, title: item.title });
  const charts = $derived(home.charts);
  const chartTracks = $derived((charts?.topTracks ?? []).map((t) => ({ ...t, source: "youtube" })));
  const trending = $derived((charts?.trending ?? []).map((t) => ({ ...t, source: "youtube" })));
</script>

<div class="home">
  <h1>{greeting()}</h1>

  {#if recent.length}
    <section>
      <h2>Weiterhören</h2>
      <div class="tiles">
        {#each recent.slice(0, 8) as track, i (track.source + track.id)}
          <button class="tile" onclick={() => playList(recent, i)} title="{track.title} · {track.artist ?? ''}">
            <Cover {track} size={56} />
            <span class="text">
              <strong>{track.title}</strong>
              <small>{track.artist ?? ""}</small>
            </span>
            <span class="play"><Icon name="play" size={18} /></span>
          </button>
        {/each}
      </div>
    </section>
  {:else}
    <section class="welcome">
      <p>Hier erscheinen bald deine zuletzt gehörten Songs, deine Lieblingssongs und Mixes, die zu dir passen.</p>
      <p class="dim">Hör einfach los – zum Beispiel mit den Charts unten.</p>
    </section>
  {/if}

  {#if artists.length}
    <section>
      <h2>Mixes für dich</h2>
      <div class="mixes">
        {#each artists as artist (artist.name)}
          <button class="mix" onclick={() => startMix(artist)} disabled={mixLoading === artist.name}>
            <div class="mix-art">
              {#if artist.seed.artworkUrl}<img src={artworkSrc(artist.seed.artworkUrl)} alt="" loading="lazy" />{/if}
              <span class="badge">{mixLoading === artist.name ? "Lädt…" : "Mix"}</span>
            </div>
            <strong>{artist.name} Mix</strong>
            <small>{artist.plays}× gehört</small>
          </button>
        {/each}
      </div>
    </section>
  {/if}

  {#if top.length >= 3}
    <section>
      <h2>Deine Top-Songs <small>letzte 30 Tage</small></h2>
      <ol class="top">
        {#each top as { track, plays }, i (track.source + track.id)}
          <li>
            <button class="top-row" onclick={() => playList(top.map((t) => t.track), i)}>
              <span class="rank">{i + 1}</span>
              <Cover {track} size={40} />
              <span class="text">
                <strong>{track.title}</strong>
                <small>{track.artist ?? ""}</small>
              </span>
              <Icon name={sourceIcon[track.source] ?? "music"} size={16} />
              <span class="plays">{plays}×</span>
            </button>
          </li>
        {/each}
      </ol>
    </section>
  {/if}

  {#if playlists.length}
    <section>
      <h2>Deine Playlists</h2>
      <div class="grid">
        {#each playlists as item (item.source + item.id)}
          <CollectionCard {item} onopen={openPlaylist} />
        {/each}
      </div>
    </section>
  {/if}

  {#if home.loading && !charts}
    <p class="dim">Charts werden geladen…</p>
  {:else if charts}
    {#if chartTracks.length}
      <section>
        <h2>Charts Deutschland</h2>
        <TrackList tracks={chartTracks.slice(0, 10)} covers />
      </section>
    {/if}
    {#if charts.newAlbums.length}
      <section>
        <h2>Neu erschienen</h2>
        <div class="grid">
          {#each charts.newAlbums.slice(0, 12) as item (item.id)}
            <CollectionCard {item} onopen={openChartsAlbum} />
          {/each}
        </div>
      </section>
    {/if}
    {#if trending.length}
      <section>
        <h2>Gerade im Trend</h2>
        <TrackList tracks={trending.slice(0, 10)} covers />
      </section>
    {/if}
    {#if charts.topArtists.length}
      <section>
        <h2>Angesagte Künstler</h2>
        <div class="artists">
          {#each charts.topArtists as artist (artist.id)}
            <button class="artist" onclick={() => openArtist(artist)}>
              <div class="avatar">
                {#if artist.imageUrl}<img src={artworkSrc(artist.imageUrl)} alt="" loading="lazy" />{/if}
              </div>
              <span>{artist.name}</span>
            </button>
          {/each}
        </div>
      </section>
    {/if}
  {:else if home.error}
    <p class="dim">Charts konnten nicht geladen werden (kein Internet?).</p>
  {/if}
</div>

<style>
  .home {
    padding: 32px 40px 48px;
    max-width: 1400px;
  }
  h1 {
    margin: 0 0 8px;
    font-size: 32px;
  }
  h2 {
    margin: 32px 0 14px;
    font-size: 20px;
  }
  h2 small {
    color: var(--text-dim);
    font-weight: 400;
    font-size: 13px;
    margin-left: 8px;
  }
  .dim {
    color: var(--text-dim);
  }
  .welcome p {
    margin: 12px 0 0;
    max-width: 560px;
    line-height: 1.5;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: 10px;
  }
  .tile {
    display: flex;
    align-items: center;
    gap: 12px;
    padding-right: 12px;
    background: var(--surface);
    border-radius: 8px;
    overflow: hidden;
    text-align: left;
    min-width: 0;
  }
  .tile:hover {
    background: var(--surface-2);
  }
  .tile .play {
    margin-left: auto;
    width: 32px;
    height: 32px;
    flex-shrink: 0;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
    display: grid;
    place-items: center;
    opacity: 0;
    transition: opacity 0.15s;
  }
  .tile:hover .play {
    opacity: 1;
  }
  .text {
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
    font-weight: 600;
  }
  .text small {
    color: var(--text-dim);
    font-size: 13px;
  }
  .mixes,
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(170px, 1fr));
    gap: 4px;
    margin: 0 -12px;
  }
  .mix {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px;
    border-radius: var(--radius);
    text-align: left;
    min-width: 0;
  }
  .mix:hover {
    background: var(--surface-2);
  }
  .mix-art {
    position: relative;
    aspect-ratio: 1;
    border-radius: 8px;
    overflow: hidden;
    background: linear-gradient(135deg, var(--accent), #6d28d9);
    margin-bottom: 6px;
  }
  .mix-art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    opacity: 0.75;
    mix-blend-mode: luminosity;
  }
  .mix-art .badge {
    position: absolute;
    left: 10px;
    bottom: 10px;
    padding: 4px 10px;
    border-radius: 999px;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    font-size: 12px;
    font-weight: 700;
  }
  .mix strong,
  .mix small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mix small {
    color: var(--text-dim);
    font-size: 13px;
  }
  .top {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(380px, 1fr));
    gap: 2px 16px;
  }
  .top-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 10px;
    border-radius: 8px;
    text-align: left;
    color: var(--text-dim);
  }
  .top-row:hover {
    background: var(--surface-2);
  }
  .top-row .text {
    flex: 1;
    color: var(--text);
  }
  .rank {
    width: 20px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .plays {
    width: 34px;
    text-align: right;
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  .artists {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(130px, 1fr));
    gap: 8px;
  }
  .artist {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 10px;
    border-radius: var(--radius);
    min-width: 0;
  }
  .artist:hover {
    background: var(--surface-2);
  }
  .avatar {
    width: 100%;
    aspect-ratio: 1;
    border-radius: 50%;
    overflow: hidden;
    background: var(--surface-2);
  }
  .avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .artist span {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 14px;
  }
</style>
