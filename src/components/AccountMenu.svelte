<script>
  import { ytm, logout } from "../lib/ytm.svelte.js";
  import { artworkSrc } from "../lib/artwork.js";
  import { ui } from "../lib/ui.svelte.js";
  import Icon from "./Icon.svelte";

  let open = $state(false);
  let root = $state();

  const initial = $derived((ytm.account?.name ?? "?").trim().charAt(0).toUpperCase());

  function close(event) {
    if (open && !root?.contains(event.target)) open = false;
  }

  async function signOut() {
    open = false;
    await logout();
  }
</script>

<svelte:window onmousedown={close} onkeydown={(e) => e.key === "Escape" && (open = false)} />

<div class="account" bind:this={root}>
  <button class="avatar" title={ytm.account?.name ?? "Konto"} onclick={() => (open = !open)}>
    {#if ytm.account?.photoUrl}
      <img src={artworkSrc(ytm.account.photoUrl)} alt="" />
    {:else}
      <span>{initial}</span>
    {/if}
  </button>

  {#if open}
    <div class="menu" role="menu">
      <div class="who">
        <div class="avatar big">
          {#if ytm.account?.photoUrl}
            <img src={artworkSrc(ytm.account.photoUrl)} alt="" />
          {:else}
            <span>{initial}</span>
          {/if}
        </div>
        <div class="names">
          <strong>{ytm.account?.name ?? "Angemeldet"}</strong>
          {#if ytm.account?.handle}<small>{ytm.account.handle}</small>{/if}
        </div>
      </div>
      <button
        role="menuitem"
        onclick={() => {
          open = false;
          ui.view = "settings";
        }}
      >
        <Icon name="settings" size={18} /> Einstellungen
      </button>
      <button role="menuitem" onclick={signOut}>
        <Icon name="logout" size={18} /> Abmelden
      </button>
    </div>
  {/if}
</div>

<style>
  .account {
    position: relative;
  }
  .avatar {
    width: 38px;
    height: 38px;
    border-radius: 50%;
    overflow: hidden;
    background: var(--accent);
    color: #fff;
    font-weight: 700;
    display: grid;
    place-items: center;
    flex-shrink: 0;
  }
  button.avatar:hover {
    box-shadow: 0 0 0 3px var(--hover);
  }
  .avatar img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .avatar.big {
    width: 44px;
    height: 44px;
  }
  .menu {
    position: absolute;
    right: 0;
    top: calc(100% + 8px);
    z-index: 50;
    min-width: 240px;
    padding: 6px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
  }
  .who {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 10px 12px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 6px;
  }
  .names {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .names strong,
  .names small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .names small {
    color: var(--text-dim);
  }
  .menu button {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 10px;
    border-radius: 6px;
    text-align: left;
  }
  .menu button:hover {
    background: var(--hover);
  }
</style>
