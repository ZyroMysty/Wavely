<script>
  import { ui } from "../lib/ui.svelte.js";
</script>

{#if ui.toast}
  <div class="toast" class:error={ui.toast.error} role="status">{ui.toast.text}</div>
{/if}

<style>
  .toast {
    position: fixed;
    left: 50%;
    bottom: calc(var(--player-height) + 16px);
    transform: translateX(-50%);
    z-index: 300;
    max-width: min(520px, calc(100vw - 32px));
    padding: 10px 18px;
    border-radius: 999px;
    background: var(--text);
    color: var(--bg);
    font-weight: 600;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
  }
  .toast.error {
    background: var(--accent);
    color: #fff;
  }
</style>
