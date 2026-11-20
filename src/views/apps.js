import { invoke, cached, forget, size, int, esc, date, ago, bar, loading, errorBox, tabs, reveal, icon, sortable } from '../core.js';

const S = { tab: 'installed', origin: 'all', q: '', sort: 'size', dir: -1, showHidden: false };
const ORIGINS = ['You installed', 'Came with PC', 'Manufacturer', 'Windows'];

export default function apps(el) {
  el.innerHTML = `<div class="page-head"><div><h1>Apps</h1><div class="sub">Everything installed, and where it came from.</div></div>
    <div class="actions"><button class="btn" id="ap-refresh">${icon('refresh', 16)} Refresh</button></div></div><div id="ap-tabs"></div><div id="ap-body">${loading()}</div>`;
  const body = el.querySelector('#ap-body');
  el.querySelector('#ap-refresh').onclick = () => { forget('apps'); forget('startup'); forget('usage'); render(); };

  async function render() {
    el.querySelector('#ap-tabs').innerHTML = tabs([['installed', 'Installed'], ['usage', 'Most used'], ['startup', 'Starts with Windows']], S.tab);
    el.querySelectorAll('.tab').forEach((t) => (t.onclick = () => { S.tab = t.dataset.tab; render(); }));
    body.innerHTML = loading();
    try { await ({ installed, usage, startup })[S.tab](); } catch (e) { body.innerHTML = errorBox(e); }
  }

  async function usage() {
    const list = (await cached('usage', () => invoke('app_usage'))).filter((x) => x.focusMs > 0 || x.runs > 0);
    const hours = (ms) => (ms >= 3600000 ? `${(ms / 3600000).toFixed(ms >= 36000000 ? 0 : 1)} h` : `${Math.max(1, Math.round(ms / 60000))} min`);
    const mx = list[0]?.focusMs || 1;
    const stale = list.filter((x) => x.lastRun && Date.now() / 1000 - x.lastRun > 180 * 86400).length;
    body.innerHTML = `<div class="caption" style="margin-bottom:8px">From Windows' own per-user counters: how often you opened each program and how long it was the active window. ${stale ? `${stale} haven't been opened in six months.` : ''}</div>
      <div class="card scroll-box" style="max-height:calc(100vh - 260px)"><table class="tbl"><thead><tr><th>Program</th><th class="num">Time in front</th><th style="width:140px"></th><th class="num">Opened</th><th class="num">Last opened</th></tr></thead><tbody>
      ${list.slice(0, 200).map((x) => `<tr><td><div class="name">${esc(x.name)}</div><div class="path">${esc(x.path)}</div></td>
        <td class="num">${x.focusMs ? hours(x.focusMs) : '—'}</td><td>${bar(x.focusMs, mx, 'accent')}</td>
        <td class="num muted">${int(x.runs)}×</td><td class="num muted">${x.lastRun ? ago(x.lastRun) : '—'}</td></tr>`).join('')}</tbody></table></div>`;
  }

  async function installed() {
    const data = await cached('apps', (fresh) => invoke('apps_list', { fresh }));
    const all = data.apps.filter((a) => S.showHidden || !a.hidden);
    const counts = Object.fromEntries(ORIGINS.map((o) => [o, all.filter((a) => a.origin === o).length]));
    let rows = all.filter((a) => (S.origin === 'all' || a.origin === S.origin) &&
      (!S.q || `${a.name} ${a.publisher}`.toLowerCase().includes(S.q.toLowerCase())));
    rows = sortable(rows, S.sort, S.dir);
    const total = rows.reduce((t, a) => t + (a.size || 0), 0);
    const th = (k, label, cls = '') => `<th class="sort ${cls}" data-sort="${k}">${label}${S.sort === k ? (S.dir < 0 ? ' ↓' : ' ↑') : ''}</th>`;
    body.innerHTML = `<div class="row wrap" style="margin-bottom:14px;gap:12px">
        <div class="chips"><button class="chip ${S.origin === 'all' ? 'on' : ''}" data-o="all">All <span class="n">${all.length}</span></button>
        ${ORIGINS.map((o) => `<button class="chip ${S.origin === o ? 'on' : ''}" data-o="${o}">${o} <span class="n">${counts[o]}</span></button>`).join('')}</div>
        <input type="search" id="ap-q" placeholder="Search apps" value="${esc(S.q)}" style="margin-left:auto">
        <label class="row caption"><input type="checkbox" id="ap-hidden" ${S.showHidden ? 'checked' : ''}> Show hidden components</label></div>
      <div class="caption" style="margin-bottom:8px">${int(rows.length)} apps · ${size(total)} reported by installers. Windows was set up on ${date(data.osInstall)}.</div>
      <div class="card scroll-box" style="max-height:calc(100vh - 300px)"><table class="tbl"><thead><tr>${th('name', 'Name')}${th('origin', 'Origin')}${th('installed', 'Installed', 'num')}${th('size', 'Size', 'num')}<th></th></tr></thead><tbody>
      ${rows.map((a) => `<tr><td><div class="name">${esc(a.name)}</div><div class="path">${esc([a.publisher, a.version].filter(Boolean).join(' · '))}</div></td>
        <td><span class="pill ${a.origin === 'You installed' ? 'accent' : ''}">${esc(a.origin)}</span><div class="caption" style="margin-top:2px">${esc(a.why)}</div></td>
        <td class="num muted">${date(a.installed)}</td><td class="num">${a.size ? size(a.size) : '<span class="faint">—</span>'}</td>
        <td class="act">${a.location ? `<button class="btn subtle small" data-reveal="${esc(a.location)}" title="Open install folder">${icon('open', 14)}</button>` : ''}</td></tr>`).join('')}
      </tbody></table></div>
      <div class="caption" style="margin-top:8px">To uninstall, use Windows Settings › Apps › Installed apps. <a data-settings="ms-settings:appsfeatures">Open it</a></div>`;
    body.querySelectorAll('[data-o]').forEach((b) => (b.onclick = () => { S.origin = b.dataset.o; installed(); }));
    body.querySelectorAll('[data-sort]').forEach((h) => (h.onclick = () => { S.dir = S.sort === h.dataset.sort ? -S.dir : (h.dataset.sort === 'name' ? 1 : -1); S.sort = h.dataset.sort; installed(); }));
    const q = body.querySelector('#ap-q');
    q.oninput = () => { S.q = q.value; clearTimeout(q._t); q._t = setTimeout(() => { installed().then(() => { const n = body.querySelector('#ap-q'); n.focus(); n.setSelectionRange(n.value.length, n.value.length); }); }, 180); };
    body.querySelector('#ap-hidden').onchange = (e) => { S.showHidden = e.target.checked; installed(); };
    wire();
  }

  async function startup() {
    const items = await cached('startup', () => invoke('startup_list'));
    const on = items.filter((i) => i.enabled).length;
    body.innerHTML = `<div class="caption" style="margin-bottom:8px">${int(items.length)} entries, ${int(on)} enabled. Soundings only lists them. To turn one off, use <a data-settings="ms-settings:startupapps">Settings › Apps › Startup</a>.</div>
      <div class="card scroll-box" style="max-height:calc(100vh - 260px)"><table class="tbl"><thead><tr><th>Name</th><th>Status</th><th>For</th><th>Command</th></tr></thead><tbody>
      ${items.sort((a, b) => b.enabled - a.enabled || a.name.localeCompare(b.name)).map((i) => `<tr><td class="name">${esc(i.name)}</td>
        <td>${i.enabled ? '<span class="pill accent">On</span>' : '<span class="pill">Off</span>'}</td><td class="muted">${esc(i.scope)}</td>
        <td><div class="mono path" style="max-width:640px">${esc(i.command)}</div></td></tr>`).join('')}</tbody></table></div>`;
    wire();
  }

  function wire() {
    body.querySelectorAll('[data-reveal]').forEach((b) => (b.onclick = () => reveal(b.dataset.reveal)));
    body.querySelectorAll('[data-settings]').forEach((a) => (a.onclick = () => reveal(a.dataset.settings)));
  }
  render();
}
