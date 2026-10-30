<script>
  import { feedback, KINDS, sendFeedback } from "../lib/feedback.svelte.js";
  import { account } from "../lib/account.svelte.js";
  import { showToast } from "../lib/ui.svelte.js";

  let kind = $state("bug");
  let text = $state("");
  let contact = $state("");
  let withInfo = $state(true);
  let busy = $state(false);
  let error = $state(null);
  let input = $state();

  $effect(() => {
    if (!feedback.open) return;
    error = null;
    contact = account.user?.email ?? "";
    queueMicrotask(() => input?.focus());
  });

  const placeholder = $derived(
    {
      bug: "Was ist passiert? Was hast du gerade gemacht, und was hättest du erwartet?",
      idea: "Was würdest du dir in Wavely wünschen?",
      other: "Was möchtest du uns sagen?",
    }[kind],
  );

  async function submit(event) {
    event.preventDefault();
    if (!text.trim() || busy) return;
    busy = true;
    error = null;
    try {
      const result = await sendFeedback({ kind, text, contact, withInfo });
      showToast(result === "sent" ? "Danke für dein Feedback!" : "Danke! Wird gesendet, sobald du online bist.");
      text = "";
      feedback.open = false;
    } catch (e) {
      error = e?.code === "permission-denied" ? "Feedback ist gerade nicht möglich (Firebase-Regeln)." : String(e?.message ?? e);
    } finally {
      busy = false;
    }
  }

  const close = () => !busy && (feedback.open = false);
</script>

{#if feedback.open}
  <div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && close()}>
    <form class="dialog" onsubmit={submit} aria-label="Feedback">
      <h2>Feedback</h2>
      <div class="kinds">
        {#each KINDS as k (k.id)}
          <button type="button" class:on={kind === k.id} onclick={() => (kind = k.id)}>{k.label}</button>
        {/each}
      </div>
      <textarea bind:this={input} bind:value={text} {placeholder} maxlength="5000" rows="7"></textarea>
      <label>
        Kontakt für Rückfragen (freiwillig)
        <input bind:value={contact} placeholder="E-Mail oder Discord-Name" maxlength="200" />
      </label>
      <label class="check">
        <input type="checkbox" bind:checked={withInfo} />
        Wavely-Version und Windows-Version mitschicken (hilft bei Fehlern)
      </label>
      {#if error}<p class="error">{error}</p>{/if}
      <div class="buttons">
        <button type="button" class="secondary" onclick={close}>Abbrechen</button>
        <button type="submit" class="primary" disabled={!text.trim() || busy}>{busy ? "Wird gesendet…" : "Senden"}</button>
      </div>
    </form>
  </div>
{/if}

<svelte:window onkeydown={(e) => e.key === "Escape" && feedback.open && close()} />

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
    width: min(500px, calc(100vw - 32px));
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6);
  }
  h2 {
    margin: 0;
  }
  .kinds {
    display: flex;
    gap: 6px;
  }
  .kinds button {
    flex: 1;
    height: 34px;
    border-radius: 17px;
    border: 1px solid var(--border);
    color: var(--text-dim);
    font-size: 13px;
  }
  .kinds button.on {
    border-color: var(--accent);
    color: var(--accent);
  }
  textarea,
  input:not([type="checkbox"]) {
    padding: 10px 12px;
    border-radius: 8px;
    border: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
    font-size: 14px;
    outline: none;
    resize: vertical;
  }
  textarea:focus,
  input:focus {
    border-color: var(--accent);
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
  }
  label.check input {
    accent-color: var(--accent);
  }
  .error {
    margin: 0;
    color: var(--accent);
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
