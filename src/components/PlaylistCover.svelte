<script>
  import { coversOf } from "../lib/playlists.svelte.js";
  import Cover from "./Cover.svelte";
  import Icon from "./Icon.svelte";

  /** Cover einer Wavely-Playlist: ein Bild oder ein Mosaik aus vier */
  let { playlist, size = 200 } = $props();
  const covers = $derived(coversOf(playlist));
</script>

<div class="mosaic" style:width="{size}px" style:height="{size}px">
  {#if covers.length >= 4}
    {#each covers as track (track.artworkUrl)}
      <Cover {track} size={size / 2} />
    {/each}
  {:else if covers.length}
    <Cover track={covers[0]} {size} />
  {:else}
    <div class="empty"><Icon name="queue" size={size * 0.35} /></div>
  {/if}
</div>

<style>
  .mosaic {
    flex-shrink: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    border-radius: 8px;
    overflow: hidden;
    background: var(--surface-2);
  }
  .mosaic :global(.cover) {
    border-radius: 0;
  }
  .mosaic > :global(.cover:only-child) {
    grid-column: span 2;
  }
  .empty {
    grid-column: span 2;
    display: grid;
    place-items: center;
    color: var(--text-dim);
    background: linear-gradient(135deg, var(--accent-soft), var(--surface-2));
  }
</style>
