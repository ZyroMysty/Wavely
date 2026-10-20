<script>
  import { onMount } from "svelte";
  import { refreshLibrary } from "./lib/library.svelte.js";
  import { togglePlay } from "./lib/player.svelte.js";
  import { initYtm } from "./lib/ytm.svelte.js";
  import Sidebar from "./components/Sidebar.svelte";
  import LocalView from "./components/LocalView.svelte";
  import YouTubeView from "./components/YouTubeView.svelte";
  import PlayerBar from "./components/PlayerBar.svelte";

  let view = $state("local");

  onMount(() => {
    refreshLibrary();
    initYtm();
  });

  // Leertaste = Play/Pause (außer beim Tippen)
  function onKeydown(e) {
    if (e.code === "Space" && !(e.target instanceof HTMLInputElement)) {
      e.preventDefault();
      togglePlay();
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="app">
  <Sidebar bind:view />
  <main>
    {#if view === "local"}
      <LocalView />
    {:else}
      <YouTubeView />
    {/if}
  </main>
  <PlayerBar />
</div>

<style>
  .app {
    height: 100%;
    display: grid;
    grid-template-columns: var(--sidebar-width) 1fr;
    grid-template-rows: minmax(0, 1fr) auto;
    grid-template-areas:
      "sidebar main"
      "player player";
  }
  main {
    grid-area: main;
    overflow-y: auto;
  }
</style>
