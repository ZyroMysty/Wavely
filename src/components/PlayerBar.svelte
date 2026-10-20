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
  import Cover from "./Cover.svelte";
  import Icon from "./Icon.svelte";

  const track = $derived(currentTrack());

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
          <small>{track.artist ?? "Unbekannter Künstler"}</small>
        {/if}
      </div>
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
