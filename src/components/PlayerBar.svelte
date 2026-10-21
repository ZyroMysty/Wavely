<script>
  import {
    player,
    currentTrack,
    togglePlay,
    next,
    previous,
    seek,
    setVolume,
    toggleMute,
    toggleShuffle,
    cycleRepeat,
  } from "../lib/player.svelte.js";
  import { formatTime } from "../lib/format.js";
  import { ui, togglePanel, showToast } from "../lib/ui.svelte.js";
  import { invoke } from "@tauri-apps/api/core";
  import { settings } from "../lib/settings.svelte.js";
  import { ytm, isLiked, toggleLike } from "../lib/ytm.svelte.js";
  import Cover from "./Cover.svelte";
  import ArtistLinks from "./ArtistLinks.svelte";
  import Icon from "./Icon.svelte";

  const track = $derived(currentTrack());
  const canLike = $derived(track?.source === "youtube" && ytm.status === "loggedIn");

  async function like() {
    try {
      await toggleLike(track);
    } catch (e) {
      showToast(`Like fehlgeschlagen: ${e}`, true);
    }
  }

  // Während des Ziehens nicht vom laufenden Song überschreiben lassen
  let dragging = $state(false);
  let dragValue = $state(0);
  const shownPosition = $derived(dragging ? dragValue : player.position);
  const progress = $derived(player.duration ? (shownPosition / player.duration) * 100 : 0);
  const volume = $derived(player.muted ? 0 : player.volume);
</script>

<footer>
  <div class="now">
    {#if track}
      <Cover {track} size={56} />
      <div class="meta">
        <strong>{track.title}</strong>
        {#if player.error}
          <small class="error" title={player.error}>{player.error}</small>
        {:else if player.loading}
          <small>Lädt…</small>
        {:else}
          <small><ArtistLinks {track} /></small>
        {/if}
      </div>
      {#if canLike}
        <button
          class="small like"
          class:on={isLiked(track.id)}
          title={isLiked(track.id) ? "Gefällt mir nicht mehr" : "Gefällt mir"}
          onclick={like}
        >
          <Icon name={isLiked(track.id) ? "heart" : "heartOutline"} size={20} />
        </button>
      {/if}
    {/if}
  </div>

  <div class="center">
    <div class="controls">
      <button class="small" class:on={player.shuffle} title="Zufallswiedergabe" onclick={toggleShuffle}>
        <Icon name="shuffle" size={18} />
      </button>
      <button class="small" title="Zurück" onclick={previous} disabled={!track}>
        <Icon name="previous" size={22} />
      </button>
      <button class="play" title={player.playing ? "Pause" : "Abspielen"} onclick={togglePlay} disabled={!track}>
        <Icon name={player.playing ? "pause" : "play"} size={22} />
      </button>
      <button class="small" title="Weiter" onclick={next} disabled={!track}>
        <Icon name="next" size={22} />
      </button>
      <button
        class="small"
        class:on={player.repeat !== "off"}
        title={{ off: "Wiederholen: aus", all: "Alle wiederholen", one: "Titel wiederholen" }[player.repeat]}
        onclick={cycleRepeat}
      >
        <Icon name={player.repeat === "one" ? "repeatOne" : "repeat"} size={18} />
      </button>
    </div>
    <div class="seek">
      <span>{formatTime(shownPosition)}</span>
      <input
        type="range"
        min="0"
        max={player.duration || 0}
        step="0.1"
        value={shownPosition}
        style:--progress="{progress}%"
        disabled={!track}
        oninput={(e) => {
          dragging = true;
          dragValue = +e.currentTarget.value;
        }}
        onchange={(e) => {
          seek(+e.currentTarget.value);
          dragging = false;
        }}
      />
      <span>{formatTime(player.duration)}</span>
    </div>
  </div>

  <div class="right">
    <button class="small" title="Mini-Player" onclick={() => invoke("mini_player_open", { pinned: settings.miniPinned, width: settings.miniWidth, height: settings.miniHeight })}>
      <Icon name="miniPlayer" size={20} />
    </button>
    <button class="small" class:on={ui.panel === "lyrics"} title="Liedtext" onclick={() => togglePanel("lyrics")}>
      <Icon name="lyrics" size={20} />
    </button>
    <button class="small" class:on={ui.panel === "history"} title="Verlauf" onclick={() => togglePanel("history")}>
      <Icon name="history" size={20} />
    </button>
    <button class="small" class:on={ui.panel === "queue"} title="Warteschlange" onclick={() => togglePanel("queue")}>
      <Icon name="queue" size={20} />
    </button>
    <button class="small" title={player.muted ? "Ton an" : "Stumm"} onclick={toggleMute}>
      <Icon name={volume === 0 ? "mute" : "volume"} size={20} />
    </button>
    <input
      class="volume"
      type="range"
      min="0"
      max="1"
      step="0.01"
      value={volume}
      style:--progress="{volume * 100}%"
      oninput={(e) => setVolume(+e.currentTarget.value)}
    />
  </div>
</footer>

<style>
  footer {
    grid-area: player;
    height: var(--player-height);
    background: var(--surface);
    border-top: 1px solid var(--border);
    display: grid;
    grid-template-columns: 1fr minmax(320px, 2fr) 1fr;
    align-items: center;
    gap: 16px;
    padding: 0 20px;
  }
  .now {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
  }
  .meta {
    display: flex;
    flex-direction: column;
    min-width: 0;
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
    font-size: 13px;
  }
  .meta small.error {
    color: var(--accent);
  }
  .center {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
  }
  .controls {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .small {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--text-dim);
  }
  .small:hover:not(:disabled) {
    color: var(--text);
  }
  .small.on {
    color: var(--accent);
  }
  .like {
    flex-shrink: 0;
  }
  .play {
    width: 40px;
    height: 40px;
    border-radius: 50%;
    background: var(--text);
    color: var(--bg);
    display: grid;
    place-items: center;
  }
  .play:hover:not(:disabled) {
    transform: scale(1.06);
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .seek {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
    color: var(--text-dim);
    font-variant-numeric: tabular-nums;
  }
  .seek span {
    width: 40px;
    text-align: center;
  }
  .right {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 8px;
  }
  .volume {
    width: 110px;
  }
</style>
