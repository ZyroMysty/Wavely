<script>
  import { library, addFolder, removeFolder } from "../lib/library.svelte.js";
  import { ui, toggleSidebar } from "../lib/ui.svelte.js";
  import Icon from "./Icon.svelte";
  import Logo from "./Logo.svelte";

  const folderName = (path) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;
  const collapsed = $derived(ui.sidebarCollapsed);

  const items = [
    { id: "local", icon: "folder", label: "Lokale Musik" },
    { id: "youtube", icon: "youtube", label: "YouTube Music" },
    { id: "spotify", icon: "spotify", label: "Spotify" },
    { id: "soundcloud", icon: "soundcloud", label: "SoundCloud" },
    { id: "offline", icon: "download", label: "Offline" },
  ];

  function open(id) {
    // Erneuter Klick auf YouTube Music führt zurück zur Bibliothek
    if (id === "youtube" && ui.view === "youtube") ui.ytmHome++;
    if (id === "spotify" && ui.view === "spotify") ui.spHome++;
    if (id === "soundcloud" && ui.view === "soundcloud") ui.scHome++;
    ui.view = id;
  }
</script>

<aside class:collapsed>
  <div class="brand">
    <Logo size={30} />
    {#if !collapsed}<span class="name">Wavely</span>{/if}
    <button
      class="icon-btn toggle"
      title={collapsed ? "Seitenleiste ausklappen" : "Seitenleiste einklappen"}
      onclick={toggleSidebar}
    >
      <Icon name={collapsed ? "expand" : "collapse"} size={18} />
    </button>
  </div>

  <nav>
    {#each items as item (item.id)}
      <button class:active={ui.view === item.id} title={collapsed ? item.label : null} onclick={() => open(item.id)}>
        <Icon name={item.icon} size={18} />
        {#if !collapsed}{item.label}{/if}
      </button>
    {/each}
  </nav>

  {#if collapsed}
    <button class="icon-btn add" title="Ordner hinzufügen" onclick={addFolder}>
      <Icon name="plus" size={18} />
    </button>
  {:else}
    <div class="section">
      <div class="section-title">
        Ordner
        <button class="icon-btn" title="Ordner hinzufügen" onclick={addFolder}>
          <Icon name="plus" size={16} />
        </button>
      </div>
      {#each library.folders as folder (folder)}
        <div class="folder" title={folder}>
          <span>{folderName(folder)}</span>
          <button class="icon-btn remove" title="Ordner entfernen" onclick={() => removeFolder(folder)}>
            <Icon name="close" size={14} />
          </button>
        </div>
      {:else}
        <p class="hint">Noch keine Ordner</p>
      {/each}
    </div>
  {/if}

  <nav class="bottom">
    <button class:active={ui.view === "settings"} title={collapsed ? "Einstellungen" : null} onclick={() => open("settings")}>
      <Icon name="settings" size={18} />
      {#if !collapsed}Einstellungen{/if}
    </button>
  </nav>
</aside>

<style>
  aside {
    grid-area: sidebar;
    background: var(--surface);
    border-right: 1px solid var(--border);
    padding: 20px 12px;
    display: flex;
    flex-direction: column;
    gap: 24px;
    overflow-y: auto;
    overflow-x: hidden;
  }
  aside.collapsed {
    padding: 20px 10px;
    align-items: center;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 18px;
    font-weight: 700;
    padding: 0 0 0 8px;
  }
  .collapsed .brand {
    flex-direction: column;
    padding: 0;
  }
  .name {
    flex: 1;
  }
  .bottom {
    margin-top: auto;
  }
  nav {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  nav button {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 10px;
    border-radius: 8px;
    color: var(--text-dim);
    text-align: left;
    white-space: nowrap;
  }
  .collapsed nav button {
    width: 42px;
    height: 42px;
    padding: 0;
    justify-content: center;
  }
  nav button:hover {
    background: var(--hover);
    color: var(--text);
  }
  nav button.active {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .section-title {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0 8px 6px 10px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-dim);
  }
  .icon-btn {
    width: 26px;
    height: 26px;
    flex-shrink: 0;
    border-radius: 6px;
    display: grid;
    place-items: center;
    color: var(--text-dim);
  }
  .icon-btn:hover {
    background: var(--hover);
    color: var(--text);
  }
  .toggle,
  .add {
    width: 32px;
    height: 32px;
  }
  .folder {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 8px 6px 10px;
    border-radius: 8px;
  }
  .folder:hover {
    background: var(--surface-2);
  }
  .folder span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .remove {
    opacity: 0;
  }
  .folder:hover .remove {
    opacity: 1;
  }
  .hint {
    margin: 0;
    padding: 0 10px;
    color: var(--text-dim);
  }
</style>
