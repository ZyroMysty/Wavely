<script>
  import { coverFor } from "../lib/covers.js";
  import Icon from "./Icon.svelte";

  let { track, size = 56 } = $props();
  let src = $state(null);

  $effect(() => {
    const t = track;
    src = null;
    coverFor(t).then((url) => {
      if (track === t) src = url;
    });
  });
</script>

<div class="cover" style:width="{size}px" style:height="{size}px">
  {#if src}
    <img {src} alt="" />
  {:else}
    <Icon name="music" size={size * 0.45} />
  {/if}
</div>

<style>
  .cover {
    flex-shrink: 0;
    border-radius: 6px;
    background: var(--surface-2);
    color: var(--text-dim);
    display: grid;
    place-items: center;
    overflow: hidden;
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
