<script>
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { emit, listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { settings, updateSetting, applyAppearance } from "../lib/settings.svelte.js";
  import { isLetterboxed } from "../lib/artwork.js";
  import Icon from "./Icon.svelte";
  import Logo from "./Logo.svelte";

  /** Zustand kommt vom Hauptfenster (der Mini-Player spielt selbst nichts ab) */
  let state = $state({ key: null, title: null, artist: null, playing: false, loading: false, position: 0, duration: 0, canLike: false, liked: false, volume: 0.6, muted: false });
  let cover = $state(null);
  const win = getCurrentWindow();

  const progress = $derived(state.duration ? Math.min(100, (state.position / state.duration) * 100) : 0);
  const command = (action) => emit("player-command", action);
  const volume = $derived(state.muted ? 0 : state.volume);

  onMount(() => {
    applyAppearance();

    const unlisten = [
      listen("player-state", (e) => (state = e.payload)),
      listen("player-cover", (e) => (cover = e.payload.src)),
    ];
    Promise.all(unlisten).then(() => emit("mini-ready"));

    return () => {
      unlisten.forEach((p) => p.then((fn) => fn()));
    };
  });

  async function togglePin() {
    const pinned = !settings.miniPinned;
    updateSetting("miniPinned", pinned);
    await win.setAlwaysOnTop(pinned);
  }

  async function expand() {
    await invoke("show_main_window");
    await win.close();
  }
</script>

<div class="mini" data-tauri-drag-region>
  <div class="cover" data-tauri-drag-region>
    {#if cover}
      <img src={cover} alt="" class:letterboxed={isLetterboxed(cover)} data-tauri-drag-region />
    {:else}
      <Logo size={72} />
    {/if}
  </div>

  <div class="body" data-tauri-drag-region>
    <div class="top" data-tauri-drag-region>
      <div class="meta" data-tauri-drag-region>
        <strong data-tauri-drag-region>{state.title ?? "Nichts läuft"}</strong>
        <small data-tauri-drag-region>{state.loading ? "Lädt…" : (state.artist ?? "Wavely")}</small>
      </div>
      <div class="window-buttons">
        <button class:on={settings.miniPinned} title={settings.miniPinned ? "Nicht mehr im Vordergrund" : "Immer im Vordergrund"} onclick={togglePin}>
          <Icon name="pin" size={15} />
        </button>
        <button title="Großes Fenster öffnen" onclick={expand}><Icon name="expandWindow" size={15} /></button>
        <button title="Schließen" onclick={() => win.close()}><Icon name="close" size={15} /></button>
      </div>
    </div>

    <div class="controls">
      <button title="Zurück" disabled={!state.key} onclick={() => command("previous")}><Icon name="previous" size={20} /></button>
      <button class="play" title={state.playing ? "Pause" : "Abspielen"} disabled={!state.key} onclick={() => command("toggle")}>
        <Icon name={state.playing ? "pause" : "play"} size={20} />
      </button>
      <button title="Weiter" disabled={!state.key} onclick={() => command("next")}><Icon name="next" size={20} /></button>
      {#if state.canLike}
        <button class:on={state.liked} title={state.liked ? "Gefällt mir nicht mehr" : "Gefällt mir"} onclick={() => command("like")}>
          <Icon name={state.liked ? "heart" : "heartOutline"} size={18} />
        </button>
      {/if}
      <div class="volume">
        <button title={state.muted ? "Ton an" : "Stumm"} onclick={() => command("mute")}>
          <Icon name={volume === 0 ? "mute" : "volume"} size={17} />
        </button>
        <input
          type="range"
          min="0"
          max="1"
          step="0.01"
          value={volume}
          style:--progress="{volume * 100}%"
          aria-label="Lautstärke"
          oninput={(e) => command({ action: "volume", value: +e.currentTarget.value })}
        />
      </div>
    </div>
  </div>

  <div class="progress" style:width="{progress}%"></div>
</div>

<style>
  :global(html, body, #app) {
    background: var(--surface);
  }
  .mini {
    position: relative;
    height: 100vh;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 0 10px 0 12px;
    background: var(--surface);
    overflow: hidden;
    border: 1px solid var(--border);
  }
  .cover {
    width: 72px;
    height: 72px;
    flex-shrink: 0;
    border-radius: 8px;
    overflow: hidden;
    background: var(--surface-2);
  }
  .cover img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .cover img.letterboxed {
    transform: scale(1.34);
  }
  .body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .top {
    display: flex;
    align-items: flex-start;
    gap: 6px;
  }
  .meta {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .meta strong,
  .meta small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta strong {
    font-weight: 600;
  }
  .meta small {
    color: var(--text-dim);
    font-size: 12px;
  }
  .window-buttons {
    display: flex;
    gap: 2px;
    margin-top: -2px;
  }
  .window-buttons button {
    width: 24px;
    height: 24px;
    border-radius: 6px;
    display: grid;
    place-items: center;
    color: var(--text-dim);
  }
  .window-buttons button:hover {
    background: var(--hover);
    color: var(--text);
  }
  .controls {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .controls button {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--text-dim);
  }
  .controls button:hover:not(:disabled) {
    color: var(--text);
    background: var(--hover);
  }
  .controls .play {
    background: var(--text);
    color: var(--bg);
  }
  .controls .play:hover:not(:disabled) {
    background: var(--text);
    color: var(--bg);
    transform: scale(1.06);
  }
  button.on {
    color: var(--accent);
  }
  .volume {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .volume button {
    width: 28px;
    height: 28px;
  }
  .volume input {
    width: 64px;
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .progress {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 3px;
    background: var(--accent);
    transition: width 0.25s linear;
  }
</style>
