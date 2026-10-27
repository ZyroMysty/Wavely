<script>
  import { loadPlays, summarize, statsFlush } from "../lib/stats.svelte.js";
  import { playList } from "../lib/player.svelte.js";
  import { artworkSrc } from "../lib/artwork.js";
  import Cover from "./Cover.svelte";
  import Icon from "./Icon.svelte";

  const RANGES = [
    { id: "week", label: "7 Tage", days: 7 },
    { id: "month", label: "30 Tage", days: 30 },
    { id: "year", label: "Dieses Jahr" },
    { id: "all", label: "Insgesamt" },
  ];
  const SOURCE_NAMES = { youtube: "YouTube Music", spotify: "Spotify", soundcloud: "SoundCloud", local: "Lokal" };
  const SOURCE_COLORS = { youtube: "#ff4f4f", spotify: "#1ed760", soundcloud: "#ff7700", local: "#8b5cf6" };
  const WEEKDAYS = ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"];

  let range = $state("month");
  let recap = $state(null);
  let loading = $state(true);

  function since(id) {
    const r = RANGES.find((x) => x.id === id);
    if (r.days) return Date.now() - r.days * 86_400_000;
    if (id === "year") return new Date(new Date().getFullYear(), 0, 1).getTime();
    return 0;
  }

  $effect(() => {
    const id = range;
    loading = true;
    statsFlush();
    loadPlays(since(id))
      .then((plays) => {
        if (range === id) recap = summarize(plays, since(id));
      })
      .catch(() => (recap = null))
      .finally(() => (loading = false));
  });

  const asTrack = (s) => ({
    source: s.src,
    id: s.id,
    title: s.title,
    artist: s.artist,
    album: s.album,
    artworkUrl: s.art,
    duration: s.d,
    // lokale Songs: Pfad steht in der ID
    path: s.src === "local" ? s.id : undefined,
  });

  const hoursMax = $derived(recap ? Math.max(1, ...recap.hours) : 1);
  const weekMax = $derived(recap ? Math.max(1, ...recap.weekdays) : 1);
  const peakHour = $derived(recap ? recap.hours.indexOf(Math.max(...recap.hours)) : 0);
  const peakDay = $derived(recap ? recap.weekdays.indexOf(Math.max(...recap.weekdays)) : 0);
  const fmt = (n) => n.toLocaleString("de-DE");
  const daytime = (h) => (h < 5 ? "nachts" : h < 11 ? "morgens" : h < 14 ? "mittags" : h < 18 ? "nachmittags" : h < 22 ? "abends" : "spätabends");
</script>

<div class="recap">
  <header>
    <h1>Dein Rückblick</h1>
    <div class="ranges">
      {#each RANGES as r (r.id)}
        <button class:on={range === r.id} onclick={() => (range = r.id)}>{r.label}</button>
      {/each}
    </div>
  </header>

  {#if loading && !recap}
    <p class="dim">Wird ausgewertet…</p>
  {:else if !recap || !recap.plays}
    <div class="empty">
      <Icon name="insights" size={48} />
      <p>Für diesen Zeitraum gibt es noch keine Daten.</p>
      <p class="dim">Wavely zählt jeden Song, den du mindestens 10 Sekunden hörst – hör einfach los.</p>
    </div>
  {:else}
    <section class="hero">
      <div class="big">
        <strong>{fmt(recap.minutes)}</strong>
        <span>Minuten Musik</span>
      </div>
      <div class="facts">
        <div><strong>{fmt(recap.songCount)}</strong><span>{recap.songCount === 1 ? "Song" : "verschiedene Songs"}</span></div>
        <div><strong>{fmt(recap.artistCount)}</strong><span>Künstler</span></div>
        <div><strong>{fmt(recap.days)}</strong><span>{recap.days === 1 ? "Tag" : "Tage"} mit Musik</span></div>
        <div><strong>{fmt(recap.streak)}</strong><span>{recap.streak === 1 ? "Tag" : "Tage"} am Stück (Rekord)</span></div>
      </div>
    </section>

    <div class="columns">
      <section>
        <h2>Top-Songs</h2>
        <ol class="list">
          {#each recap.topSongs as song, i (song.src + song.id)}
            <li>
              <button onclick={() => playList(recap.topSongs.map(asTrack), i)}>
                <span class="rank" class:first={i === 0}>{i + 1}</span>
                <Cover track={asTrack(song)} size={44} />
                <span class="text">
                  <strong>{song.title}</strong>
                  <small>{song.artist ?? ""}</small>
                </span>
                <span class="count">{song.plays}×</span>
              </button>
            </li>
          {/each}
        </ol>
      </section>

      <section>
        <h2>Top-Künstler</h2>
        <ol class="list">
          {#each recap.topArtists as artist, i (artist.name)}
            <li>
              <div class="row">
                <span class="rank" class:first={i === 0}>{i + 1}</span>
                <div class="avatar">{#if artist.art}<img src={artworkSrc(artist.art)} alt="" loading="lazy" />{/if}</div>
                <span class="text">
                  <strong>{artist.name}</strong>
                  <small>{artist.songs} {artist.songs === 1 ? "Song" : "Songs"}</small>
                </span>
                <span class="count">{fmt(Math.round(artist.seconds / 60))} Min.</span>
              </div>
            </li>
          {/each}
        </ol>
      </section>
    </div>

    <div class="columns">
      <section>
        <h2>Wann du hörst</h2>
        <p class="dim">Am meisten {daytime(peakHour)} (um {peakHour} Uhr) und am liebsten {["montags", "dienstags", "mittwochs", "donnerstags", "freitags", "samstags", "sonntags"][peakDay]}.</p>
        <div class="hours" role="img" aria-label="Hörzeit nach Uhrzeit">
          {#each recap.hours as value, h (h)}
            <div class="bar" title="{h} Uhr: {Math.round(value / 60)} Min.">
              <div style:height="{(value / hoursMax) * 100}%" class:peak={h === peakHour}></div>
            </div>
          {/each}
        </div>
        <div class="axis"><span>0 Uhr</span><span>6</span><span>12</span><span>18</span><span>23</span></div>
        <div class="weekdays">
          {#each recap.weekdays as value, d (d)}
            <div class="day" title="{WEEKDAYS[d]}: {Math.round(value / 60)} Min.">
              <div class="track"><div style:width="{(value / weekMax) * 100}%" class:peak={d === peakDay}></div></div>
              <span>{WEEKDAYS[d]}</span>
            </div>
          {/each}
        </div>
      </section>

      <section>
        <h2>Woher deine Musik kommt</h2>
        <div class="sources">
          {#each recap.sources as s (s.src)}
            <div style:flex={s.share} style:background={SOURCE_COLORS[s.src] ?? "var(--accent)"} title="{SOURCE_NAMES[s.src] ?? s.src}: {Math.round(s.share * 100)} %"></div>
          {/each}
        </div>
        <ul class="legend">
          {#each recap.sources as s (s.src)}
            <li>
              <span class="dot" style:background={SOURCE_COLORS[s.src] ?? "var(--accent)"}></span>
              {SOURCE_NAMES[s.src] ?? s.src}
              <strong>{Math.round(s.share * 100)} %</strong>
            </li>
          {/each}
        </ul>
      </section>
    </div>
  {/if}
</div>

<style>
  .recap {
    padding: 32px 40px 48px;
    max-width: 1300px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }
  h1 {
    margin: 0;
    font-size: 32px;
  }
  h2 {
    margin: 0 0 14px;
    font-size: 20px;
  }
  .dim {
    color: var(--text-dim);
  }
  .ranges {
    display: flex;
    gap: 4px;
    padding: 4px;
    border-radius: 999px;
    background: var(--surface);
  }
  .ranges button {
    padding: 6px 14px;
    border-radius: 999px;
    color: var(--text-dim);
    font-size: 14px;
  }
  .ranges button.on {
    background: var(--accent);
    color: #fff;
  }
  .empty {
    margin-top: 60px;
    text-align: center;
    color: var(--text-dim);
  }
  .empty p:first-of-type {
    color: var(--text);
    font-size: 18px;
  }
  .hero {
    margin: 28px 0 36px;
    padding: 32px;
    border-radius: 18px;
    background: linear-gradient(135deg, var(--accent), #6d28d9);
    color: #fff;
    display: flex;
    align-items: center;
    gap: 40px;
    flex-wrap: wrap;
  }
  .big strong {
    display: block;
    font-size: 64px;
    line-height: 1;
    font-weight: 800;
    font-variant-numeric: tabular-nums;
  }
  .big span {
    font-size: 18px;
    opacity: 0.9;
  }
  .facts {
    display: grid;
    grid-template-columns: repeat(2, minmax(150px, 1fr));
    gap: 14px 32px;
  }
  .facts strong {
    display: block;
    font-size: 28px;
    font-weight: 700;
  }
  .facts span {
    opacity: 0.85;
    font-size: 14px;
  }
  .columns {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
    gap: 32px 40px;
    margin-bottom: 36px;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .list button,
  .list .row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 8px;
    border-radius: 8px;
    text-align: left;
  }
  .list button:hover {
    background: var(--surface-2);
  }
  .rank {
    width: 22px;
    text-align: right;
    color: var(--text-dim);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .rank.first {
    color: var(--accent);
  }
  .avatar {
    width: 44px;
    height: 44px;
    flex-shrink: 0;
    border-radius: 50%;
    overflow: hidden;
    background: var(--surface-2);
  }
  .avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
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
  .text small {
    color: var(--text-dim);
    font-size: 13px;
  }
  .count {
    color: var(--text-dim);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  .hours {
    display: flex;
    align-items: flex-end;
    gap: 3px;
    height: 120px;
    margin-top: 14px;
  }
  .bar {
    flex: 1;
    height: 100%;
    display: flex;
    align-items: flex-end;
  }
  .bar div {
    width: 100%;
    min-height: 2px;
    border-radius: 3px 3px 0 0;
    background: var(--hover);
  }
  .bar div.peak,
  .track div.peak {
    background: var(--accent);
  }
  .axis {
    display: flex;
    justify-content: space-between;
    color: var(--text-dim);
    font-size: 11px;
    margin-top: 4px;
  }
  .weekdays {
    margin-top: 18px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .day {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
    color: var(--text-dim);
  }
  .day span {
    order: -1;
    width: 22px;
  }
  .track {
    flex: 1;
    height: 8px;
    border-radius: 4px;
    background: var(--surface);
    overflow: hidden;
  }
  .track div {
    height: 100%;
    border-radius: 4px;
    background: var(--hover);
  }
  .sources {
    display: flex;
    height: 18px;
    border-radius: 9px;
    overflow: hidden;
    gap: 2px;
  }
  .legend {
    list-style: none;
    padding: 0;
    margin: 16px 0 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .legend strong {
    margin-left: auto;
    font-variant-numeric: tabular-nums;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
</style>
