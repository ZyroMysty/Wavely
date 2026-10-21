<script>
  import { openArtist } from "../lib/ui.svelte.js";

  /** Künstlernamen eines Songs; YouTube-Künstler mit ID sind anklickbar */
  let { track } = $props();

  const artists = $derived(
    track.artists?.length ? track.artists : [{ id: null, name: track.artist ?? "Unbekannter Künstler" }],
  );
</script>

{#each artists as artist, i (i)}{#if i > 0}, {/if}{#if artist.id}<button
      class="link"
      onclick={(e) => {
        e.stopPropagation();
        openArtist(artist);
      }}
      ondblclick={(e) => e.stopPropagation()}>{artist.name}</button
    >{:else}{artist.name}{/if}{/each}

<style>
  .link {
    color: inherit;
    font: inherit;
  }
  .link:hover {
    color: var(--text);
    text-decoration: underline;
  }
</style>
