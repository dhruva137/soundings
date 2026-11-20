import { invoke, cached, size, esc, date, dateTime, int, loading, errorBox, icon, pct } from '../core.js';
import { incidents } from './stability.js';

export default function overview(el, { go }) {
  el.innerHTML = `<div class="page-head"><div><h1>Overview</h1><div class="sub" id="ov-sub">Reading this PC…</div></div></div>
    <div id="ov-drives">${loading()}</div>
    <div class="section grid-3">
      <div class="card pad" id="ov-stab"><h3>Stability · last 30 days</h3>${loading()}</div>
      <div class="card pad" id="ov-cache"><h3>Heaviest caches</h3>${loading()}</div>
      <div class="card pad" id="ov-lib"><h3>On this PC</h3>${loading()}</div>
    </div>`;

  cached('overview', () => invoke('overview')).then((o) => {
    const m = o.machine;
    const gpu = (m.gpus || []).map((g) => g.name.replace(/\(R\)|\(TM\)|Laptop GPU/g, '').replace(/\s+/g, ' ').trim()).join(' + ');
    const tidy = (s) => (s && s === s.toUpperCase() && s.length > 3 ? s[0] + s.slice(1).toLowerCase() : s);
    const model = [tidy(m.manufacturer), m.model].filter(Boolean).join(' ');
    el.querySelector('#ov-sub').innerHTML = esc([model, m.cpu, `${Math.ceil(m.ram / 2 ** 30 / 2) * 2} GB RAM`, gpu].filter(Boolean).join(' · ')) +
      `<br><span class="faint">${esc(m.os || 'Windows')} ${esc(m.osVersion || '')} · build ${m.build} · set up ${date(m.installDate)}</span>`;
    el.querySelector('#ov-drives').innerHTML = `<div class="grid-2">${o.drives.map((d) => `
      <div class="card drive"><div class="row"><h3>${esc(d.label || 'Local Disk')} (${esc(d.root.replace('\\', ''))})</h3>
        <span class="muted" style="margin-left:auto">${size(d.free)} free of ${size(d.total)}</span></div>
        <div class="meter"><i style="width:${pct(d.used, d.total)}%;background:${pct(d.used, d.total) > 90 ? 'var(--critical)' : 'var(--accent)'}"></i></div>
        <div class="row"><span class="caption">${pct(d.used, d.total).toFixed(0)}% used · ${esc(d.fs)}</span>
        <a style="margin-left:auto" data-map="${esc(d.root)}">Map this drive</a></div></div>`).join('')}</div>`;
    el.querySelectorAll('[data-map]').forEach((a) => (a.onclick = () => go('storage', { scanRoot: a.dataset.map })));
  }).catch((e) => (el.querySelector('#ov-drives').innerHTML = errorBox(e)));

  cached('stab-30', () => invoke('stability_history', { days: 30 })).then((ev) => {
    const inc = incidents(ev);
    const crit = inc.filter((i) => i.sev === 'critical').length;
    const high = inc.filter((i) => i.sev === 'high').length;
    const top = inc.filter((i) => i.sev === 'critical' || i.sev === 'high').slice(0, 4).map((i) => ({ ...i.head, severity: i.sev }));
    el.querySelector('#ov-stab').innerHTML = `<h3>Stability · last 30 days</h3>
      <div class="row" style="margin:10px 0 6px;gap:24px">
        <div><div class="hero" style="font-size:28px;line-height:36px">${crit}</div><div class="caption">crashes or hard restarts</div></div>
        <div><div class="hero" style="font-size:28px;line-height:36px">${high}</div><div class="caption">GPU and app errors</div></div></div>
      <div class="list">${top.map((e) => `<div class="li"><span class="pill ${e.severity === 'critical' ? 'critical' : 'caution'}">${e.severity === 'critical' ? 'Crash' : 'GPU'}</span>
        <span class="grow" title="${esc(e.summary)}">${esc(e.summary)}</span><span class="caption">${dateTime(e.time).split(',')[0]}</span></div>`).join('') || '<div class="caption">No crashes recorded. Nice.</div>'}</div>
      <div style="margin-top:10px"><a data-go="stability">Open Stability</a></div>`;
    el.querySelector('#ov-stab [data-go]').onclick = () => go('stability');
  }).catch((e) => (el.querySelector('#ov-stab').innerHTML = `<h3>Stability</h3>${errorBox(e)}`));

  // Only show cache sizes already measured; measuring 70 GB of temp files is not worth doing on every launch.
  invoke('dev_caches', { cachedOnly: true }).then((c) => {
    if (!c) {
      el.querySelector('#ov-cache').innerHTML = `<h3>Heaviest caches</h3><p class="caption" style="margin:10px 0">Package caches, AI models and temporary files are often the biggest hidden space users. Measuring them takes about a minute.</p>
        <button class="btn" data-measure>Measure caches</button>`;
      el.querySelector('[data-measure]').onclick = () => go('developer', { tab: 'caches' });
      return;
    }
    const s = [...c].sort((a, b) => b.size - a.size);
    const total = s.reduce((a, b) => a + b.size, 0);
    el.querySelector('#ov-cache').innerHTML = `<h3>Heaviest caches</h3>
      <div class="hero" style="font-size:28px;line-height:36px;margin-top:10px">${size(total)}</div><div class="caption" style="margin-bottom:6px">in ${s.length} caches and model stores</div>
      <div class="list">${s.slice(0, 5).map((x) => `<div class="li"><span class="grow">${esc(x.name)}</span><span class="num muted">${size(x.size)}</span></div>`).join('')}</div>
      <div style="margin-top:10px"><a data-go>Open Developer</a></div>`;
    el.querySelector('#ov-cache [data-go]').onclick = () => go('developer', { tab: 'caches' });
  }).catch((e) => (el.querySelector('#ov-cache').innerHTML = `<h3>Caches</h3>${errorBox(e)}`));

  Promise.all([cached('apps', (fresh) => invoke('apps_list', { fresh })), cached('games', (fresh) => invoke('games_list', { fresh }))]).then(([a, g]) => {
    const visible = a.apps.filter((x) => !x.hidden);
    const mine = visible.filter((x) => x.origin === 'You installed').length;
    const oem = visible.filter((x) => x.origin === 'Manufacturer' || x.origin === 'Came with PC').length;
    const gsize = g.games.reduce((t, x) => t + (x.size || 0), 0);
    el.querySelector('#ov-lib').innerHTML = `<h3>On this PC</h3>
      <div class="list" style="margin-top:10px">
        <div class="li"><span class="grow">Programs you installed</span><b class="num">${int(mine)}</b></div>
        <div class="li"><span class="grow">From the manufacturer or preinstalled</span><b class="num">${int(oem)}</b></div>
        <div class="li"><span class="grow">Windows components and runtimes</span><b class="num">${int(visible.length - mine - oem)}</b></div>
        <div class="li"><span class="grow">Games</span><b class="num">${int(g.games.length)} · ${size(gsize)}</b></div>
      </div>
      <div class="row" style="margin-top:10px;gap:16px"><a data-go="apps">Open Apps</a><a data-go="games">Open Games</a></div>`;
    el.querySelectorAll('#ov-lib [data-go]').forEach((x) => (x.onclick = () => go(x.dataset.go)));
  }).catch((e) => (el.querySelector('#ov-lib').innerHTML = `<h3>On this PC</h3>${errorBox(e)}`));
}
