<script>
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { settings, updateSetting, resetAppearance, ACCENTS } from "../lib/settings.svelte.js";
  import { ui, toggleSidebar, showToast } from "../lib/ui.svelte.js";
  import { player, toggleAutoplay, sendPresence } from "../lib/player.svelte.js";
  import { library, addFolder, removeFolder } from "../lib/library.svelte.js";
  import { ytm, login, logout } from "../lib/ytm.svelte.js";
  import { spotify, login as spotifyLogin, logout as spotifyLogout } from "../lib/spotify.svelte.js";
  import { spotifyOut } from "../lib/spotifyPlayer.svelte.js";
  import { invoke } from "@tauri-apps/api/core";
  import { soundcloud, login as soundcloudLogin, logout as soundcloudLogout } from "../lib/soundcloud.svelte.js";
  import { offlineStats, openOfflineFolder, clearOffline } from "../lib/offline.svelte.js";
  import { artworkSrc } from "../lib/artwork.js";
  import Icon from "./Icon.svelte";
  import Logo from "./Logo.svelte";
  import { updater, checkForUpdates, installUpdate } from "../lib/updater.svelte.js";

  let version = $state("");
  let stats = $state(null);
  let confirmClear = $state(false);

  const sections = [
    { id: "appearance", label: "Darstellung" },
    { id: "playback", label: "Wiedergabe" },
    { id: "discord", label: "Discord" },
    { id: "library", label: "Bibliothek" },
    { id: "youtube", label: "YouTube Music" },
    { id: "spotify", label: "Spotify" },
    { id: "soundcloud", label: "SoundCloud" },
    { id: "offline", label: "Offline" },
    { id: "about", label: "Über Wavely" },
  ];

  const themes = [
    { value: "system", label: "Wie Windows" },
    { value: "dark", label: "Dunkel" },
    { value: "light", label: "Hell" },
  ];

  const isPreset = $derived(ACCENTS.some((a) => a.value === settings.accent));

  // Eigene Spotify Client ID (für Spotify Premium / Geräteauswahl nötig)
  let clientId = $state("");
  let clientIdDefault = $state(true);
  let clientIdError = $state(null);

  async function saveClientId(reset = false) {
    clientIdError = null;
    try {
      await invoke("spotify_set_client_id", { clientId: reset ? null : clientId });
      await spotifyLogout();
      const info = await invoke("spotify_get_client_id");
      clientId = info.isDefault ? "" : info.clientId;
      clientIdDefault = info.isDefault;
      showToast(reset ? "Standard-Client-ID wiederhergestellt – bitte neu anmelden" : "Client ID gespeichert – bitte bei Spotify neu anmelden");
    } catch (e) {
      clientIdError = String(e);
    }
  }

  onMount(async () => {
    invoke("spotify_get_client_id")
      .then((info) => {
        clientId = info.isDefault ? "" : info.clientId;
        clientIdDefault = info.isDefault;
      })
      .catch(() => {});
    version = await getVersion().catch(() => "");
    refreshStats();
  });

  async function refreshStats() {
    stats = await offlineStats().catch(() => null);
  }

  function formatBytes(bytes) {
    if (!bytes) return "0 MB";
    const mb = bytes / 1024 / 1024;
    return mb >= 1024 ? `${(mb / 1024).toFixed(2)} GB` : `${mb.toFixed(0)} MB`;
  }

  function setDiscord(key, value) {
    updateSetting(key, value);
    sendPresence();
  }

  async function clearAll() {
    confirmClear = false;
    await clearOffline();
    await refreshStats();
    showToast("Alle Offline-Songs wurden gelöscht");
  }

  function scrollTo(id) {
    document.getElementById(`settings-${id}`)?.scrollIntoView({ behavior: "smooth", block: "start" });
  }
</script>

{#snippet toggle(checked, onchange, label)}
  <button class="switch" class:on={checked} role="switch" aria-checked={checked} aria-label={label} onclick={onchange}>
    <span></span>
  </button>
{/snippet}

<div class="settings">
  <nav class="toc">
    <h1>Einstellungen</h1>
    {#each sections as s (s.id)}
      <button onclick={() => scrollTo(s.id)}>{s.label}</button>
    {/each}
  </nav>

  <div class="content">
    <!-- Darstellung -->
    <section id="settings-appearance">
      <h2>Darstellung</h2>
      <div class="card">
        <div class="row">
          <div class="label">
            <strong>Design</strong>
            <small>Heller oder dunkler Modus</small>
          </div>
          <div class="segmented">
            {#each themes as t (t.value)}
              <button class:active={settings.theme === t.value} onclick={() => updateSetting("theme", t.value)}>
                {t.label}
              </button>
            {/each}
          </div>
        </div>
        <div class="row column">
          <div class="label">
            <strong>Akzentfarbe</strong>
            <small>Für Buttons, Hervorhebungen und den Fortschrittsbalken</small>
          </div>
          <div class="swatches">
            {#each ACCENTS as a (a.value)}
              <button
                class="swatch"
                class:active={settings.accent === a.value}
                style:background={a.value}
                title={a.name}
                aria-label={a.name}
                onclick={() => updateSetting("accent", a.value)}
              ></button>
            {/each}
            <label class="swatch custom" class:active={!isPreset} title="Eigene Farbe">
              <input
                type="color"
                value={settings.accent}
                oninput={(e) => updateSetting("accent", e.currentTarget.value)}
              />
              <Icon name="plus" size={16} />
            </label>
          </div>
        </div>
        <div class="row">
          <div class="label">
            <strong>Seitenleiste einklappen</strong>
            <small>Nur Symbole anzeigen, mehr Platz für Musik</small>
          </div>
          {@render toggle(ui.sidebarCollapsed, toggleSidebar, "Seitenleiste einklappen")}
        </div>
        <div class="row">
          <div class="label">
            <strong>Darstellung zurücksetzen</strong>
            <small>Dunkles Design mit pinker Akzentfarbe</small>
          </div>
          <button class="secondary" onclick={resetAppearance}>Zurücksetzen</button>
        </div>
      </div>
    </section>

    <!-- Wiedergabe -->
    <section id="settings-playback">
      <h2>Wiedergabe</h2>
      <div class="card">
        <div class="row">
          <div class="label">
            <strong>Autoplay</strong>
            <small>Nach dem Ende der Warteschlange passende Songs abspielen</small>
          </div>
          {@render toggle(player.autoplay, toggleAutoplay, "Autoplay")}
        </div>
        <div class="row">
          <div class="label">
            <strong>Startansicht</strong>
            <small>Womit Wavely beim Öffnen startet</small>
          </div>
          <select value={settings.startView} onchange={(e) => updateSetting("startView", e.currentTarget.value)}>
            <option value="youtube">YouTube Music</option>
            <option value="spotify">Spotify</option>
            <option value="soundcloud">SoundCloud</option>
            <option value="local">Lokale Musik</option>
            <option value="offline">Offline</option>
          </select>
        </div>
      </div>
    </section>

    <!-- Discord -->
    <section id="settings-discord">
      <h2>Discord</h2>
      <div class="card">
        <div class="row">
          <div class="label">
            <strong>Im Discord-Status anzeigen</strong>
            <small>Zeigt deinen Freunden, was du gerade hörst (Discord muss laufen)</small>
          </div>
          {@render toggle(settings.discordEnabled, () => setDiscord("discordEnabled", !settings.discordEnabled), "Discord-Status")}
        </div>
        <div class="row" class:disabled={!settings.discordEnabled}>
          <div class="label">
            <strong>Anzeige in der Mitgliederliste</strong>
            <small>„Hört …“ neben deinem Namen</small>
          </div>
          <select
            value={settings.discordDisplay}
            disabled={!settings.discordEnabled}
            onchange={(e) => setDiscord("discordDisplay", e.currentTarget.value)}
          >
            <option value="details">Songtitel</option>
            <option value="state">Künstler</option>
            <option value="name">„Wavely“</option>
          </select>
        </div>
      </div>
    </section>

    <!-- Bibliothek -->
    <section id="settings-library">
      <h2>Bibliothek</h2>
      <div class="card">
        <div class="row column">
          <div class="label">
            <strong>Musikordner</strong>
            <small>Ordner mit lokalen Musikdateien (MP3, FLAC, M4A, OGG, OPUS, WAV)</small>
          </div>
          <div class="folders">
            {#each library.folders as folder (folder)}
              <div class="folder">
                <Icon name="folder" size={18} />
                <span title={folder}>{folder}</span>
                <button class="ghost small" title="Entfernen" onclick={() => removeFolder(folder)}>
                  <Icon name="close" size={16} />
                </button>
              </div>
            {:else}
              <p class="hint">Noch keine Ordner hinzugefügt.</p>
            {/each}
          </div>
          <button class="secondary fit" onclick={addFolder}><Icon name="plus" size={18} /> Ordner hinzufügen</button>
        </div>
      </div>
    </section>

    <!-- YouTube Music -->
    <section id="settings-youtube">
      <h2>YouTube Music</h2>
      <div class="card">
        {#if ytm.status === "loggedIn"}
          <div class="row">
            <div class="account">
              <div class="avatar">
                {#if ytm.account?.photoUrl}
                  <img src={artworkSrc(ytm.account.photoUrl)} alt="" />
                {:else}
                  {(ytm.account?.name ?? "?").charAt(0)}
                {/if}
              </div>
              <div class="label">
                <strong>{ytm.account?.name ?? "Angemeldet"}</strong>
                <small>{ytm.account?.handle ?? "Mit Google angemeldet"}</small>
              </div>
            </div>
            <button class="secondary" onclick={logout}><Icon name="logout" size={18} /> Abmelden</button>
          </div>
          <div class="row">
            <div class="label">
              <strong>Verlauf mit YouTube Music abgleichen</strong>
              <small>In Wavely gehörte Songs erscheinen auch in deinem YouTube-Music-Verlauf</small>
            </div>
            {@render toggle(settings.syncYouTubeHistory, () => updateSetting("syncYouTubeHistory", !settings.syncYouTubeHistory), "Verlauf abgleichen")}
          </div>
        {:else}
          <div class="row">
            <div class="label">
              <strong>Nicht angemeldet</strong>
              <small>Anmelden, um Playlists, Likes und deine Bibliothek zu sehen</small>
            </div>
            <button class="primary" onclick={login} disabled={ytm.loggingIn}>Mit Google anmelden</button>
          </div>
        {/if}
      </div>
    </section>

    <!-- Spotify -->
    <section id="settings-spotify">
      <h2>Spotify</h2>
      <div class="card">
        <div class="row">
          {#if spotify.status === "loggedIn"}
            <div class="account">
              <div class="avatar">
                {#if spotify.account?.photoUrl}
                  <img src={artworkSrc(spotify.account.photoUrl)} alt="" />
                {:else}
                  {(spotify.account?.name ?? "S").charAt(0)}
                {/if}
              </div>
              <div class="label">
                <strong>{spotify.account?.name ?? "Angemeldet"}</strong>
                <small>Abgespielt über YouTube Music</small>
              </div>
            </div>
            <button class="secondary" onclick={spotifyLogout}><Icon name="logout" size={18} /> Abmelden</button>
          {:else}
            <div class="label">
              <strong>Nicht angemeldet</strong>
              <small>Spotify-Playlists, Lieblingssongs und Alben in Wavely nutzen</small>
            </div>
            <button class="primary" onclick={spotifyLogin} disabled={spotify.loggingIn}>Mit Spotify anmelden</button>
          {/if}
        </div>
      </div>
    </section>

    <!-- Spotify: Wiedergabe & eigene App -->
    <section id="settings-spotify-app">
      <div class="card">
        <div class="row column">
          <div class="label">
            <strong>Spotify-Wiedergabe</strong>
            <small>
              {#if spotifyOut.premium === true}
                Premium erkannt: Spotify-Songs laufen direkt über Spotify. Das Gerät wählst du unten in der Player-Leiste.
              {:else if spotifyOut.premium === false}
                Spotify Free: Spotify-Songs laufen über YouTube Music. Mit Premium spielen sie direkt über Spotify.
              {:else}
                Mit Spotify Premium laufen Spotify-Songs direkt über Spotify (auch auf anderen Geräten), sonst über YouTube Music.
              {/if}
            </small>
          </div>
          {#if spotifyOut.notice}<p class="hint warn">{spotifyOut.notice}</p>{/if}
        </div>
        <div class="row column">
          <div class="label">
            <strong>Eigene Spotify Client ID</strong>
            <small>
              Spotify verlangt seit 2026, dass der Inhaber einer Entwickler-App selbst Premium hat (max. 5 Nutzer).
              Lege dazu auf developer.spotify.com eine App an (Redirect URI: http://127.0.0.1:43827/callback, „Web API“ und
              „Web Playback SDK“) und trage hier ihre Client ID ein.
            </small>
          </div>
          <div class="client-id">
            <input bind:value={clientId} placeholder={clientIdDefault ? "Standard (Wavely)" : ""} maxlength="32" spellcheck="false" />
            <button class="secondary" onclick={() => saveClientId(false)} disabled={!clientId.trim()}>Speichern</button>
            {#if !clientIdDefault}
              <button class="secondary" onclick={() => saveClientId(true)}>Standard</button>
            {/if}
          </div>
          {#if clientIdError}<p class="hint warn">{clientIdError}</p>{/if}
        </div>
      </div>
    </section>

    <!-- SoundCloud -->
    <section id="settings-soundcloud">
      <h2>SoundCloud</h2>
      <div class="card">
        <div class="row">
          {#if soundcloud.status === "loggedIn"}
            <div class="account">
              <div class="avatar">
                {#if soundcloud.account?.photoUrl}
                  <img src={artworkSrc(soundcloud.account.photoUrl)} alt="" />
                {:else}
                  {(soundcloud.account?.name ?? "S").charAt(0)}
                {/if}
              </div>
              <div class="label">
                <strong>{soundcloud.account?.name ?? "Angemeldet"}</strong>
                <small>Ausschnitt-Songs (Go+) laufen über YouTube Music</small>
              </div>
            </div>
            <button class="secondary" onclick={soundcloudLogout}><Icon name="logout" size={18} /> Abmelden</button>
          {:else}
            <div class="label">
              <strong>Nicht angemeldet</strong>
              <small>Likes, Playlists und gefolgte Künstler in Wavely nutzen</small>
            </div>
            <button class="primary" onclick={soundcloudLogin} disabled={soundcloud.loggingIn}>Mit SoundCloud anmelden</button>
          {/if}
        </div>
      </div>
    </section>

    <!-- Offline -->
    <section id="settings-offline">
      <h2>Offline</h2>
      <div class="card">
        <div class="row">
          <div class="label">
            <strong>Gespeicherte Songs</strong>
            <small>{stats ? `${stats.songs} Songs · ${formatBytes(stats.bytes)}` : "Wird berechnet…"}</small>
          </div>
          <button class="secondary" onclick={openOfflineFolder}><Icon name="folder" size={18} /> Ordner öffnen</button>
        </div>
        <div class="row">
          <div class="label">
            <strong>Alle Offline-Songs löschen</strong>
            <small>Gibt den Speicherplatz frei; die Songs bleiben bei YouTube Music erhalten</small>
          </div>
          {#if confirmClear}
            <div class="confirm">
              <button class="secondary" onclick={() => (confirmClear = false)}>Abbrechen</button>
              <button class="danger" onclick={clearAll}>Wirklich löschen</button>
            </div>
          {:else}
            <button class="secondary" onclick={() => (confirmClear = true)} disabled={!stats?.songs}>
              <Icon name="trash" size={18} /> Löschen
            </button>
          {/if}
        </div>
      </div>
    </section>

    <!-- Über -->
    <section id="settings-about">
      <h2>Über Wavely</h2>
      <div class="card">
        <div class="row">
          <div class="account">
            <Logo size={44} />
            <div class="label">
              <strong>Wavely {version}</strong>
              <small>Leichtgewichtiger Musikplayer für lokale Musik, YouTube Music, Spotify und SoundCloud</small>
            </div>
          </div>
        </div>
        <div class="row">
          <div class="label">
            <strong>Updates</strong>
            <small>
              {#if updater.status === "checking"}
                Suche nach Updates…
              {:else if updater.status === "none"}
                Wavely ist auf dem neuesten Stand.
              {:else if updater.status === "available"}
                Version {updater.version} ist verfügbar.
              {:else if updater.status === "downloading"}
                Wird heruntergeladen{updater.progress != null ? ` (${Math.round(updater.progress * 100)} %)` : "…"}
              {:else if updater.status === "installing"}
                Wird installiert – Wavely startet gleich neu…
              {:else if updater.status === "error"}
                Update fehlgeschlagen: {updater.error}
              {:else}
                Neue Versionen kommen direkt von GitHub und werden vor der Installation auf ihre Echtheit geprüft.
              {/if}
            </small>
          </div>
          {#if updater.status === "available"}
            <button class="primary" onclick={installUpdate}><Icon name="download" size={18} /> Jetzt aktualisieren</button>
          {:else}
            <button
              class="secondary"
              onclick={() => checkForUpdates(false)}
              disabled={["checking", "downloading", "installing"].includes(updater.status)}
            >
              Nach Updates suchen
            </button>
          {/if}
        </div>
        <div class="row">
          <div class="label">
            <strong>Automatisch nach Updates suchen</strong>
            <small>Beim Start und alle paar Stunden; installiert wird erst, wenn du es bestätigst</small>
          </div>
          {@render toggle(settings.autoUpdateCheck, () => updateSetting("autoUpdateCheck", !settings.autoUpdateCheck), "Automatisch nach Updates suchen")}
        </div>
        <div class="row column">
          <div class="label">
            <strong>Verwendete Projekte</strong>
            <small>
              Tauri, Svelte, RustyPipe (GPL-3.0), yt-dlp (Unlicense), LRCLIB (Liedtexte), lofty, discord-rich-presence
            </small>
          </div>
        </div>
      </div>
    </section>
  </div>
</div>

<style>
  .settings {
    display: grid;
    grid-template-columns: 200px minmax(0, 720px);
    gap: 40px;
    padding: 28px 32px 60px;
  }
  .toc {
    position: sticky;
    top: 0;
    align-self: start;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .toc h1 {
    margin: 0 0 16px;
    font-size: 26px;
  }
  .toc button {
    text-align: left;
    padding: 8px 10px;
    border-radius: 8px;
    color: var(--text-dim);
  }
  .toc button:hover {
    background: var(--hover);
    color: var(--text);
  }
  .content {
    display: flex;
    flex-direction: column;
    gap: 32px;
    padding-top: 6px;
  }
  section {
    scroll-margin-top: 20px;
  }
  h2 {
    margin: 0 0 12px;
    font-size: 18px;
  }
  .card {
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 12px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    padding: 16px 18px;
  }
  .row + .row {
    border-top: 1px solid var(--border);
  }
  .row.column {
    flex-direction: column;
    align-items: stretch;
    gap: 12px;
  }
  .row.disabled .label {
    opacity: 0.5;
  }
  .label {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .label small {
    color: var(--text-dim);
    font-size: 13px;
    line-height: 1.4;
  }
  .segmented {
    display: flex;
    gap: 2px;
    padding: 3px;
    border-radius: 999px;
    background: var(--surface-2);
    flex-shrink: 0;
  }
  .segmented button {
    padding: 6px 14px;
    border-radius: 999px;
    color: var(--text-dim);
    font-weight: 600;
    font-size: 13px;
  }
  .segmented button.active {
    background: var(--accent);
    color: #fff;
  }
  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
  }
  .swatch {
    width: 32px;
    height: 32px;
    border-radius: 50%;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.15);
    transition: transform 0.1s;
  }
  .swatch:hover {
    transform: scale(1.08);
  }
  .swatch.active {
    box-shadow:
      0 0 0 2px var(--surface),
      0 0 0 4px var(--text);
  }
  .swatch.custom {
    display: grid;
    place-items: center;
    background: var(--surface-2);
    color: var(--text-dim);
    cursor: pointer;
    position: relative;
    overflow: hidden;
  }
  .swatch.custom input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }
  select {
    height: 36px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
    flex-shrink: 0;
  }
  .switch {
    width: 40px;
    height: 23px;
    border-radius: 12px;
    background: var(--hover);
    position: relative;
    flex-shrink: 0;
    transition: background 0.15s;
  }
  .switch span {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 17px;
    height: 17px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3);
    transition: transform 0.15s;
  }
  .switch.on {
    background: var(--accent);
  }
  .switch.on span {
    transform: translateX(17px);
  }
  .folders {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 6px 6px 10px;
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text-dim);
  }
  .folder span {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
  }
  .ghost.small {
    width: 30px;
    height: 30px;
  }
  .fit {
    align-self: flex-start;
  }
  .hint {
    margin: 0;
    color: var(--text-dim);
  }
  .account {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
  }
  .avatar {
    width: 44px;
    height: 44px;
    border-radius: 50%;
    overflow: hidden;
    flex-shrink: 0;
    background: var(--accent);
    color: #fff;
    font-weight: 700;
    display: grid;
    place-items: center;
  }
  .avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .client-id {
    display: flex;
    gap: 8px;
  }
  .client-id input {
    flex: 1;
    height: 38px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
    font-family: ui-monospace, Consolas, monospace;
    outline: none;
  }
  .client-id input:focus {
    border-color: var(--accent);
  }
  .hint.warn {
    color: var(--accent);
    margin: 0;
  }
  .confirm {
    display: flex;
    gap: 8px;
    flex-shrink: 0;
  }
  .danger {
    height: 38px;
    padding: 0 16px;
    border-radius: 999px;
    background: #e5484d;
    color: #fff;
    font-weight: 600;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .row :global(.secondary),
  .row :global(.primary) {
    flex-shrink: 0;
  }

  @media (max-width: 900px) {
    .settings {
      grid-template-columns: minmax(0, 1fr);
      gap: 16px;
    }
    .toc {
      position: static;
      flex-direction: row;
      flex-wrap: wrap;
    }
    .toc h1 {
      width: 100%;
    }
  }
</style>
