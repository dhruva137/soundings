// Line icons in the Fluent "Regular" spirit: 20px grid, 1.5px strokes, round joins.
const P = {
  overview: '<path d="M3.5 9.5 10 4l6.5 5.5"/><path d="M5 8.5V16h3.5v-4h3v4H15V8.5"/>',
  storage: '<circle cx="10" cy="10" r="6.5"/><path d="M10 3.5V10h6.5"/><path d="M10 10 5.4 14.6"/>',
  apps: '<rect x="3.5" y="3.5" width="5" height="5" rx="1"/><rect x="11.5" y="3.5" width="5" height="5" rx="1"/><rect x="3.5" y="11.5" width="5" height="5" rx="1"/><rect x="11.5" y="11.5" width="5" height="5" rx="1"/>',
  developer: '<path d="m7 6-4 4 4 4"/><path d="m13 6 4 4-4 4"/><path d="m11 4.5-2 11"/>',
  games: '<path d="M6.5 6.5h7a3.5 3.5 0 0 1 3.4 4.3l-.6 2.6a1.9 1.9 0 0 1-3.2.9L11.6 13H8.4l-1.5 1.3a1.9 1.9 0 0 1-3.2-.9l-.6-2.6a3.5 3.5 0 0 1 3.4-4.3Z"/><path d="M7 8.8v2.4M5.8 10h2.4"/><circle cx="13" cy="9.3" r=".4" fill="currentColor"/><circle cx="14.2" cy="10.7" r=".4" fill="currentColor"/>',
  stability: '<path d="M2.5 10.5h3l2-5 3 9 2-5.5 1.2 1.5h3.8"/>',
  settings: '<circle cx="10" cy="10" r="2.5"/><path d="M10 2.8v1.7M10 15.5v1.7M17.2 10h-1.7M4.5 10H2.8M15.1 4.9l-1.2 1.2M6.1 13.9l-1.2 1.2M15.1 15.1l-1.2-1.2M6.1 6.1 4.9 4.9"/><circle cx="10" cy="10" r="5.2"/>',
  menu: '<path d="M3.5 6h13M3.5 10h13M3.5 14h13"/>',
  refresh: '<path d="M15.5 9.5a5.5 5.5 0 1 1-1.6-3.9"/><path d="M14.5 3v3h-3"/>',
  folder: '<path d="M3 6.5A1.5 1.5 0 0 1 4.5 5h3.2l1.6 1.6h6.2A1.5 1.5 0 0 1 17 8.1v6.4A1.5 1.5 0 0 1 15.5 16h-11A1.5 1.5 0 0 1 3 14.5Z"/>',
  file: '<path d="M5.5 3h6l3.5 3.5v9A1.5 1.5 0 0 1 13.5 17h-8A1.5 1.5 0 0 1 4 15.5v-11A1.5 1.5 0 0 1 5.5 3Z"/><path d="M11.5 3v3.5H15"/>',
  tray: '<path d="M3 11.5 5 4.5h10l2 7v3A1.5 1.5 0 0 1 15.5 16h-11A1.5 1.5 0 0 1 3 14.5Z"/><path d="M3 11.5h4l1 1.5h4l1-1.5h4"/>',
  open: '<path d="M8 4.5H5A1.5 1.5 0 0 0 3.5 6v9A1.5 1.5 0 0 0 5 16.5h9a1.5 1.5 0 0 0 1.5-1.5v-3"/><path d="M11 3.5h5.5V9M16.5 3.5 9 11"/>',
  close: '<path d="m5 5 10 10M15 5 5 15"/>',
  up: '<path d="M10 16V4M5 9l5-5 5 5"/>',
  shield: '<path d="M10 2.8 4 5v4.6c0 3.4 2.5 6.3 6 7.6 3.5-1.3 6-4.2 6-7.6V5Z"/>',
};
export function icon(name, size = 20) {
  return `<svg width="${size}" height="${size}" viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${P[name] || ''}</svg>`;
}
