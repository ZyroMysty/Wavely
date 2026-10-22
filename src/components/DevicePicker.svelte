<script>
  import { spotifyOut, targetDevice, loadDevices, selectDevice, initSpotifyPlayer } from "../lib/spotifyPlayer.svelte.js";
  import { player, currentTrack } from "../lib/player.svelte.js";
  import Icon from "./Icon.svelte";

  let open = $state(false);
  let root = $state();
  let loading = $state(false);

  const target = $derived(targetDevice());
  const activeName = $derived(
    target === "wavely" ? null : (spotifyOut.devices.find((d) => d.id === target)?.name ?? "anderes Gerät"),
  );

  async function toggle() {
    open = !open;
    if (open) {
      loading = true;
      await initSpotifyPlayer();
      await loadDevices();
      loading = false;
    }
  }

  async function choose(id) {
    open = false;
    const playingSpotify = currentTrack()?.source === "spotify" && player.playing;
    await selectDevice(id, playingSpotify);
  }

  function close(event) {
    if (open && !root?.contains(event.target)) open = false;
  }

  const iconFor = (kind) => ({ Smartphone: "phone", Speaker: "speaker", TV: "tv" })[kind] ?? "devices";
</script>

<svelte:window onmousedown={close} onkeydown={(e) => e.key === "Escape" && (open = false)} />

<div class="picker" bind:this={root}>
  <button class="small" class:on={target !== "wavely"} title={activeName ? `Spotify läuft auf: ${activeName}` : "Spotify-Gerät wählen"} onclick={toggle}>
    <Icon name="devices" size={20} />
  </button>

  {#if open}
    <div class="menu" role="menu">
      <h4>Spotify abspielen auf</h4>
      <button role="menuitem" class:active={target === "wavely"} onclick={() => choose("wavely")}>
        <Icon name="devices" size={18} />
        <span>
          <strong>Wavely (dieser PC)</strong>
          <small>{spotifyOut.premium === false ? "Spotify Free: über YouTube Music" : "Direkt in Wavely"}</small>
        </span>
      </button>
      {#if loading}
        <p class="hint">Geräte werden gesucht…</p>
      {:else}
        {#each spotifyOut.devices as device (device.id)}
          <button role="menuitem" class:active={target === device.id} onclick={() => choose(device.id)}>
            <Icon name={iconFor(device.kind)} size={18} />
            <span>
              <strong>{device.name}</strong>
              <small>{device.isActive ? "Gerade aktiv" : device.kind}</small>
            </span>
          </button>
        {:else}
          <p class="hint">Keine weiteren Geräte. Öffne Spotify auf deinem Handy, Lautsprecher oder PC, dann erscheint es hier.</p>
        {/each}
      {/if}
      {#if spotifyOut.premium === false}
        <p class="hint">Andere Geräte lassen sich nur mit Spotify Premium steuern.</p>
      {/if}
      {#if spotifyOut.notice}<p class="hint warn">{spotifyOut.notice}</p>{/if}
    </div>
  {/if}
</div>

<style>
  .picker {
    position: relative;
  }
  .small {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    display: grid;
    place-items: center;
    color: var(--text-dim);
  }
  .small:hover {
    color: var(--text);
  }
  .small.on {
    color: var(--accent);
  }
  .menu {
    position: absolute;
    right: -40px;
    bottom: calc(100% + 12px);
    z-index: 60;
    width: 290px;
    padding: 8px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
  }
  h4 {
    margin: 6px 8px 8px;
    font-size: 13px;
    color: var(--text-dim);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .menu button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 10px;
    border-radius: 6px;
    text-align: left;
  }
  .menu button:hover {
    background: var(--hover);
  }
  .menu button.active {
    color: var(--accent);
  }
  .menu span {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .menu small {
    color: var(--text-dim);
    font-size: 12px;
  }
  .hint {
    margin: 8px 10px 4px;
    color: var(--text-dim);
    font-size: 12px;
    line-height: 1.4;
  }
  .hint.warn {
    color: var(--accent);
  }
</style>
