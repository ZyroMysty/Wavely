<script>
  import { changelog, releases, formatDate } from "../lib/changelog.svelte.js";
  import Logo from "./Logo.svelte";

  let showAll = $state(false);

  $effect(() => {
    if (changelog.open) showAll = !changelog.highlight;
  });

  const shown = $derived(showAll ? releases : releases.filter((r) => r.version === changelog.highlight));
  const close = () => (changelog.open = false);
</script>

{#if changelog.open}
  <div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && close()}>
    <div class="dialog" role="dialog" aria-label="Versionsverlauf">
      <header>
        <Logo size={36} />
        <div>
          <h2>{showAll ? "Versionsverlauf" : `Neu in Wavely ${changelog.highlight}`}</h2>
          {#if !showAll}<small>Wavely wurde aktualisiert</small>{/if}
        </div>
      </header>

      <div class="list">
        {#each shown as release, i (release.version)}
          <section class:latest={i === 0 && showAll}>
            {#if showAll}
              <h3>
                Version {release.version}
                {#if release.date}<small>{formatDate(release.date)}</small>{/if}
              </h3>
            {/if}
            <ul>
              {#each release.items as item, j (j)}
                <li>{item}</li>
              {/each}
            </ul>
          </section>
        {/each}
      </div>

      <div class="buttons">
        {#if !showAll}
          <button class="secondary" onclick={() => (showAll = true)}>Alle Versionen</button>
        {/if}
        <button class="primary" onclick={close}>{showAll ? "Schließen" : "Super!"}</button>
      </div>
    </div>
  </div>
{/if}

<svelte:window onkeydown={(e) => e.key === "Escape" && changelog.open && close()} />

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
    width: min(520px, calc(100vw - 32px));
    max-height: min(640px, calc(100vh - 48px));
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: 14px;
    padding: 24px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    box-shadow: 0 24px 60px rgba(0, 0, 0, 0.6);
  }
  header {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  h2 {
    margin: 0;
    font-size: 20px;
  }
  header small {
    color: var(--text-dim);
    font-size: 13px;
  }
  .list {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 18px;
    padding-right: 4px;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 15px;
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  h3 small {
    color: var(--text-dim);
    font-weight: 400;
    font-size: 12px;
  }
  section.latest h3 {
    color: var(--accent);
  }
  ul {
    margin: 0;
    padding-left: 20px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    line-height: 1.45;
    font-size: 14px;
  }
  li::marker {
    color: var(--accent);
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
