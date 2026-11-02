<script>
  import { coverFor } from "../lib/covers.js";
  import { isLetterboxed } from "../lib/artwork.js";
  import Icon from "./Icon.svelte";

  let { track, size = 56 } = $props();
  let src = $state(null);
  let element = $state();
  // Erst laden, wenn das Cover in die Nähe des sichtbaren Bereichs kommt (lange Listen)
  let visible = $state(false);

  $effect(() => {
    if (visible || !element) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          visible = true;
          observer.disconnect();
        }
      },
      { rootMargin: "400px" },
    );
    observer.observe(element);
    return () => observer.disconnect();
  });

  $effect(() => {
    const t = track;
    src = null;
    if (!visible) return;
    coverFor(t, size).then((url) => {
      if (track === t) src = url;
    });
  });
</script>

<div class="cover" bind:this={element} style:width="{size}px" style:height="{size}px">
  {#if src}
    <img {src} alt="" decoding="async" class:letterboxed={isLetterboxed(src)} />
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
  img.letterboxed {
    transform: scale(1.34);
  }
</style>
