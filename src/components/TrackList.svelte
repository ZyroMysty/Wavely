<script>
  import { untrack } from "svelte";
  import { player, playList, currentTrack } from "../lib/player.svelte.js";
  import { formatTime } from "../lib/format.js";
  import { openTrackMenu, openAlbum } from "../lib/ui.svelte.js";
  import ArtistLinks from "./ArtistLinks.svelte";
  import { offline, isOffline } from "../lib/offline.svelte.js";
  import Cover from "./Cover.svelte";
  import Icon from "./Icon.svelte";

  /**
   * covers: kleine Cover in jeder Zeile anzeigen (für YouTube Music)
   * single: nur den angeklickten Song spielen (danach übernimmt Autoplay), z. B. bei Suchergebnissen
   */
  let { tracks, covers = false, single = false, onremove = undefined, onmove = undefined } = $props();

  const play = (i) => (single ? playList([tracks[i]]) : playList(tracks, i));

  // Umsortieren per Ziehen (nur wenn `onmove` übergeben wird, z. B. in Wavely-Playlists)
  let dragFrom = $state(null);
  let dragOver = $state(null);
  function drop(i) {
    if (dragFrom !== null && dragFrom !== i) onmove(dragFrom, i);
    dragFrom = dragOver = null;
  }

  // Lange Listen (z. B. „Gefällt mir“ mit Tausenden Songs): nur die sichtbaren Zeilen aufbauen,
  // der Rest ist ein leerer Platzhalter gleicher Höhe – spart Arbeitsspeicher und Zeit
  const VIRTUAL_FROM = 150;
  const BUFFER = 30;
  const virtual = $derived(tracks.length > VIRTUAL_FROM);
  let rowsElement = $state();
  let rowHeight = $state(52);
  let range = $state({ start: 0, end: 80 });
  const offset = $derived(virtual ? Math.min(range.start, tracks.length) : 0);
  const visible = $derived(virtual ? tracks.slice(offset, range.end) : tracks);

  function scrollParent(element) {
    for (let e = element.parentElement; e; e = e.parentElement) {
      if (/(auto|scroll)/.test(getComputedStyle(e).overflowY)) return e;
    }
    return document.scrollingElement;
  }

  $effect(() => {
    if (!virtual || !rowsElement) return;
    const count = tracks.length;
    const scroller = scrollParent(rowsElement);
    let frame = 0;
    const update = () => {
      frame = 0;
      const row = rowsElement.querySelector(".row");
      if (row?.offsetHeight) rowHeight = row.offsetHeight;
      const top = rowsElement.getBoundingClientRect().top - scroller.getBoundingClientRect().top;
      const first = Math.floor(-top / rowHeight);
      const start = Math.max(0, first - BUFFER);
      const end = Math.min(count, Math.max(0, first) + Math.ceil(scroller.clientHeight / rowHeight) + BUFFER);
      if (start !== range.start || end !== range.end) range = { start, end };
    };
    const schedule = () => (frame ||= requestAnimationFrame(update));
    untrack(update);
    scroller.addEventListener("scroll", schedule, { passive: true });
    const resize = new ResizeObserver(schedule);
    resize.observe(scroller);
    return () => {
      scroller.removeEventListener("scroll", schedule);
      resize.disconnect();
      cancelAnimationFrame(frame);
    };
  });
</script>

<div class="table">
  <div class="row head" class:covers class:editable={onremove}>
    <span>#</span><span>Titel</span><span>Album</span><span class="right">Dauer</span>
    {#if onremove}<span></span>{/if}
  </div>
  <div bind:this={rowsElement} class="rows">
  {#if virtual}<div style:height="{offset * rowHeight}px"></div>{/if}
  {#each visible as track, j (track.id + ":" + (offset + j))}
    {@const i = offset + j}
    {@const active = currentTrack()?.id === track.id}
    <div
      class="row"
      class:covers
      class:active
      class:editable={onremove}
      class:drop-target={dragOver === i && dragFrom !== i}
      draggable={Boolean(onmove)}
      ondragstart={() => (dragFrom = i)}
      ondragover={(e) => {
        if (dragFrom === null) return;
        e.preventDefault();
        dragOver = i;
      }}
      ondrop={() => drop(i)}
      ondragend={() => (dragFrom = dragOver = null)}
      role="button"
      tabindex="-1"
      ondblclick={() => play(i)}
      onkeydown={(e) => e.key === "Enter" && play(i)}
      oncontextmenu={(e) => openTrackMenu(e, track)}
    >
      <span class="num">
        {#if active && player.playing}<Icon name="volume" size={16} />{:else}{i + 1}{/if}
      </span>
      <span class="title">
        {#if covers}<Cover {track} size={40} />{/if}
        <span class="text">
          <strong>{track.title}</strong>
          <small>
            {#if track.source !== "local" && isOffline(track.id)}
              <span class="saved" title="Offline gespeichert"><Icon name="downloaded" size={13} /></span>
            {:else if track.source !== "local" && offline.pending[track.id]}
              <span class="saving" title="Wird gespeichert"><Icon name="download" size={13} /></span>
            {/if}
            <ArtistLinks {track} />
          </small>
        </span>
      </span>
      <span class="dim ellipsis">
        {#if track.albumId}
          <button
            class="link"
            onclick={(e) => {
              e.stopPropagation();
              openAlbum(track);
            }}
            ondblclick={(e) => e.stopPropagation()}>{track.album}</button
          >
        {:else}
          {track.album ?? ""}
        {/if}
      </span>
      <span class="dim right">{track.duration ? formatTime(track.duration) : ""}</span>
      {#if onremove}
        <button
          class="remove"
          title="Aus der Playlist entfernen"
          onclick={(e) => {
            e.stopPropagation();
            onremove(i);
          }}
          ondblclick={(e) => e.stopPropagation()}><Icon name="close" size={16} /></button
        >
      {/if}
    </div>
  {/each}
  {#if virtual}<div style:height="{Math.max(0, tracks.length - range.end) * rowHeight}px"></div>{/if}
  </div>
</div>

<style>
  .table,
  .rows {
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
  .row.editable {
    grid-template-columns: 40px minmax(0, 2fr) minmax(0, 1fr) 70px 32px;
  }
  .row.drop-target {
    box-shadow: inset 0 2px 0 var(--accent);
  }
  .row[draggable="true"] {
    cursor: grab;
  }
  .remove {
    width: 28px;
    height: 28px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--text-dim);
    opacity: 0;
  }
  .row:hover .remove {
    opacity: 1;
  }
  .remove:hover {
    color: var(--text);
    background: var(--hover);
  }
  .row.covers:not(.head) {
    padding: 6px 12px;
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
    display: grid;
    place-items: center;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 12px;
    min-width: 0;
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .text strong,
  .text small,
  .ellipsis {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .text strong {
    font-weight: 500;
  }
  .text small {
    color: var(--text-dim);
    font-size: 13px;
  }
  .saved,
  .saving {
    display: inline-flex;
    vertical-align: -2px;
    margin-right: 2px;
    color: var(--accent);
  }
  .saving {
    color: var(--text-dim);
    animation: pulse 1.2s ease-in-out infinite;
  }
  @keyframes pulse {
    50% {
      opacity: 0.3;
    }
  }
  .dim {
    color: var(--text-dim);
  }
  .link {
    color: inherit;
    font: inherit;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .link:hover {
    color: var(--text);
    text-decoration: underline;
  }
  .right {
    text-align: right;
  }
</style>
