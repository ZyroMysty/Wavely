<script>
  import { onMount } from "svelte";
  import { refreshLibrary } from "./lib/library.svelte.js";
  import { togglePlay } from "./lib/player.svelte.js";
  import { initYtm } from "./lib/ytm.svelte.js";
  import { initOffline } from "./lib/offline.svelte.js";
  import { initSpotify, spotify } from "./lib/spotify.svelte.js";
  import { initSpotifyPlayer } from "./lib/spotifyPlayer.svelte.js";
  import { initSoundCloud } from "./lib/soundcloud.svelte.js";
  import SoundCloudView from "./components/SoundCloudView.svelte";
  import SpotifyView from "./components/SpotifyView.svelte";
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
  import UpdateBanner from "./components/UpdateBanner.svelte";
  import { initUpdater } from "./lib/updater.svelte.js";
  import ChangelogDialog from "./components/ChangelogDialog.svelte";
  import { showWhatsNew } from "./lib/changelog.svelte.js";
  import { initSound } from "./lib/sound.svelte.js";
  import SettingsView from "./components/SettingsView.svelte";
  import { ui } from "./lib/ui.svelte.js";
  import { applyAppearance, settings } from "./lib/settings.svelte.js";

  // Spotify-Player (Premium) im Hintergrund vorbereiten, sobald das Konto bekannt ist
  $effect(() => {
    if (spotify.status === "loggedIn" && spotify.account) initSpotifyPlayer();
  });

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
    initSpotify();
    initSoundCloud();
    initBridge();
    initUpdater();
    initSound();
    showWhatsNew();
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
    {:else if ui.view === "spotify"}
      <SpotifyView />
    {:else if ui.view === "soundcloud"}
      <SoundCloudView />
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
<UpdateBanner />
<ChangelogDialog />

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
