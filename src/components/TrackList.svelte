<script>
  import { player, playList, currentTrack } from "../lib/player.svelte.js";
  import { formatTime } from "../lib/format.js";
  import Cover from "./Cover.svelte";
  import Icon from "./Icon.svelte";

  /** covers: kleine Cover in jeder Zeile anzeigen (für YouTube Music) */
  let { tracks, covers = false } = $props();
</script>

<div class="table">
  <div class="row head" class:covers>
    <span>#</span><span>Titel</span><span>Album</span><span class="right">Dauer</span>
  </div>
  {#each tracks as track, i (track.id + ":" + i)}
    {@const active = currentTrack()?.id === track.id}
    <div
      class="row"
      class:covers
      class:active
      role="button"
      tabindex="-1"
      ondblclick={() => playList(tracks, i)}
      onkeydown={(e) => e.key === "Enter" && playList(tracks, i)}
    >
      <span class="num">
        {#if active && player.playing}<Icon name="volume" size={16} />{:else}{i + 1}{/if}
      </span>
      <span class="title">
        {#if covers}<Cover {track} size={40} />{/if}
        <span class="text">
          <strong>{track.title}</strong>
          <small>{track.artist ?? "Unbekannter Künstler"}</small>
        </span>
      </span>
      <span class="dim ellipsis">{track.album ?? ""}</span>
      <span class="dim right">{track.duration ? formatTime(track.duration) : ""}</span>
    </div>
  {/each}
</div>

<style>
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
  .dim {
    color: var(--text-dim);
  }
  .right {
    text-align: right;
  }
</style>
