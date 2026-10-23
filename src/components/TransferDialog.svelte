<script>
  import { transfer, TARGETS, targetAvailable, startTransfer, closeTransfer } from "../lib/transfer.svelte.js";
  import { openIn } from "../lib/ui.svelte.js";
  import Icon from "./Icon.svelte";

  let target = $state(null);
  let name = $state("");
  let isPublic = $state(false);

  // Beim Öffnen: Ziel vorschlagen (anderer Dienst, bei dem man angemeldet ist)
  $effect(() => {
    if (!transfer.from || transfer.status !== "idle") return;
    name = transfer.from.title;
    isPublic = false;
    target = TARGETS.find((t) => t.id !== transfer.from.source && targetAvailable(t.id))?.id ?? null;
  });

  const sourceName = (id) => ({ youtube: "YouTube Music", spotify: "Spotify", soundcloud: "SoundCloud", local: "Lokal" })[id] ?? id;
  const progress = $derived(transfer.total ? transfer.done / transfer.total : 0);

  function openResult() {
    const { target: t, id, title } = transfer.result;
    closeTransfer();
    openIn(t, { type: "collection", kind: "playlist", id, title });
  }
</script>

{#if transfer.from}
  <div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && closeTransfer()}>
    <div class="dialog" role="dialog" aria-label="Playlist übertragen">
      <h2>Playlist übertragen</h2>
      <p class="dim">
        „{transfer.from.title}“ · {transfer.from.tracks.length} {transfer.from.tracks.length === 1 ? "Song" : "Songs"} von {sourceName(transfer.from.source)}
      </p>

      {#if transfer.status === "idle"}
        <div class="targets">
          {#each TARGETS as t (t.id)}
            {@const same = t.id === transfer.from.source}
            {@const available = targetAvailable(t.id)}
            <button class="target" class:on={target === t.id} disabled={same || !available} onclick={() => (target = t.id)}>
              <Icon name={t.id} size={22} />
              <span>
                <strong>{t.name}</strong>
                <small>{same ? "Quelle" : available ? "Neue Playlist anlegen" : "Nicht angemeldet"}</small>
              </span>
            </button>
          {/each}
        </div>
        <label>
          Name der neuen Playlist
          <input bind:value={name} maxlength="150" />
        </label>
        <label class="check">
          <input type="checkbox" bind:checked={isPublic} />
          Öffentlich (sonst privat)
        </label>
        <p class="hint">
          Wavely sucht jeden Song beim Ziel anhand von Titel, Künstler und Länge. Bei Remixes oder seltenen Songs kann
          die Zuordnung danebenliegen.
        </p>
        <div class="buttons">
          <button class="secondary" onclick={closeTransfer}>Abbrechen</button>
          <button class="primary" disabled={!target || !name.trim()} onclick={() => startTransfer(target, name.trim(), isPublic)}>
            Übertragen
          </button>
        </div>
      {:else if transfer.status === "running"}
        <p>Songs werden gesucht… {transfer.done} / {transfer.total}</p>
        <div class="bar"><div style:width="{progress * 100}%"></div></div>
        {#if transfer.missing.length}<p class="dim small">{transfer.missing.length} bisher nicht gefunden</p>{/if}
      {:else if transfer.status === "done"}
        <p class="ok">
          <Icon name="check" size={20} />
          {transfer.result.count} Songs nach {TARGETS.find((t) => t.id === transfer.result.target)?.name} übertragen.
        </p>
        {#if transfer.missing.length}
          <p class="dim">Nicht gefunden ({transfer.missing.length}):</p>
          <ul class="missing">
            {#each transfer.missing as track, i (i)}
              <li>{track.title}<span> · {track.artist ?? ""}</span></li>
            {/each}
          </ul>
        {/if}
        <div class="buttons">
          <button class="secondary" onclick={closeTransfer}>Schließen</button>
          <button class="primary" onclick={openResult}>Playlist öffnen</button>
        </div>
      {:else}
        <p class="error">{transfer.error}</p>
        <div class="buttons">
          <button class="secondary" onclick={() => (transfer.from = null)}>Schließen</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<svelte:window onkeydown={(e) => e.key === "Escape" && transfer.from && closeTransfer()} />

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
    width: min(480px, calc(100vw - 32px));
    max-height: calc(100vh - 48px);
    overflow-y: auto;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6);
  }
  h2,
  p {
    margin: 0;
  }
  .dim {
    color: var(--text-dim);
    font-size: 14px;
  }
  .small {
    font-size: 12px;
  }
  .targets {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .target {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px;
    border-radius: 10px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    text-align: left;
  }
  .target span {
    display: flex;
    flex-direction: column;
  }
  .target small {
    color: var(--text-dim);
    font-size: 12px;
  }
  .target.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  .target:disabled {
    opacity: 0.45;
    cursor: default;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    color: var(--text-dim);
    font-size: 13px;
  }
  label.check {
    flex-direction: row;
    align-items: center;
    gap: 8px;
    color: var(--text);
  }
  label.check input {
    accent-color: var(--accent);
  }
  input:not([type="checkbox"]) {
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
  .hint {
    color: var(--text-dim);
    font-size: 12px;
    line-height: 1.45;
  }
  .bar {
    height: 6px;
    border-radius: 3px;
    background: var(--border);
    overflow: hidden;
  }
  .bar div {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s;
  }
  .ok {
    display: flex;
    align-items: center;
    gap: 8px;
    font-weight: 600;
  }
  .error {
    color: var(--accent);
    line-height: 1.45;
  }
  .missing {
    margin: 0;
    padding-left: 18px;
    max-height: 180px;
    overflow-y: auto;
    font-size: 13px;
    line-height: 1.6;
  }
  .missing span {
    color: var(--text-dim);
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
