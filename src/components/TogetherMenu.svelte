<script>
  import Popover from "./Popover.svelte";
  import Icon from "./Icon.svelte";
  import { together, startRoom, joinRoom, leaveRoom, inviteLink } from "../lib/together.svelte.js";
  import { account } from "../lib/account.svelte.js";
  import { ui, showToast } from "../lib/ui.svelte.js";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";

  let code = $state("");
  let error = $state(null);

  async function run(action) {
    error = null;
    try {
      await action();
      code = "";
    } catch (e) {
      error = e?.code === "permission-denied" ? "Firebase verweigert den Zugriff." : String(e?.message ?? e);
    }
  }

  async function copyInvite() {
    const text = `Hör mit mir auf Wavely! Link: ${inviteLink(together.code)} – oder in Wavely unter „Zusammen hören“ den Code ${together.code} eingeben.`;
    await writeText(text).catch(() => {});
    showToast("Einladung kopiert – schick sie deinen Freunden");
  }

  function openAccount(close) {
    close();
    ui.view = "settings";
    setTimeout(() => document.getElementById("settings-account")?.scrollIntoView({ behavior: "smooth" }), 50);
  }

  const badge = $derived(
    together.role === "host" ? `${together.members.length}` : together.role === "guest" ? "Live" : null,
  );
</script>

<Popover icon="group" title="Zusammen hören" active={Boolean(together.role)} {badge} width={300}>
  {#snippet children({ close })}
    <h4>Zusammen hören</h4>

    {#if account.status !== "loggedIn"}
      <p class="hint">Hör mit Freunden synchron dieselben Songs. Dafür braucht ihr beide ein Wavely-Konto.</p>
      <div class="block"><button class="primary" onclick={() => openAccount(close)}>Anmelden oder Konto erstellen</button></div>
    {:else if together.role === "host"}
      <div class="block">
        <p class="label">Dein Code</p>
        <p class="code">{together.code}</p>
        <button class="primary" onclick={copyInvite}><Icon name="copy" size={18} /> Einladung kopieren</button>
      </div>
      <hr />
      <p class="label pad">{together.members.length ? "Hören gerade mit" : "Noch niemand dabei"}</p>
      {#each together.members as member (member.uid)}
        <div class="item"><span class="dot"></span> {member.name}</div>
      {/each}
      <hr />
      <button class="item" onclick={() => run(leaveRoom)}><Icon name="close" size={18} /> Beenden</button>
    {:else if together.role === "guest"}
      <p class="hint">Du hörst mit <strong>{together.hostName}</strong>. Songs, Pausen und Spulen kommen vom Gastgeber.</p>
      {#if together.notice}<p class="hint warn">{together.notice}</p>{/if}
      <button class="item" onclick={() => run(leaveRoom)}><Icon name="close" size={18} /> Verlassen</button>
    {:else}
      <p class="hint">Starte einen Raum und schick deinen Freunden den Code – sie hören dann genau das, was du hörst.</p>
      <div class="block">
        <button class="primary" disabled={together.busy} onclick={() => run(startRoom)}>
          <Icon name="group" size={18} /> Raum starten
        </button>
      </div>
      <hr />
      <form
        class="block join"
        onsubmit={(e) => {
          e.preventDefault();
          run(() => joinRoom(code));
        }}
      >
        <input bind:value={code} placeholder="Code oder Link" maxlength="40" />
        <button class="secondary" type="submit" disabled={together.busy || !code.trim()}>Mithören</button>
      </form>
    {/if}
    {#if error}<p class="hint warn">{error}</p>{/if}
  {/snippet}
</Popover>

<style>
  .block {
    padding: 4px 10px 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .label {
    margin: 0;
    color: var(--text-dim);
    font-size: 12px;
  }
  .pad {
    padding: 0 10px 4px;
  }
  .code {
    margin: 0;
    font-size: 32px;
    font-weight: 800;
    letter-spacing: 0.18em;
    font-variant-numeric: tabular-nums;
  }
  .join {
    flex-direction: row;
  }
  .join input {
    flex: 1;
    min-width: 0;
    height: 36px;
    padding: 0 10px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    text-transform: uppercase;
    outline: none;
  }
  .join input:focus {
    border-color: var(--accent);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #22c55e;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
