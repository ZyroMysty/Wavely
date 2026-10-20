<script>
  import { artworkSrc, isLetterboxed } from "../lib/artwork.js";
  import Icon from "./Icon.svelte";

  let { item, onopen } = $props();
</script>

<button class="card" onclick={() => onopen(item)}>
  <div class="art">
    {#if item.artworkUrl}
      <img src={artworkSrc(item.artworkUrl)} alt="" loading="lazy" class:letterboxed={isLetterboxed(item.artworkUrl)} />
    {:else}
      <Icon name="music" size={40} />
    {/if}
  </div>
  <strong>{item.title}</strong>
  {#if item.subtitle}<small>{item.subtitle}</small>{/if}
</button>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px;
    border-radius: var(--radius);
    text-align: left;
    min-width: 0;
  }
  .card:hover {
    background: var(--surface-2);
  }
  .art {
    aspect-ratio: 1;
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text-dim);
    display: grid;
    place-items: center;
    overflow: hidden;
    margin-bottom: 6px;
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  img.letterboxed {
    transform: scale(1.34);
  }
  strong,
  small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  strong {
    font-weight: 600;
  }
  small {
    color: var(--text-dim);
    font-size: 13px;
  }
</style>
