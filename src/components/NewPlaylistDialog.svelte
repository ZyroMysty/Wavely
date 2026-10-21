<script>
  import { ui, showToast } from "../lib/ui.svelte.js";
  import { createPlaylist } from "../lib/ytm.svelte.js";

  let title = $state("");
  let privacy = $state("PRIVATE");
  let busy = $state(false);
  let input = $state();

  $effect(() => {
    if (ui.newPlaylist) {
      title = "";
      privacy = "PRIVATE";
      queueMicrotask(() => input?.focus());
    }
  });

  async function submit(event) {
    event.preventDefault();
    const name = title.trim();
    if (!name || busy) return;
    busy = true;
    const tracks = ui.newPlaylist.tracks;
    try {
      await createPlaylist(name, privacy, tracks);
      showToast(tracks.length ? `Playlist „${name}“ erstellt und Song hinzugefügt` : `Playlist „${name}“ erstellt`);
      ui.newPlaylist = null;
    } catch (e) {
      showToast(`Playlist konnte nicht erstellt werden: ${e}`, true);
    } finally {
      busy = false;
    }
  }
</script>

{#if ui.newPlaylist}
  <div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && (ui.newPlaylist = null)}>
    <form class="dialog" onsubmit={submit}>
      <h2>Neue Playlist</h2>
      <label>
        Name
        <input bind:this={input} bind:value={title} maxlength="150" placeholder="Meine Playlist" />
      </label>
      <label>
        Sichtbarkeit
        <select bind:value={privacy}>
          <option value="PRIVATE">Privat</option>
          <option value="UNLISTED">Nicht gelistet</option>
          <option value="PUBLIC">Öffentlich</option>
        </select>
      </label>
      {#if ui.newPlaylist.tracks.length}
        <p class="hint">„{ui.newPlaylist.tracks[0].title}“ wird gleich hinzugefügt.</p>
      {/if}
      <div class="buttons">
        <button type="button" class="secondary" onclick={() => (ui.newPlaylist = null)}>Abbrechen</button>
        <button type="submit" class="primary" disabled={!title.trim() || busy}>
          {busy ? "Wird erstellt…" : "Erstellen"}
        </button>
      </div>
    </form>
  </div>
{/if}

<svelte:window onkeydown={(e) => e.key === "Escape" && ui.newPlaylist && (ui.newPlaylist = null)} />

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
    background: rgba(0, 0, 0, 0.55);
    display: grid;
    place-items: center;
  }
  .dialog {
    width: min(420px, calc(100vw - 32px));
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6);
  }
  h2 {
    margin: 0;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    color: var(--text-dim);
    font-size: 13px;
  }
  input,
  select {
    height: 40px;
    padding: 0 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
    font-size: 14px;
    outline: none;
  }
  input:focus,
  select:focus {
    border-color: var(--accent);
  }
  .hint {
    margin: 0;
    color: var(--text-dim);
    font-size: 13px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
