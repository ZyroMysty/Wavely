<script>
  import { onMount } from "svelte";
  import { refreshLibrary } from "./lib/library.svelte.js";
  import { togglePlay } from "./lib/player.svelte.js";
  import { initYtm } from "./lib/ytm.svelte.js";
  import { initOffline } from "./lib/offline.svelte.js";
  import { initBridge } from "./lib/bridge.svelte.js";
  import OfflineView from "./components/OfflineView.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import LocalView from "./components/LocalView.svelte";
  import YouTubeView from "./components/YouTubeView.svelte";
  import PlayerBar from "./components/PlayerBar.svelte";
  import SidePanel from "./components/SidePanel.svelte";
  import TrackMenu from "./components/TrackMenu.svelte";
  import NewPlaylistDialog from "./components/NewPlaylistDialog.svelte";
  import Toast from "./components/Toast.svelte";
  import SettingsView from "./components/SettingsView.svelte";
  import { ui } from "./lib/ui.svelte.js";
  import { applyAppearance, settings } from "./lib/settings.svelte.js";

  // Design und Akzentfarbe live übernehmen
  $effect(() => {
    settings.theme;
    settings.accent;
    applyAppearance();
  });

  onMount(() => {
    refreshLibrary();
    initYtm();
    initOffline();
    initBridge();
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

<div class="app" class:with-panel={ui.panel} class:collapsed={ui.sidebarCollapsed}>
  <Sidebar />
  <main>
    {#if ui.view === "local"}
      <LocalView />
    {:else if ui.view === "offline"}
      <OfflineView />
    {:else if ui.view === "settings"}
      <SettingsView />
    {:else}
      <YouTubeView />
    {/if}
  </main>
  {#if ui.panel}<SidePanel />{/if}
  <PlayerBar />
</div>
<TrackMenu />
<NewPlaylistDialog />
<Toast />

<style>
  .app {
    height: 100%;
    display: grid;
    grid-template-columns: var(--sidebar-width) minmax(0, 1fr);
    grid-template-rows: minmax(0, 1fr) auto;
    grid-template-areas:
      "sidebar main"
      "player player";
  }
  .app.with-panel {
    grid-template-columns: var(--sidebar-width) minmax(0, 1fr) auto;
    grid-template-areas:
      "sidebar main panel"
      "player player player";
  }
  .app.collapsed {
    --sidebar-width: 64px;
  }
  main {
    grid-area: main;
    overflow-y: auto;
  }
</style>
