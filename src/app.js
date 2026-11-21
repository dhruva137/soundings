import { icon, invoke, inTauri, review, size, esc, toast, cached } from './core.js';
import overview from './views/overview.js';
import storage from './views/storage.js';
import apps from './views/apps.js';
import developer from './views/developer.js';
import games from './views/games.js';
import stability from './views/stability.js';
import settings from './views/settings.js';

const VIEWS = [
  ['overview', 'Overview', overview],
  ['storage', 'Storage', storage],
  ['apps', 'Apps', apps],
  ['developer', 'Developer', developer],
  ['games', 'Games', games],
  ['stability', 'Stability', stability],
];
const nav = document.getElementById('nav');
const page = document.getElementById('page');
const content = document.getElementById('content');
let current = null;
let cleanup = null;

function navHtml(active) {
  const item = (k, label) => `<div class="nav-item ${k === active ? 'on' : ''}" data-go="${k}" role="link" tabindex="0" title="${label}">${icon(k)}<span>${label}</span></div>`;
  return `<div class="pane-top"><button class="icon-btn" id="collapse" title="Collapse navigation">${icon('menu', 18)}</button><span class="brand">Soundings</span></div>
    ${VIEWS.map(([k, l]) => item(k, l)).join('')}
    <div class="nav-spacer"></div>
    ${item('settings', 'Settings')}`;
}

export function go(key, opts = {}) {
  const v = key === 'settings' ? settings : (VIEWS.find((x) => x[0] === key) || VIEWS[0])[2];
  current = key;
  nav.innerHTML = navHtml(key);
  if (typeof cleanup === 'function') { try { cleanup(); } catch {} }
  page.innerHTML = '';
  content.scrollTop = 0;
  const el = document.createElement('div');
  el.className = 'page';
  page.appendChild(el);
  try { localStorage.setItem('soundings.view', key); } catch {}
  cleanup = v(el, { go, ...opts });
}
window.soundingsGo = go;

nav.addEventListener('click', (e) => {
  if (e.target.closest('#collapse')) {
    document.getElementById('shell').classList.toggle('compact');
    return;
  }
  const t = e.target.closest('[data-go]');
  if (t) go(t.dataset.go);
});
nav.addEventListener('keydown', (e) => {
  const t = e.target.closest('[data-go]');
  if (t && (e.key === 'Enter' || e.key === ' ')) { e.preventDefault(); go(t.dataset.go); }
});

// ---- shortlist: things you want to look at. Soundings never deletes; you do that in File Explorer. ----
const trayBar = document.getElementById('tray-bar');
function renderTray() {
  const n = review.items.size;
  trayBar.innerHTML = n ? `<button class="btn accent" id="open-tray">${icon('tray', 16)} Shortlist · ${n} item${n > 1 ? 's' : ''} · ${size(review.total())}</button>` : '';
}
review.listeners.add(renderTray);
trayBar.addEventListener('click', (e) => { if (e.target.closest('#open-tray')) openDrawer(); });

// Drop items you already removed in File Explorer, and say how much space that freed.
async function sweepShortlist(quiet) {
  const paths = [...review.items.keys()];
  if (!paths.length) return;
  const gone = await invoke('missing_paths', { paths }).catch(() => []);
  if (!gone.length) { if (!quiet) toast('Everything on your shortlist is still there.'); return; }
  let freed = 0;
  for (const p of gone) { freed += review.items.get(p)?.size || 0; review.items.delete(p); }
  review.emit();
  toast(`Nice. ${gone.length} item${gone.length > 1 ? 's are' : ' is'} gone, freeing about <b>${size(freed)}</b>.`);
  window.dispatchEvent(new CustomEvent('soundings:changed'));
}
window.addEventListener('focus', () => sweepShortlist(true)); // coming back from File Explorer

function openDrawer() {
  const back = document.createElement('div');
  back.className = 'drawer-back';
  const d = document.createElement('aside');
  d.className = 'drawer';
  const draw = () => {
    const rows = [...review.items.entries()].sort((a, b) => (b[1].size || 0) - (a[1].size || 0));
    d.innerHTML = `<header><h2>Shortlist</h2><div class="caption">Soundings never deletes anything. Open an item in File Explorer and delete it there if you're sure; it goes to the Recycle Bin like always. Items you remove drop off this list by themselves.</div></header>
      <div class="body"><table class="tbl"><tbody>${rows.map(([p, v]) => `<tr><td><div class="name">${esc(v.label)}</div><div class="path">${esc(p)}</div></td>
        <td class="num">${size(v.size)}</td><td class="act"><button class="btn small" data-show="${esc(p)}" title="Show in File Explorer">${icon('open', 14)} Show</button>
        <button class="btn subtle small" data-rm="${esc(p)}" title="Take off the list">${icon('close', 14)}</button></td></tr>`).join('') ||
        '<tr><td class="muted">Your shortlist is empty.</td></tr>'}</tbody></table></div>
      <footer><span class="grow" style="flex:1">${rows.length} items · <b>${size(review.total())}</b></span>
        <button class="btn" data-copy ${rows.length ? '' : 'disabled'}>Copy list</button><button class="btn" data-check ${rows.length ? '' : 'disabled'}>Check again</button><button class="btn" data-clear>Clear list</button></footer>`;
  };
  draw();
  const close = () => { back.remove(); d.remove(); review.listeners.delete(draw); };
  review.listeners.add(draw);
  back.onclick = close;
  d.addEventListener('click', async (e) => {
    const show = e.target.closest('[data-show]');
    if (show) { invoke('reveal', { path: show.dataset.show }).catch((err) => toast(esc(String(err)), 'caution')); return; }
    const rm = e.target.closest('[data-rm]');
    if (rm) { review.remove(rm.dataset.rm); return; }
    if (e.target.closest('[data-clear]')) { review.clear(); close(); return; }
    if (e.target.closest('[data-check]')) { sweepShortlist(false); return; }
    if (e.target.closest('[data-copy]')) {
      try { await navigator.clipboard.writeText([...review.items.keys()].join('\r\n')); toast('Copied the list of paths.'); }
      catch { toast('Could not copy to the clipboard.', 'caution'); }
    }
  });
  document.body.append(back, d);
}

// ---- theme: follow Windows accent ----
async function initTheme() {
  try {
    const o = await cached('overview', () => invoke('overview'));
    if (inTauri && o.theme?.mica) document.body.classList.add('mica');
    if (o.theme?.accent) {
      const a = o.theme.accent;
      document.documentElement.style.setProperty('--accent', a);
      const [r, g, b] = [1, 3, 5].map((i) => parseInt(a.slice(i, i + 2), 16));
      const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
      document.documentElement.style.setProperty('--accent-text', lum > 0.6 ? '#000' : '#fff');
    }
  } catch {}
}

initTheme();
renderTray();
let start = 'overview';
try { start = localStorage.getItem('soundings.view') || 'overview'; } catch {}
go(start);
