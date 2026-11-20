import { icon, invoke, inTauri, review, size, esc, confirmDialog, toast, base, cached } from './core.js';
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

// ---- review tray ----
const trayBar = document.getElementById('tray-bar');
function renderTray() {
  const n = review.items.size;
  trayBar.innerHTML = n ? `<button class="btn accent" id="open-tray">${icon('tray', 16)} Review ${n} item${n > 1 ? 's' : ''} · ${size(review.total())}</button>` : '';
}
review.listeners.add(renderTray);
trayBar.addEventListener('click', (e) => { if (e.target.closest('#open-tray')) openDrawer(); });

function openDrawer() {
  const back = document.createElement('div');
  back.className = 'drawer-back';
  const d = document.createElement('aside');
  d.className = 'drawer';
  const draw = () => {
    const rows = [...review.items.entries()].sort((a, b) => (b[1].size || 0) - (a[1].size || 0));
    d.innerHTML = `<header><h2>Review</h2><div class="caption">Items you marked. Moving them to the Recycle Bin can be undone from here or from the Recycle Bin itself.</div></header>
      <div class="body"><table class="tbl"><tbody>${rows.map(([p, v]) => `<tr><td><div class="name">${esc(v.label)}</div><div class="path">${esc(p)}</div></td>
        <td class="num">${size(v.size)}</td><td class="act"><button class="btn subtle small keep" data-rm="${esc(p)}" title="Keep">${icon('close', 14)}</button></td></tr>`).join('')}</tbody></table></div>
      <footer><span class="grow" style="flex:1">${rows.length} items · <b>${size(review.total())}</b></span>
        <button class="btn" data-clear>Clear list</button><button class="btn accent" data-go ${rows.length ? '' : 'disabled'}>Move to Recycle Bin</button></footer>`;
  };
  draw();
  const close = () => { back.remove(); d.remove(); review.listeners.delete(draw); };
  review.listeners.add(draw);
  back.onclick = close;
  d.addEventListener('click', async (e) => {
    const rm = e.target.closest('[data-rm]');
    if (rm) { review.remove(rm.dataset.rm); return; }
    if (e.target.closest('[data-clear]')) { review.clear(); close(); return; }
    if (e.target.closest('[data-go]')) {
      const paths = [...review.items.keys()];
      const total = review.total();
      const ok = await confirmDialog({
        title: `Move ${paths.length} item${paths.length > 1 ? 's' : ''} to the Recycle Bin?`,
        body: `${size(total)} will move to the Recycle Bin. Nothing is permanently deleted; you can restore everything with Undo.`,
        ok: 'Move to Recycle Bin',
      });
      if (!ok) return;
      const r = await invoke('recycle', { paths }).catch((err) => ({ recycled: [], failed: [{ path: '', error: String(err) }] }));
      for (const p of r.recycled) review.remove(p);
      close();
      window.dispatchEvent(new CustomEvent('soundings:recycled', { detail: r.recycled }));
      const msg = `Moved ${r.recycled.length} item${r.recycled.length === 1 ? '' : 's'} to the Recycle Bin` +
        (r.failed.length ? ` · ${r.failed.length} skipped (${esc(r.failed[0].error)})` : '');
      toast(msg, r.failed.length ? 'caution' : 'success', r.recycled.length ? {
        label: 'Undo',
        run: async () => {
          const u = await invoke('undo_recycle', { paths: r.recycled }).catch(() => ({ restored: 0 }));
          toast(`Restored ${u.restored} item${u.restored === 1 ? '' : 's'}`);
          window.dispatchEvent(new CustomEvent('soundings:restored'));
        },
      } : null);
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
