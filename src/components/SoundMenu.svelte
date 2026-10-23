<script>
  import Popover from "./Popover.svelte";
  import { settings } from "../lib/settings.svelte.js";
  import { EQ_PRESETS, selectEqPreset, setNormalize, setTempo, setPreservePitch } from "../lib/sound.svelte.js";
  import { currentTrack } from "../lib/player.svelte.js";
  import { spotifyOut } from "../lib/spotifyPlayer.svelte.js";
  import { ui } from "../lib/ui.svelte.js";

  const TEMPOS = [0.75, 1, 1.25, 1.5];
  const formatRate = (r) => `${r.toLocaleString("de-DE", { maximumFractionDigits: 2 })}×`;

  const active = $derived(settings.eqEnabled || settings.normalize || settings.playbackRate !== 1);
  const onSpotify = $derived(currentTrack()?.source === "spotify" && spotifyOut.premium === true);

  function openSettings(close) {
    close();
    ui.view = "settings";
    setTimeout(() => document.getElementById("settings-sound")?.scrollIntoView({ behavior: "smooth" }), 50);
  }
</script>

<Popover
  icon="equalizer"
  title="Klang: Equalizer, Lautstärke angleichen, Tempo"
  {active}
  badge={settings.playbackRate !== 1 ? formatRate(settings.playbackRate) : null}
  width={300}
>
  {#snippet children({ close })}
    <h4>Tempo</h4>
    <div class="block">
      <div class="tempo">
        <input
          type="range"
          min="0.5"
          max="2"
          step="0.05"
          value={settings.playbackRate}
          style:--progress="{((settings.playbackRate - 0.5) / 1.5) * 100}%"
          oninput={(e) => setTempo(+e.currentTarget.value)}
        />
        <strong>{formatRate(settings.playbackRate)}</strong>
      </div>
      <div class="chips">
        {#each TEMPOS as rate (rate)}
          <button class:on={settings.playbackRate === rate} onclick={() => setTempo(rate)}>{formatRate(rate)}</button>
        {/each}
      </div>
      <label class="check">
        <input type="checkbox" checked={settings.preservePitch} onchange={(e) => setPreservePitch(e.currentTarget.checked)} />
        Tonhöhe beibehalten
        <small>{settings.preservePitch ? "" : "(schneller = höher, wie „Sped up“)"}</small>
      </label>
    </div>

    <hr />
    <h4>Equalizer</h4>
    <div class="block">
      <select value={settings.eqEnabled ? settings.eqPreset : "flat"} onchange={(e) => selectEqPreset(e.currentTarget.value)}>
        {#each EQ_PRESETS as preset (preset.id)}
          <option value={preset.id}>{preset.name}</option>
        {/each}
        {#if settings.eqPreset === "custom"}<option value="custom" disabled>Eigene Einstellung</option>{/if}
      </select>
      <button class="link" onclick={() => openSettings(close)}>Bänder einzeln einstellen…</button>
    </div>

    <hr />
    <label class="item switch-row">
      <span>
        Lautstärke angleichen
        <small>Alle Songs klingen etwa gleich laut</small>
      </span>
      <input type="checkbox" checked={settings.normalize} onchange={(e) => setNormalize(e.currentTarget.checked)} />
    </label>

    {#if onSpotify}
      <p class="hint warn">Spotify-Songs laufen gerade direkt über Spotify Premium – dort wirken Equalizer, Angleichen und Tempo nicht.</p>
    {/if}
  {/snippet}
</Popover>

<style>
  .block {
    padding: 0 10px 6px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .tempo {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .tempo input {
    flex: 1;
  }
  .tempo strong {
    width: 48px;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .chips {
    display: flex;
    gap: 6px;
  }
  .chips button {
    flex: 1;
    height: 28px;
    border-radius: 14px;
    border: 1px solid var(--border);
    font-size: 13px;
    color: var(--text-dim);
  }
  .chips button:hover {
    color: var(--text);
  }
  .chips button.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .check {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    flex-wrap: wrap;
  }
  .check small {
    color: var(--text-dim);
    font-size: 12px;
  }
  .check input,
  .switch-row input {
    accent-color: var(--accent);
    width: 16px;
    height: 16px;
  }
  .switch-row {
    justify-content: space-between;
    cursor: pointer;
  }
  select {
    height: 36px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    outline: none;
  }
  .link {
    align-self: flex-start;
    color: var(--accent);
    font-size: 13px;
  }
  .link:hover {
    text-decoration: underline;
  }
</style>
