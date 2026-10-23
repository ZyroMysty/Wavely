<script>
  import Popover from "./Popover.svelte";
  import Icon from "./Icon.svelte";
  import { spotifyOut, targetDevice, loadDevices, selectDevice, initSpotifyPlayer } from "../lib/spotifyPlayer.svelte.js";
  import { spotify } from "../lib/spotify.svelte.js";
  import { player, currentTrack } from "../lib/player.svelte.js";
  import { settings } from "../lib/settings.svelte.js";
  import { sound, loadOutputs, selectOutput } from "../lib/sound.svelte.js";

  let loading = $state(false);

  const spotifyOn = $derived(spotify.status === "loggedIn");
  const target = $derived(spotifyOn ? targetDevice() : "wavely");
  const remoteName = $derived(
    target === "wavely" ? null : (spotifyOut.devices.find((d) => d.id === target)?.name ?? "anderes Gerät"),
  );
  const output = $derived(settings.outputDevice || "");
  const outputName = $derived(sound.outputs.find((o) => o.id === output)?.name);

  async function opened() {
    loading = true;
    const jobs = [loadOutputs()];
    if (spotifyOn) jobs.push(initSpotifyPlayer().then(loadDevices));
    await Promise.all(jobs);
    loading = false;
  }

  /** Ausgabe an diesem PC: Gerät wählen und Spotify (falls woanders) zurück zu Wavely holen */
  async function chooseOutput(id, close) {
    close();
    await selectOutput(id);
    if (spotifyOn && target !== "wavely") await chooseSpotify("wavely");
  }

  async function chooseSpotify(id, close) {
    close?.();
    const playingSpotify = currentTrack()?.source === "spotify" && player.playing;
    await selectDevice(id, playingSpotify);
  }

  const iconFor = (kind) => ({ Smartphone: "phone", Speaker: "speaker", TV: "tv", Computer: "computer" })[kind] ?? "devices";
  const outputIcon = (name = "") => (/kopfh|headph|headset|earbud|airpods|buds/i.test(name) ? "headphones" : "speaker");
  const title = $derived(
    remoteName ? `Spotify läuft auf: ${remoteName}` : outputName ? `Ausgabe: ${outputName}` : "Ausgabegerät wählen",
  );
</script>

<Popover icon="devices" {title} active={Boolean(remoteName || output)} width={300} onopen={opened}>
  {#snippet children({ close })}
    <h4>Dieser PC</h4>
    <button class="item" class:active={!remoteName && !output} onclick={() => chooseOutput("", close)}>
      <Icon name="computer" size={18} />
      <span>
        <strong>Windows-Standard</strong>
        <small>Das Gerät, das in Windows eingestellt ist</small>
      </span>
    </button>
    {#if loading && !sound.outputs.length}
      <p class="hint">Geräte werden gesucht…</p>
    {:else}
      {#each sound.outputs as device (device.id)}
        <button class="item" class:active={!remoteName && output === device.id} onclick={() => chooseOutput(device.id, close)}>
          <Icon name={outputIcon(device.name)} size={18} />
          <span><strong>{device.name}</strong></span>
        </button>
      {/each}
    {/if}

    {#if spotifyOn}
      <hr />
      <h4>Spotify Connect</h4>
      {#if loading}
        <p class="hint">Geräte werden gesucht…</p>
      {:else}
        {#each spotifyOut.devices as device (device.id)}
          <button class="item" class:active={target === device.id} onclick={() => chooseSpotify(device.id, close)}>
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
        <p class="hint">Spotify-Geräte lassen sich nur mit Spotify Premium steuern.</p>
      {/if}
      {#if spotifyOut.notice}<p class="hint warn">{spotifyOut.notice}</p>{/if}
    {/if}
  {/snippet}
</Popover>

<style>
  span {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  strong {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
