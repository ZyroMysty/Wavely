<script>
  import { library, addFolder, removeFolder } from "../lib/library.svelte.js";
  import Icon from "./Icon.svelte";

  let { view = $bindable() } = $props();

  const folderName = (path) => path.split(/[\\/]/).filter(Boolean).pop() ?? path;
</script>

<aside>
  <div class="brand">
    <span class="logo"><Icon name="music" size={18} /></span>
    Wavely
  </div>

  <nav>
    <button class:active={view === "local"} onclick={() => (view = "local")}>
      <Icon name="folder" size={18} /> Lokale Musik
    </button>
    <button class:active={view === "youtube"} onclick={() => (view = "youtube")}>
      <Icon name="youtube" size={18} /> YouTube Music
    </button>
  </nav>

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
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 18px;
    font-weight: 700;
    padding: 0 8px;
  }
  .logo {
    width: 30px;
    height: 30px;
    border-radius: 8px;
    background: var(--accent);
    color: #fff;
    display: grid;
    place-items: center;
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
    border-radius: 6px;
    display: grid;
    place-items: center;
    color: var(--text-dim);
  }
  .icon-btn:hover {
    background: var(--hover);
    color: var(--text);
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
