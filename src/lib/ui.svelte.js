/** Zustand der Oberfläche, der nicht zu einer bestimmten Ansicht gehört. */
export const ui = $state({
  /** Rechte Seitenleiste: null | "queue" | "lyrics" */
  panel: null,
  /** Kontextmenü für einen Song: { x, y, track } oder null */
  menu: null,
});

export function togglePanel(name) {
  ui.panel = ui.panel === name ? null : name;
}

export function openTrackMenu(event, track) {
  event.preventDefault();
  ui.menu = { x: event.clientX, y: event.clientY, track };
}
