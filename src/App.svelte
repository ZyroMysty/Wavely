<script>
  import { onMount } from "svelte";
  import { initModules } from "./lib/modules.svelte.js";
  import PlaylistView from "./components/PlaylistView.svelte";
  import RecapView from "./components/RecapView.svelte";
  import { initPlaylists } from "./lib/playlists.svelte.js";
  import { initStats } from "./lib/stats.svelte.js";
  import { initOffline } from "./lib/offline.svelte.js";
  import { spotify } from "./lib/spotify.svelte.js";
  import { initSpotifyPlayer } from "./lib/spotifyPlayer.svelte.js";
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
  import TransferDialog from "./components/TransferDialog.svelte";
  import { showWhatsNew } from "./lib/changelog.svelte.js";
  import { initSound } from "./lib/sound.svelte.js";
  import HomeView from "./components/HomeView.svelte";
  import ShortcutsDialog from "./components/ShortcutsDialog.svelte";
  import { handleKey, applyGlobalShortcuts } from "./lib/shortcuts.svelte.js";
  import SettingsView from "./components/SettingsView.svelte";
  import { ui, showToast } from "./lib/ui.svelte.js";
  import { account, initAccount } from "./lib/account.svelte.js";
  import FeedbackDialog from "./components/FeedbackDialog.svelte";
  import DiagnosticsDialog from "./components/DiagnosticsDialog.svelte";
  import { joinRoom, codeFromLink } from "./lib/together.svelte.js";
  import { onOpenUrl, getCurrent as getCurrentLinks } from "@tauri-apps/plugin-deep-link";
  import { getCurrentWindow } from "@tauri-apps/api/window";
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
    // Lokale Musik, YouTube Music, Spotify, SoundCloud, Last.fm – nur was eingeschaltet ist
    initModules();
    initPlaylists();
    initStats();
    initOffline();
    initBridge();
    initUpdater();
    initSound();
    applyGlobalShortcuts();
    showWhatsNew();
    initAccount();
    initInviteLinks();
  });

  // Einladungslinks „wavely://join/CODE“: beim Start mitgegeben oder später (zweiter Aufruf)
  let pendingInvite = $state(null);
  async function handleLinks(urls) {
    const code = (urls ?? []).map(codeFromLink).find(Boolean);
    if (!code) return;
    pendingInvite = code;
    getCurrentWindow().setFocus().catch(() => {});
  }
  async function initInviteLinks() {
    handleLinks(await getCurrentLinks().catch(() => null));
    onOpenUrl(handleLinks).catch(() => {});
  }
  $effect(() => {
    if (!pendingInvite || account.status === "checking") return;
    const code = pendingInvite;
    pendingInvite = null;
    if (account.status !== "loggedIn") {
      showToast("Zum Mithören bitte zuerst im Wavely-Konto anmelden");
      ui.view = "settings";
      setTimeout(() => document.getElementById("settings-account")?.scrollIntoView(), 50);
      return;
    }
    joinRoom(code).catch((e) => showToast(String(e?.message ?? e), true));
  });

</script>

<svelte:window onkeydown={handleKey} />

<div class="app" class:with-panel={ui.panel} class:collapsed={ui.sidebarCollapsed}>
  <Sidebar />
  <main>
    {#if ui.view === "home"}
      <HomeView />
    {:else if ui.view === "playlist"}
      <PlaylistView />
    {:else if ui.view === "recap"}
      <RecapView />
    {:else if ui.view === "local"}
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
<TransferDialog />
<ShortcutsDialog />
<FeedbackDialog />
{#if account.admin}<DiagnosticsDialog />{/if}

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
