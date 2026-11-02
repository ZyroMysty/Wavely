<script>
  import { invoke } from "@tauri-apps/api/core";
  import { getVersion } from "@tauri-apps/api/app";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { ui, showToast } from "../lib/ui.svelte.js";
  import { player } from "../lib/player.svelte.js";
  import { library } from "../lib/library.svelte.js";
  import { coverCacheSize } from "../lib/covers.js";
  import Icon from "./Icon.svelte";

  /** Abfrage alle 2 Sekunden, Verlauf der letzten 5 Minuten */
  const INTERVAL = 2000;
  const HISTORY = 150;

  let snapshot = $state(null);
  let cpu = $state({});
  let history = $state([]);
  let folders = $state(null);
  let page = $state(null);
  let version = $state("");
  let error = $state(null);
  let previous = null;

  const close = () => (ui.diagnostics = false);
  const mb = (bytes) => (bytes / 1048576).toFixed(1);

  async function sample() {
    try {
      const next = await invoke("diagnostics_snapshot");
      const now = performance.now();
      // CPU-Last aus der verbrauchten CPU-Zeit seit der letzten Abfrage (100 % = ein Kern voll)
      if (previous) {
        const elapsed = now - previous.at;
        const result = {};
        for (const p of next.processes) {
          const before = previous.processes.find((q) => q.pid === p.pid);
          if (before) result[p.pid] = Math.max(0, ((p.cpuMs - before.cpuMs) / elapsed) * 100);
        }
        cpu = result;
      }
      previous = { at: now, processes: next.processes };
      snapshot = next;
      history = [...history, next.processes.reduce((sum, p) => sum + p.privateBytes, 0)].slice(-HISTORY);
      page = pageInfo();
      error = null;
    } catch (e) {
      error = String(e);
    }
  }

  /** Werte aus der Oberfläche selbst */
  function pageInfo() {
    const memory = performance.memory;
    let storage = 0;
    try {
      for (let i = 0; i < localStorage.length; i++) {
        const key = localStorage.key(i);
        storage += (key.length + (localStorage.getItem(key)?.length ?? 0)) * 2;
      }
    } catch {}
    return {
      heapUsed: memory?.usedJSHeapSize ?? 0,
      heapTotal: memory?.totalJSHeapSize ?? 0,
      heapLimit: memory?.jsHeapSizeLimit ?? 0,
      domNodes: document.getElementsByTagName("*").length,
      images: document.images.length,
      covers: coverCacheSize(),
      storage,
      queue: player.queue.length,
      library: library.tracks.length,
    };
  }

  $effect(() => {
    if (!ui.diagnostics) return;
    history = [];
    previous = null;
    folders = null;
    sample();
    const timer = setInterval(sample, INTERVAL);
    getVersion().then((v) => (version = v)).catch(() => {});
    invoke("diagnostics_folders").then((f) => (folders = f)).catch(() => {});
    return () => clearInterval(timer);
  });

  const total = $derived(
    snapshot
      ? {
          privateBytes: snapshot.processes.reduce((s, p) => s + p.privateBytes, 0),
          workingSet: snapshot.processes.reduce((s, p) => s + p.workingSet, 0),
          cpu: Object.values(cpu).reduce((s, v) => s + v, 0),
        }
      : null,
  );

  const uptime = $derived.by(() => {
    if (!snapshot?.startedMs) return "–";
    const minutes = Math.floor((Date.now() - snapshot.startedMs) / 60000);
    return minutes >= 60 ? `${Math.floor(minutes / 60)} h ${minutes % 60} min` : `${minutes} min`;
  });

  // Verlauf als Linie (privater Speicher gesamt)
  const chart = $derived.by(() => {
    if (history.length < 2) return null;
    const min = Math.min(...history);
    const max = Math.max(...history);
    const span = Math.max(max - min, 1048576);
    const points = history
      .map((v, i) => `${(i / (HISTORY - 1)) * 100},${36 - ((v - min) / span) * 32}`)
      .join(" ");
    return { points, min, max };
  });

  function report() {
    const lines = [
      `Wavely ${version} – Diagnose ${new Date().toLocaleString("de-DE")}`,
      `WebView2 ${snapshot.webviewVersion ?? "?"}, Hardwarebeschleunigung ${snapshot.hardwareAcceleration ? "an" : "aus"}, ${snapshot.cores} Kerne, läuft seit ${uptime}`,
      `Browser-Argumente: ${snapshot.browserArgs}`,
      "",
      `Gesamt: privat ${mb(total.privateBytes)} MB, Working Set ${mb(total.workingSet)} MB, CPU ${total.cpu.toFixed(1)} %`,
      ...snapshot.processes.map(
        (p) =>
          `  ${p.kind.padEnd(28)} PID ${String(p.pid).padEnd(6)} privat ${mb(p.privateBytes).padStart(6)} MB  WS ${mb(p.workingSet).padStart(6)} MB  CPU ${(cpu[p.pid] ?? 0).toFixed(1).padStart(5)} %`,
      ),
      "",
      `Oberfläche: JS-Heap ${mb(page.heapUsed)}/${mb(page.heapTotal)} MB, ${page.domNodes} DOM-Knoten, ${page.images} Bilder, ${page.covers} Cover im Speicher, localStorage ${mb(page.storage)} MB`,
      `Warteschlange ${page.queue} Songs, Bibliothek ${page.library} Songs, Mini-Player ${ui.miniOpen ? "offen" : "zu"}`,
      ...(folders ?? []).map((f) => `${f.label}: ${mb(f.bytes)} MB in ${f.files} Dateien (${f.path})`),
    ];
    return lines.join("\n");
  }

  async function copy() {
    if (!snapshot) return;
    await writeText(report()).catch(() => {});
    showToast("Diagnose kopiert");
  }
</script>

{#if ui.diagnostics}
  <div class="backdrop" role="presentation" onmousedown={(e) => e.target === e.currentTarget && close()}>
    <div class="dialog" role="dialog" aria-label="Diagnose">
      <header>
        <Icon name="speed" size={26} />
        <div>
          <h2>Diagnose <span class="badge">Admin</span></h2>
          <small>Wird alle 2 Sekunden neu gemessen</small>
        </div>
      </header>

      {#if error}<p class="error">{error}</p>{/if}

      {#if snapshot && total}
        <div class="body">
          <div class="tiles">
            <div class="tile">
              <small>Arbeitsspeicher (privat)</small>
              <strong>{mb(total.privateBytes)} MB</strong>
            </div>
            <div class="tile">
              <small>Working Set</small>
              <strong>{mb(total.workingSet)} MB</strong>
            </div>
            <div class="tile">
              <small>CPU (100 % = ein Kern)</small>
              <strong>{total.cpu.toFixed(1)} %</strong>
            </div>
            <div class="tile">
              <small>Prozesse</small>
              <strong>{snapshot.processes.length}</strong>
            </div>
          </div>

          {#if chart}
            <div class="chart">
              <svg viewBox="0 0 100 40" preserveAspectRatio="none" aria-label="Verlauf Arbeitsspeicher">
                <polyline points={chart.points} />
              </svg>
              <small>{mb(chart.min)}–{mb(chart.max)} MB in den letzten {Math.round((history.length * INTERVAL) / 60000) || "<1"} min</small>
            </div>
          {/if}

          <table>
            <thead>
              <tr><th>Prozess</th><th>PID</th><th class="num">Privat</th><th class="num">Working Set</th><th class="num">CPU</th></tr>
            </thead>
            <tbody>
              {#each snapshot.processes as p (p.pid)}
                <tr>
                  <td title={p.name}>{p.kind}</td>
                  <td class="dim">{p.pid}</td>
                  <td class="num">{mb(p.privateBytes)} MB</td>
                  <td class="num">{mb(p.workingSet)} MB</td>
                  <td class="num">{(cpu[p.pid] ?? 0).toFixed(1)} %</td>
                </tr>
              {/each}
            </tbody>
          </table>

          <dl>
            <dt>Version</dt><dd>{version}</dd>
            <dt>WebView2</dt><dd>{snapshot.webviewVersion ?? "–"}</dd>
            <dt>Hardwarebeschleunigung</dt><dd>{snapshot.hardwareAcceleration ? "an" : "aus"}</dd>
            <dt>Läuft seit</dt><dd>{uptime}</dd>
            <dt>Prozessorkerne</dt><dd>{snapshot.cores}</dd>
            <dt>Browser-Argumente</dt><dd class="mono">{snapshot.browserArgs}</dd>
            {#if page}
              <dt>JS-Speicher</dt><dd>{mb(page.heapUsed)} von {mb(page.heapTotal)} MB belegt</dd>
              <dt>Oberfläche</dt><dd>{page.domNodes} Elemente, {page.images} Bilder</dd>
              <dt>Cover im Speicher</dt><dd>{page.covers} (lokale Dateien)</dd>
              <dt>Gespeicherte Einstellungen</dt><dd>{mb(page.storage)} MB</dd>
              <dt>Warteschlange / Bibliothek</dt><dd>{page.queue} / {page.library} Songs</dd>
              <dt>Mini-Player</dt><dd>{ui.miniOpen ? "offen" : "geschlossen"}</dd>
            {/if}
            {#each folders ?? [] as f (f.label)}
              <dt>{f.label}</dt><dd title={f.path}>{mb(f.bytes)} MB, {f.files} Dateien</dd>
            {:else}
              <dt>Ordner</dt><dd class="dim">werden gezählt …</dd>
            {/each}
          </dl>
        </div>
      {:else if !error}
        <p class="dim">Misst …</p>
      {/if}

      <div class="buttons">
        <button class="secondary" onclick={copy} disabled={!snapshot}>Als Text kopieren</button>
        <button class="primary" onclick={close}>Schließen</button>
      </div>
    </div>
  </div>
{/if}

<svelte:window onkeydown={(e) => e.key === "Escape" && ui.diagnostics && close()} />

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
    width: min(720px, calc(100vw - 32px));
    max-height: calc(100vh - 48px);
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
    color: var(--accent);
  }
  h2 {
    margin: 0;
    font-size: 20px;
    color: var(--text);
  }
  header small,
  .dim,
  .chart small {
    color: var(--text-dim);
    font-size: 13px;
  }
  .badge {
    margin-left: 6px;
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--accent);
    color: #fff;
    font-size: 11px;
    font-weight: 700;
    vertical-align: 3px;
  }
  .error {
    color: var(--accent);
    margin: 0;
  }
  .body {
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding-right: 4px;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 10px;
  }
  .tile {
    background: var(--surface-2);
    border-radius: 10px;
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .tile small {
    color: var(--text-dim);
    font-size: 12px;
  }
  .tile strong {
    font-size: 20px;
    font-variant-numeric: tabular-nums;
  }
  .chart {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .chart svg {
    width: 100%;
    height: 60px;
    background: var(--surface-2);
    border-radius: 8px;
  }
  polyline {
    fill: none;
    stroke: var(--accent);
    stroke-width: 1.5;
    vector-effect: non-scaling-stroke;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  th {
    text-align: left;
    color: var(--text-dim);
    font-weight: 600;
    font-size: 12px;
    padding: 0 8px 6px 0;
    border-bottom: 1px solid var(--border);
  }
  td {
    padding: 5px 8px 5px 0;
    border-bottom: 1px solid var(--border);
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  dl {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 16px;
    margin: 0;
    font-size: 13px;
  }
  dt {
    color: var(--text-dim);
  }
  dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .mono {
    font-family: Consolas, monospace;
    font-size: 12px;
  }
  .buttons {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }
</style>
