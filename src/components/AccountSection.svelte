<script>
  import { account, register, login, logout, resetPassword, rename, deleteAccount, errorText } from "../lib/account.svelte.js";
  import { sync } from "../lib/sync.svelte.js";
  import { showToast } from "../lib/ui.svelte.js";
  import Icon from "./Icon.svelte";

  /** "login" | "register" | "reset" */
  let mode = $state("login");
  let name = $state("");
  let email = $state("");
  let password = $state("");
  let busy = $state(false);
  let error = $state(null);
  let confirmDelete = $state(false);
  let deletePassword = $state("");
  let editingName = $state(false);
  let newName = $state("");

  async function submit(event) {
    event.preventDefault();
    error = null;
    busy = true;
    try {
      if (mode === "register") await register(name, email, password);
      else if (mode === "login") await login(email, password);
      else {
        await resetPassword(email);
        showToast("E-Mail zum Zurücksetzen verschickt");
        mode = "login";
      }
      password = "";
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function removeAccount() {
    error = null;
    busy = true;
    try {
      await deleteAccount(deletePassword);
      confirmDelete = false;
      deletePassword = "";
    } catch (e) {
      error = errorText(e);
    } finally {
      busy = false;
    }
  }

  async function saveName() {
    editingName = false;
    try {
      await rename(newName);
    } catch (e) {
      error = errorText(e);
    }
  }

  const syncText = $derived(
    {
      off: "Abgleich aus",
      syncing: "Wird abgeglichen…",
      ready: "Verlauf, Rückblick und Wavely-Playlists sind auf allen Geräten gleich",
      error: `Abgleich fehlgeschlagen: ${sync.error}`,
    }[sync.status],
  );
</script>

<section id="settings-account">
  <h2>Wavely-Konto</h2>
  <div class="card">
    {#if account.status === "checking"}
      <div class="row"><div class="label"><small>Wird geprüft…</small></div></div>
    {:else if account.status === "loggedIn"}
      <div class="row">
        <div class="account">
          <div class="avatar">{account.user.name.charAt(0).toUpperCase()}</div>
          <div class="label">
            {#if editingName}
              <input
                class="name-input"
                bind:value={newName}
                maxlength="40"
                onblur={saveName}
                onkeydown={(e) => e.key === "Enter" && saveName()}
              />
            {:else}
              <strong>
                {account.user.name}
                <button class="edit" title="Namen ändern" onclick={() => ((newName = account.user.name), (editingName = true))}>
                  <Icon name="edit" size={14} />
                </button>
              </strong>
            {/if}
            <small>{account.user.email}</small>
          </div>
        </div>
        <button class="secondary" onclick={logout}><Icon name="logout" size={18} /> Abmelden</button>
      </div>
      <div class="row">
        <div class="label">
          <strong>Abgleich</strong>
          <small class:warn={sync.status === "error"}>{syncText}</small>
        </div>
        <span class="state" class:ok={sync.status === "ready"}><Icon name={sync.status === "ready" ? "check" : "refresh"} size={18} /></span>
      </div>
      <div class="row column">
        {#if confirmDelete}
          <div class="label">
            <strong>Konto wirklich löschen?</strong>
            <small>Dein Konto und alle Daten in der Cloud werden gelöscht. Was auf diesem PC gespeichert ist, bleibt.</small>
          </div>
          <div class="inline">
            <input type="password" bind:value={deletePassword} placeholder="Passwort zur Bestätigung" />
            <button class="secondary danger" onclick={removeAccount} disabled={!deletePassword || busy}>Endgültig löschen</button>
            <button class="secondary" onclick={() => (confirmDelete = false)}>Abbrechen</button>
          </div>
        {:else}
          <button class="link danger" onclick={() => (confirmDelete = true)}>Konto löschen…</button>
        {/if}
      </div>
    {:else}
      <div class="row column">
        <div class="label">
          <strong>{mode === "register" ? "Konto erstellen" : mode === "reset" ? "Passwort vergessen" : "Anmelden"}</strong>
          <small>
            Mit einem Wavely-Konto sind Verlauf, Rückblick und deine Wavely-Playlists auf jedem PC gleich – und du kannst
            mit Freunden zusammen hören.
          </small>
        </div>
        <form onsubmit={submit}>
          {#if mode === "register"}
            <input bind:value={name} placeholder="Name (sehen deine Freunde)" maxlength="40" autocomplete="nickname" />
          {/if}
          <input type="email" bind:value={email} placeholder="E-Mail" autocomplete="email" required />
          {#if mode !== "reset"}
            <input
              type="password"
              bind:value={password}
              placeholder={mode === "register" ? "Passwort (mind. 6 Zeichen)" : "Passwort"}
              autocomplete={mode === "register" ? "new-password" : "current-password"}
              required
            />
          {/if}
          <button class="primary" type="submit" disabled={busy}>
            {busy ? "Bitte warten…" : mode === "register" ? "Konto erstellen" : mode === "reset" ? "Link schicken" : "Anmelden"}
          </button>
        </form>
        <div class="switch-mode">
          {#if mode === "login"}
            <button class="link" onclick={() => (mode = "register")}>Noch kein Konto? Jetzt erstellen</button>
            <button class="link" onclick={() => (mode = "reset")}>Passwort vergessen?</button>
          {:else}
            <button class="link" onclick={() => (mode = "login")}>Zurück zur Anmeldung</button>
          {/if}
        </div>
        <p class="privacy">
          Gespeichert werden E-Mail, Name und was du hörst (Titel, Künstler, Zeitpunkt) sowie deine Wavely-Playlists – in
          Google Firebase (EU). Pfade lokaler Dateien bleiben auf deinem PC. Du kannst das Konto jederzeit samt Daten löschen.
        </p>
      </div>
    {/if}
    {#if error}<p class="note warn">{error}</p>{/if}
  </div>
</section>

<style>
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
  .label small.warn,
  .note.warn,
  .danger {
    color: var(--accent);
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
    flex-shrink: 0;
    border-radius: 50%;
    background: linear-gradient(135deg, var(--accent), #6d28d9);
    color: #fff;
    font-weight: 700;
    font-size: 18px;
    display: grid;
    place-items: center;
  }
  .edit {
    color: var(--text-dim);
    vertical-align: -2px;
    margin-left: 4px;
  }
  .edit:hover {
    color: var(--text);
  }
  form,
  .inline {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  input {
    flex: 1;
    min-width: 160px;
    height: 40px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
    font-size: 14px;
    outline: none;
  }
  input:focus {
    border-color: var(--accent);
  }
  .name-input {
    height: 30px;
    max-width: 260px;
  }
  .switch-mode {
    display: flex;
    gap: 18px;
  }
  .link {
    align-self: flex-start;
    color: var(--accent);
    font-size: 13px;
  }
  .link:hover {
    text-decoration: underline;
  }
  .state {
    color: var(--text-dim);
  }
  .state.ok {
    color: var(--accent);
  }
  .privacy {
    margin: 0;
    color: var(--text-dim);
    font-size: 12px;
    line-height: 1.45;
  }
  .note {
    margin: 0;
    padding: 12px 18px 16px;
    border-top: 1px solid var(--border);
    font-size: 13px;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
