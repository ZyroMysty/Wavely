<script>
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { emit, listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { settings, updateSetting, applyAppearance } from "../lib/settings.svelte.js";
  import { isLetterboxed } from "../lib/artwork.js";
  import { formatTime } from "../lib/format.js";
  import Icon from "./Icon.svelte";
  import Logo from "./Logo.svelte";

  /** Zustand kommt vom Hauptfenster (der Mini-Player spielt selbst nichts ab) */
  let state = $state({
    key: null,
    title: null,
    artist: null,
    playing: false,
    loading: false,
    position: 0,
    duration: 0,
    canLike: false,
    liked: false,
    volume: 0.6,
    muted: false,
  });
  let cover = $state(null);
  const win = getCurrentWindow();

  // Fenstergröße bestimmt das Layout
  let width = $state(window.innerWidth);
  let height = $state(window.innerHeight);
  /** Groß: senkrechter Player mit großem Cover; sonst kompakte Leiste */
  const large = $derived(height >= 220 && width >= 200);
  const coverSize = $derived(
    large
      ? Math.max(80, Math.min(width - 32, height - 210))
      : Math.max(40, Math.min(height - 24, 120, width * 0.32)),
  );
  // Kompakt: je nach freier Breite neben dem Cover Knöpfe ausblenden (Regler → Lautstärke → Herz)
  const bodyWidth = $derived(width - coverSize - 36);
  const showLike = $derived(large || bodyWidth >= 150);
  const showVolume = $derived(large || bodyWidth >= 190);
  const showVolumeSlider = $derived(large || bodyWidth >= 256);

  const progress = $derived(state.duration ? Math.min(100, (state.position / state.duration) * 100) : 0);
  const command = (action) => emit("player-command", action);
  const volume = $derived(state.muted ? 0 : state.volume);

  // Ziehen am Fortschrittsbalken nicht vom laufenden Song überschreiben lassen
  let seeking = $state(false);
  let seekValue = $state(0);

  onMount(() => {
    applyAppearance();

    const unlisten = [
      listen("player-state", (e) => (state = e.payload)),
      listen("player-cover", (e) => (cover = e.payload.src)),
    ];
    Promise.all(unlisten).then(() => emit("mini-ready"));

    return () => unlisten.forEach((p) => p.then((fn) => fn()));
  });

  // Größe merken, damit der Mini-Player beim nächsten Öffnen gleich groß ist
  let saveTimer;
  function onResize() {
    width = window.innerWidth;
    height = window.innerHeight;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => {
      updateSetting("miniWidth", width);
      updateSetting("miniHeight", height);
    }, 400);
  }

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

<svelte:window onresize={onResize} />

<div class="mini" class:large data-tauri-drag-region>
  <div class="window-buttons">
    <button class:on={settings.miniPinned} title={settings.miniPinned ? "Nicht mehr im Vordergrund" : "Immer im Vordergrund"} onclick={togglePin}>
      <Icon name="pin" size={15} />
    </button>
    <button title="Großes Fenster öffnen" onclick={expand}><Icon name="expandWindow" size={15} /></button>
    <button title="Schließen" onclick={() => win.close()}><Icon name="close" size={15} /></button>
  </div>

  <div class="cover" style:width="{coverSize}px" style:height="{coverSize}px" data-tauri-drag-region>
    {#if cover}
      <img src={cover} alt="" class:letterboxed={isLetterboxed(cover)} data-tauri-drag-region />
    {:else}
      <Logo size={coverSize} />
    {/if}
  </div>

  <div class="body" data-tauri-drag-region>
    <div class="meta" data-tauri-drag-region>
      <strong data-tauri-drag-region>{state.title ?? "Nichts läuft"}</strong>
      <small data-tauri-drag-region>{state.loading ? "Lädt…" : (state.artist ?? "Wavely")}</small>
    </div>

    {#if large}
      <div class="seek">
        <span>{formatTime(seeking ? seekValue : state.position)}</span>
        <input
          type="range"
          min="0"
          max={state.duration || 0}
          step="0.1"
          value={seeking ? seekValue : state.position}
          style:--progress="{progress}%"
          disabled={!state.key}
          aria-label="Position im Song"
          oninput={(e) => {
            seeking = true;
            seekValue = +e.currentTarget.value;
          }}
          onchange={(e) => {
            command({ action: "seek", position: +e.currentTarget.value });
            seeking = false;
          }}
        />
        <span>{formatTime(state.duration)}</span>
      </div>
    {/if}

    <div class="controls">
      {#if large && showLike && state.canLike}
        <button class:on={state.liked} title={state.liked ? "Gefällt mir nicht mehr" : "Gefällt mir"} onclick={() => command("like")}>
          <Icon name={state.liked ? "heart" : "heartOutline"} size={18} />
        </button>
      {/if}
      <button title="Zurück" disabled={!state.key} onclick={() => command("previous")}>
        <Icon name="previous" size={large ? 24 : 20} />
      </button>
      <button class="play" title={state.playing ? "Pause" : "Abspielen"} disabled={!state.key} onclick={() => command("toggle")}>
        <Icon name={state.playing ? "pause" : "play"} size={large ? 26 : 20} />
      </button>
      <button title="Weiter" disabled={!state.key} onclick={() => command("next")}>
        <Icon name="next" size={large ? 24 : 20} />
      </button>
      {#if !large && showLike && state.canLike}
        <button class:on={state.liked} title={state.liked ? "Gefällt mir nicht mehr" : "Gefällt mir"} onclick={() => command("like")}>
          <Icon name={state.liked ? "heart" : "heartOutline"} size={18} />
        </button>
      {/if}
      {#if !large && showVolume}
        <div class="volume">
          <button title={state.muted ? "Ton an" : "Stumm"} onclick={() => command("mute")}>
            <Icon name={volume === 0 ? "mute" : "volume"} size={17} />
          </button>
          {#if showVolumeSlider}
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
          {/if}
        </div>
      {/if}
    </div>

    {#if large}
      <div class="volume wide">
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
    {/if}
  </div>

  {#if !large}
    <div class="progress" style:width="{progress}%"></div>
  {/if}
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
  .meta {
    min-width: 0;
    display: flex;
    flex-direction: column;
    /* Platz für die Fensterknöpfe oben rechts */
    padding-right: 78px;
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
    position: absolute;
    top: 6px;
    right: 6px;
    z-index: 2;
    display: flex;
    gap: 2px;
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
  .controls button,
  .volume button {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--text-dim);
    flex-shrink: 0;
  }
  .controls button:hover:not(:disabled),
  .volume button:hover {
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
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .volume {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 2px;
    min-width: 0;
  }
  .volume input {
    width: 64px;
  }
  .progress {
    position: absolute;
    left: 0;
    bottom: 0;
    height: 3px;
    background: var(--accent);
    transition: width 0.25s linear;
  }

  /* --- Großes Layout: Cover oben, Steuerung darunter --- */
  .mini.large {
    flex-direction: column;
    justify-content: center;
    gap: 14px;
    padding: 36px 16px 16px;
  }
  .large .cover {
    border-radius: 12px;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.35);
  }
  .large .body {
    flex: 0 0 auto;
    width: 100%;
    gap: 10px;
  }
  .large .meta {
    align-items: center;
    text-align: center;
    padding-right: 0;
  }
  .large .meta strong {
    font-size: 16px;
    max-width: 100%;
  }
  .large .meta small {
    font-size: 13px;
    max-width: 100%;
  }
  .seek {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .seek span {
    width: 34px;
    text-align: center;
    flex-shrink: 0;
  }
  .large .controls {
    justify-content: center;
    gap: 14px;
  }
  .large .controls .play {
    width: 46px;
    height: 46px;
  }
  .volume.wide {
    margin: 0 auto;
    width: min(220px, 100%);
  }
  .volume.wide input {
    flex: 1;
    width: auto;
  }
</style>
