<script>
  /** Wavely-Logo: eine Welle, die ein „W“ bildet (gleiches Motiv wie das App-Symbol, app-icon.svg) */
  let { size = 30 } = $props();
  const id = `logo-${Math.random().toString(36).slice(2, 8)}`;
</script>

<svg width={size} height={size} viewBox="2 2 60 60" aria-hidden="true">
  <defs>
    <linearGradient id={id} x1="0" y1="0" x2="1" y2="1">
      <stop offset="0" stop-color="#ff4f7b" />
      <stop offset="1" stop-color="#8b5cf6" />
    </linearGradient>
  </defs>
  <rect x="2" y="2" width="60" height="60" rx="17" fill="url(#{id})" />
  <path
    d="M13 23 C16 33 18 42 22.5 42 C27 42 28 24 32 24 C36 24 37 42 41.5 42 C46 42 48 33 51 23"
    fill="none"
    stroke="#fff"
    stroke-width="6"
    stroke-linecap="round"
    stroke-linejoin="round"
  />
</svg>

<style>
  svg {
    flex-shrink: 0;
    display: block;
  }
</style>
