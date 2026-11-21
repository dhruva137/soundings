// Shared helpers: backend calls, formatting, categories, review tray, dialogs.
import { icon } from './icons.js';

const T = window.__TAURI__;
export const inTauri = !!T;
// Browser preview: index.html?mock reads snapshots written by `soundings.exe --dump mock`.
const MOCK = !T && new URLSearchParams(location.search).has('mock');
async function mock(cmd, args) {
  const r = await fetch(`../mock/${cmd}.json`);
  if (!r.ok) throw new Error(`No mock for ${cmd}`);
  const v = await r.json();
  if (cmd === 'tree') return v[String(args?.id ?? 0)] || v['0'];
  if (cmd === 'stability_history') { const cut = Date.now() - (args?.days || 365) * 864e5; return v.filter((e) => new Date(e.time) >= cut); }
  return v;
}
export function invoke(cmd, args) {
  if (MOCK) return mock(cmd, args);
  if (!T) return Promise.reject(new Error('Open this inside the Soundings app.'));
  return T.core.invoke(cmd, args);
}
export function listen(ev, cb) {
  if (!T) return Promise.resolve(() => {});
  return T.event.listen(ev, (e) => cb(e.payload));
}

// ---- memo cache for slow reads (cleared by Refresh) ----
const memo = new Map();
export function cached(key, fn) {
  if (!memo.has(key)) memo.set(key, fn(wantFresh(key)).catch((e) => { memo.delete(key); throw e; }));
  return memo.get(key);
}
// After Refresh, backend reads for that prefix bypass Soundings' disk cache (once per key).
const freshKeys = new Map(); // prefix -> Set of keys already refreshed
export function forget(prefix) {
  for (const k of [...memo.keys()]) if (k.startsWith(prefix)) memo.delete(k);
  freshKeys.set(prefix, new Set());
  setTimeout(() => freshKeys.delete(prefix), 120000);
}
export function wantFresh(key) {
  for (const [p, used] of freshKeys) if (key.startsWith(p) && !used.has(key)) { used.add(key); return true; }
  return false;
}

// ---- formatting ----
const THIN = ' ';
export function size(b) {
  if (b == null) return '—';
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  let i = 0; let v = b;
  while (v >= 1024 && i < 4) { v /= 1024; i++; }
  const d = i === 0 ? 0 : v >= 100 ? 0 : v >= 10 ? 1 : 1;
  return `${v.toFixed(d)}${THIN}${u[i]}`;
}
export const int = (n) => (n ?? 0).toLocaleString();
export function date(unix) {
  if (!unix) return '—';
  return new Date(unix * 1000).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
}
export function dateTime(v) {
  const d = typeof v === 'number' ? new Date(v * 1000) : new Date(v);
  if (isNaN(d)) return '—';
  return d.toLocaleString(undefined, { day: 'numeric', month: 'short', year: 'numeric', hour: '2-digit', minute: '2-digit' });
}
export function ago(unix) {
  if (!unix) return '—';
  const s = Date.now() / 1000 - unix;
  const d = Math.floor(s / 86400);
  if (d < 1) return 'today';
  if (d < 2) return 'yesterday';
  if (d < 45) return `${d} days ago`;
  if (d < 600) return `${Math.round(d / 30)} months ago`;
  return `${(d / 365).toFixed(1)} years ago`;
}
export const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
export const base = (p) => String(p).split('\\').filter(Boolean).pop() || p;
export const parent = (p) => String(p).split('\\').slice(0, -1).join('\\');
export const pct = (a, b) => (b ? Math.max(0, Math.min(100, (a / b) * 100)) : 0);
export const bar = (v, max, cls = '') => `<div class="bar ${cls}"><i style="width:${pct(v, max).toFixed(1)}%"></i></div>`;

// ---- categories (disk map colours) ----
export const CATS = {
  Apps: '#5b8db8', Games: '#8a6fbf', Developer: '#3f9c8c', Media: '#c98b4a', Documents: '#6f9e5a', System: '#8c8c8c', Other: '#b77a8f',
};
const rules = [
  ['Games', /\\(steamapps|xboxgames|epic games|riot games|gog galaxy\\games|\.minecraft|roblox|games)(\\|$)/],
  ['Developer', /\\(node_modules|\.venv|venv|\.git|\.cargo|\.rustup|\.nuget|\.gradle|\.m2|miniconda3|anaconda3|\.conda|\.ollama|\.lmstudio|projects|github|npm-cache|pip|uv|docker|android|\.vscode|\.cursor|jetbrains|huggingface|programs\\python|nodejs|\.cache)(\\|$)/],
  ['System', /^[a-z]:\\(windows|programdata|\$recycle\.bin|recovery|system volume information|pagefile\.sys|hiberfil\.sys|swapfile\.sys|perflogs)(\\|$)/],
  ['Media', /\\(pictures|videos|music|onedrive\\pictures|camera roll|screenshots)(\\|$)|\.(mp4|mkv|mov|jpg|jpeg|png|heic|mp3|wav)$/],
  ['Documents', /\\(documents|desktop|downloads|onedrive)(\\|$)|^[a-z]:\\users(\\[^\\]+)?$/],
  ['Apps', /^[a-z]:\\program files( \(x86\))?(\\|$)|\\appdata(\\|$)/],
];
export function category(path) {
  const p = String(path).toLowerCase();
  for (const [c, re] of rules) if (re.test(p)) return c;
  return 'Other';
}

// ---- shortlist: everything the user marked, from any page (kept on this PC only) ----
const SHORTLIST_KEY = 'soundings.shortlist';
export const review = {
  items: new Map((() => { try { return JSON.parse(localStorage.getItem(SHORTLIST_KEY) || '[]'); } catch { return []; } })()), // path -> {size, label, kind}
  listeners: new Set(),
  add(path, sz, label, kind = 'file') { this.items.set(path, { size: sz, label: label || base(path), kind }); this.emit(); },
  remove(path) { this.items.delete(path); this.emit(); },
  toggle(path, sz, label, kind) { this.has(path) ? this.remove(path) : this.add(path, sz, label, kind); },
  has(path) { return this.items.has(path); },
  clear() { this.items.clear(); this.emit(); },
  total() { let t = 0; for (const v of this.items.values()) t += v.size || 0; return t; },
  emit() {
    try { localStorage.setItem(SHORTLIST_KEY, JSON.stringify([...this.items])); } catch {}
    for (const f of this.listeners) f();
  },
};

// ---- dialogs & toasts ----
export function confirmDialog({ title, body, ok = 'OK', cancel = 'Cancel' }) {
  return new Promise((resolve) => {
    const back = document.createElement('div');
    back.className = 'dialog-back';
    back.innerHTML = `<div class="dialog" role="dialog" aria-modal="true"><div class="b"><h2>${esc(title)}</h2><div class="muted">${body}</div></div>
      <div class="f"><button class="btn accent" data-ok>${esc(ok)}</button><button class="btn" data-no>${esc(cancel)}</button></div></div>`;
    const done = (v) => { back.remove(); resolve(v); };
    back.querySelector('[data-ok]').onclick = () => done(true);
    back.querySelector('[data-no]').onclick = () => done(false);
    back.onclick = (e) => { if (e.target === back) done(false); };
    document.body.appendChild(back);
    back.querySelector('[data-no]').focus();
  });
}
export function toast(html, kind = 'success', action) {
  const t = document.createElement('div');
  t.className = `toast infobar ${kind}`;
  t.innerHTML = `<div class="grow">${html}</div>${action ? `<button class="btn small">${esc(action.label)}</button>` : ''}`;
  if (action) t.querySelector('button').onclick = () => { action.run(); t.remove(); };
  document.body.appendChild(t);
  setTimeout(() => t.remove(), action ? 9000 : 4500);
}

export const loading = (msg = 'Reading…') => `<div class="loading"><div class="spinner"></div>${esc(msg)}</div>`;
export const errorBox = (e) => `<div class="infobar critical"><div class="grow">${esc(e?.message || e)}</div></div>`;
export const reveal = (p) => invoke('reveal', { path: p }).catch(() => {});

export function tabs(list, current) {
  return `<div class="tabs" role="tablist">${list.map(([k, label, n]) =>
    `<div class="tab ${k === current ? 'on' : ''}" role="tab" data-tab="${k}">${esc(label)}${n != null ? `<span class="count">${esc(n)}</span>` : ''}</div>`).join('')}</div>`;
}

export function sortable(rows, key, dir) {
  const s = [...rows].sort((a, b) => {
    const x = a[key]; const y = b[key];
    if (typeof x === 'number' || typeof y === 'number') return (x || 0) - (y || 0);
    return String(x || '').localeCompare(String(y || ''));
  });
  return dir < 0 ? s.reverse() : s;
}

export { icon };
