<script>
  import Popover from "./Popover.svelte";
  import Icon from "./Icon.svelte";
  import {
    sleep,
    SLEEP_OPTIONS,
    startSleepTimer,
    sleepAtEndOfTrack,
    stopSleepTimer,
    formatRemaining,
  } from "../lib/sleepTimer.svelte.js";

  const active = $derived(Boolean(sleep.endsAt || sleep.endOfTrack));
  const badge = $derived(sleep.endsAt ? formatRemaining(sleep.remaining) : sleep.endOfTrack ? "Songende" : null);
  const label = (m) => (m >= 60 ? `${m / 60} ${m === 60 ? "Stunde" : "Stunden"}` : `${m} Minuten`);
</script>

<Popover icon="bedtime" title="Schlaf-Timer" {active} {badge} width={240}>
  {#snippet children({ close })}
    <h4>Schlaf-Timer</h4>
    {#if active}
      <p class="hint">
        {sleep.endsAt ? `Musik stoppt in ${formatRemaining(sleep.remaining)}` : "Musik stoppt am Ende des Songs"}
      </p>
    {/if}
    {#each SLEEP_OPTIONS as minutes (minutes)}
      <button class="item" onclick={() => (startSleepTimer(minutes), close())}>{label(minutes)}</button>
    {/each}
    <button class="item" class:active={sleep.endOfTrack} onclick={() => (sleepAtEndOfTrack(), close())}>
      Am Ende des Songs
    </button>
    {#if active}
      <hr />
      <button class="item" onclick={() => (stopSleepTimer(), close())}><Icon name="close" size={18} /> Timer aus</button>
    {/if}
  {/snippet}
</Popover>
