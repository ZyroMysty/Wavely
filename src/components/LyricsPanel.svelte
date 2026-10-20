<script>
  import { player, currentTrack, seek } from "../lib/player.svelte.js";
  import { lyricsFor } from "../lib/lyrics.svelte.js";

  const track = $derived(currentTrack());
  let lyrics = $state(null);
  let status = $state("idle"); // idle | loading | done | error
  let container = $state();
  /** Nach manuellem Scrollen kurz nicht automatisch mitscrollen */
  let userScrolledAt = 0;

  $effect(() => {
    const t = track;
    lyrics = null;
    if (!t) {
      status = "idle";
      return;
    }
    status = "loading";
    lyricsFor(t)
      .then((result) => {
        if (currentTrack()?.qid !== t.qid) return;
        lyrics = result;
        status = "done";
      })
      .catch(() => {
        if (currentTrack()?.qid === t.qid) status = "error";
      });
  });

  // Aktuelle Zeile: letzte, deren Zeitstempel erreicht ist (leicht vorgezogen)
  const active = $derived.by(() => {
    const lines = lyrics?.lines;
    if (!lines) return -1;
    let index = -1;
    for (let i = 0; i < lines.length && lines[i].time <= player.position + 0.25; i++) index = i;
    return index;
  });

  $effect(() => {
    if (active < 0 || !container || Date.now() - userScrolledAt < 4000) return;
    container
      .querySelector(`[data-line="${active}"]`)
      ?.scrollIntoView({ block: "center", behavior: "smooth" });
  });
</script>

<div class="lyrics" bind:this={container} onwheel={() => (userScrolledAt = Date.now())}>
  {#if !track}
    <p class="info">Spiel einen Song ab, um den Liedtext zu sehen.</p>
  {:else if status === "loading"}
    <p class="info">Liedtext wird gesucht…</p>
  {:else if status === "error"}
    <p class="info">Liedtext konnte nicht geladen werden.</p>
  {:else if !lyrics}
    <p class="info">Für „{track.title}“ wurde kein Liedtext gefunden.</p>
  {:else if lyrics.lines}
    {#each lyrics.lines as line, i}
      <button
        class="line"
        class:active={i === active}
        class:past={i < active}
        data-line={i}
        onclick={() => seek(line.time)}
      >
        {line.text || "♪"}
      </button>
    {/each}
    <p class="source">Quelle: {lyrics.source}</p>
  {:else}
    <p class="plain">{lyrics.plain}</p>
    <p class="source">Quelle: {lyrics.source} · nicht synchronisiert</p>
  {/if}
</div>

<style>
  .lyrics {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-bottom: 40vh;
  }
  .line {
    text-align: left;
    font-size: 20px;
    font-weight: 700;
    line-height: 1.35;
    padding: 6px 8px;
    border-radius: 8px;
    color: var(--text-dim);
    opacity: 0.55;
    transition:
      color 0.2s,
      opacity 0.2s;
  }
  .line:hover {
    opacity: 1;
    background: var(--surface-2);
  }
  .line.past {
    opacity: 0.4;
  }
  .line.active {
    color: var(--text);
    opacity: 1;
  }
  .plain {
    white-space: pre-wrap;
    font-size: 16px;
    line-height: 1.7;
    margin: 0 8px;
  }
  .info {
    color: var(--text-dim);
    margin: 20px 8px;
    line-height: 1.5;
  }
  .source {
    margin: 24px 8px 0;
    font-size: 12px;
    color: var(--text-dim);
  }
</style>
