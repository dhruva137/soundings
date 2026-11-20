import { invoke, listen, cached, forget, size, int, esc, date, ago, base, parent, bar, loading, errorBox, tabs, review, reveal, icon, CATS, category, pct } from '../core.js';
import { Sunburst } from '../sunburst.js';

const S = { tab: 'map', focus: 0, root: 'C:\\', minMb: 250, oldDays: 365, dups: null };

export default function storage(el, opts) {
  if (opts.scanRoot) S.root = opts.scanRoot;
  if (opts.tab) S.tab = opts.tab;
  let burst = null;
  const unsub = [];

  el.innerHTML = `<div class="page-head"><div><h1>Storage</h1><div class="sub" id="st-sub">Reads file sizes only. Nothing is removed unless you review it.</div></div>
    <div class="actions"><select id="st-root" title="What to map"></select><button class="btn accent" id="st-scan">${icon('refresh', 16)} Map</button></div></div>
    <div id="st-progress"></div><div id="st-tabs"></div><div id="st-body"></div>`;
  const body = el.querySelector('#st-body');

  cached('overview', () => invoke('overview')).then((o) => {
    const sel = el.querySelector('#st-root');
    const home = o.home;
    sel.innerHTML = o.drives.map((d) => `<option value="${esc(d.root)}">${esc(d.label || 'Local Disk')} (${esc(d.root.slice(0, 2))})</option>`).join('') +
      `<option value="${esc(home)}">Your user folder</option>`;
    sel.value = S.root;
  });
  el.querySelector('#st-root').onchange = (e) => (S.root = e.target.value);
  el.querySelector('#st-scan').onclick = () => startScan();

  async function startScan() {
    S.focus = 0; S.dups = null;
    forget('scan:');
    try { await invoke('start_scan', { root: S.root }); } catch (e) { body.innerHTML = errorBox(e); return; }
    body.innerHTML = '';
    el.querySelector('#st-tabs').innerHTML = '';
    showProgress({ files: 0, bytes: 0 });
  }
  function showProgress(p) {
    el.querySelector('#st-scan').disabled = true;
    el.querySelector('#st-progress').innerHTML = `<div class="card pad" style="margin-bottom:16px"><div class="row"><b>Mapping ${esc(S.root)}</b>
      <span class="muted num" style="margin-left:auto">${int(p.files)} files · ${size(p.bytes)}</span></div><div class="progress" style="margin-top:10px"></div></div>`;
  }
  unsub.push(listen('scan-progress', showProgress));
  unsub.push(listen('scan-done', () => {
    el.querySelector('#st-progress').innerHTML = '';
    el.querySelector('#st-scan').disabled = false;
    render();
  }));

  async function render() {
    let st;
    try { st = await invoke('scan_status'); } catch (e) { body.innerHTML = errorBox(e); return; }
    if (st.scanning) { showProgress({ files: 0, bytes: 0 }); }
    if (!st.has) {
      el.querySelector('#st-tabs').innerHTML = '';
      if (!st.scanning) body.innerHTML = `<div class="card empty"><h2>Map this PC</h2><p>Soundings walks the drive and adds up folder sizes. It reads sizes and dates only, never file contents, and it never follows shortcuts or junctions. A full drive usually takes one to three minutes.</p>
        <button class="btn accent" id="st-first">Map ${esc(S.root)}</button></div>`;
      el.querySelector('#st-first')?.addEventListener('click', startScan);
      return;
    }
    el.querySelector('#st-sub').textContent = `${st.root} · ${size(st.size)} in ${int(st.files)} files · mapped in ${(st.ms / 1000).toFixed(1)}\u2009s`;
    el.querySelector('#st-tabs').innerHTML = tabs([['map', 'Map'], ['large', 'Large & old'], ['dups', 'Duplicates'], ['projects', 'Project folders'], ['downloads', 'Downloads']], S.tab);
    el.querySelectorAll('.tab').forEach((t) => (t.onclick = () => { S.tab = t.dataset.tab; render(); }));
    burst?.destroy(); burst = null;
    ({ map: renderMap, large: renderLarge, dups: renderDups, projects: renderProjects, downloads: renderDownloads })[S.tab]();
  }

  // ---------------- Map ----------------
  async function renderMap() {
    body.innerHTML = loading('Drawing the map…');
    let v;
    try { v = await invoke('tree', { id: S.focus, depth: 5 }); } catch (e) { body.innerHTML = errorBox(e); return; }
    const crumbs = v.crumbs.map((c, i) => i === v.crumbs.length - 1
      ? `<span class="cur">${esc(i === 0 ? c.name.replace(/\\$/, '') : c.name)}</span>`
      : `<a data-id="${c.id}">${esc(i === 0 ? c.name.replace(/\\$/, '') : c.name)}</a><span class="sep">›</span>`).join('');
    const kids = v.root.children;
    const max = v.root.size || 1;
    body.innerHTML = `<div class="crumbs">${crumbs}</div>
      <div class="map-wrap"><div><div class="map-canvas" id="map"></div>
        <div class="legend" style="margin-top:12px">${Object.entries(CATS).map(([k, c]) => `<span><b style="background:${c}"></b>${k}</span>`).join('')}</div></div>
      <div class="card"><div class="scroll-box" style="max-height:620px"><table class="tbl map-table"><thead><tr><th>Name</th><th class="num">Size</th><th style="width:80px"></th><th></th></tr></thead><tbody>
      ${kids.map((c) => {
        const p = `${v.path.replace(/\\$/, '')}\\${c.name}`;
        const dot = c.other ? 'transparent' : CATS[category(p)];
        return `<tr class="${c.id != null ? 'clickable' : ''}" data-id="${c.id ?? ''}">
          <td><div class="row" style="gap:10px"><span class="swatch" style="background:${dot}"></span>${icon(c.file ? 'file' : 'folder', 16)}<span class="name" style="font-weight:${c.other ? 400 : 600}">${esc(c.name)}</span></div></td>
          <td class="num">${size(c.size)}${c.files != null ? `<div class="caption">${int(c.files)} files</div>` : ''}</td><td>${bar(c.size, max)}</td>
          <td class="act">${c.other ? '' : `<button class="btn subtle small" data-reveal="${esc(p)}" title="Show in File Explorer">${icon('open', 14)}</button>`}
            ${c.file ? `<button class="btn small ${review.has(p) ? 'keep' : ''}" data-rev="${esc(p)}" data-size="${c.size}">${review.has(p) ? 'Marked' : 'Review'}</button>` : ''}</td></tr>`;
      }).join('')}</tbody></table></div></div></div>`;
    burst = new Sunburst(body.querySelector('#map'), {
      onZoom: (id) => { S.focus = id; renderMap(); },
      onUp: () => { if (v.crumbs.length > 1) { S.focus = v.crumbs[v.crumbs.length - 2].id; renderMap(); } },
    });
    burst.set(v);
    body.querySelectorAll('.crumbs a').forEach((a) => (a.onclick = () => { S.focus = +a.dataset.id; renderMap(); }));
    body.querySelectorAll('tr[data-id]').forEach((tr) => tr.addEventListener('dblclick', () => { if (tr.dataset.id) { S.focus = +tr.dataset.id; renderMap(); } }));
    body.querySelectorAll('tr[data-id]').forEach((tr) => tr.addEventListener('click', (e) => { if (!e.target.closest('button') && tr.dataset.id) { S.focus = +tr.dataset.id; renderMap(); } }));
    wireRowActions(body);
  }

  function wireRowActions(root) {
    root.querySelectorAll('[data-reveal]').forEach((b) => (b.onclick = (e) => { e.stopPropagation(); reveal(b.dataset.reveal); }));
    root.querySelectorAll('[data-rev]').forEach((b) => (b.onclick = (e) => {
      e.stopPropagation();
      const p = b.dataset.rev;
      review.toggle(p, +b.dataset.size, base(p), b.dataset.kind || 'file');
      const on = review.has(p);
      if (b.type === 'checkbox') b.checked = on;
      else { b.textContent = on ? 'Marked' : 'Review'; b.classList.toggle('keep', on); }
    }));
  }

  const fileRow = (f, extra = '') => `<tr><td><input type="checkbox" data-rev="${esc(f.path)}" data-size="${f.size}" ${review.has(f.path) ? 'checked' : ''} title="Mark for review"></td>
    <td><div class="name">${esc(base(f.path))}${f.repo ? ' <span class="pill" style="font-weight:400">In a git repo</span>' : ''}</div><div class="path">${esc(parent(f.path))}</div></td>
    <td class="num">${size(f.size)}</td><td class="num muted">${extra}</td>
    <td class="act"><button class="btn subtle small" data-reveal="${esc(f.path)}" title="Show in File Explorer">${icon('open', 14)}</button></td></tr>`;

  // ---------------- Large & old ----------------
  async function renderLarge() {
    body.innerHTML = loading();
    let r;
    try { r = await invoke('large_old', { minMb: S.minMb, oldDays: S.oldDays }); } catch (e) { body.innerHTML = errorBox(e); return; }
    const sum = (l) => l.reduce((a, b) => a + b.size, 0);
    body.innerHTML = `<div class="row wrap" style="margin-bottom:14px;gap:16px">
        <label class="row">Large means over <select id="lg-mb">${[100, 250, 500, 1000, 4000].map((m) => `<option ${m === S.minMb ? 'selected' : ''} value="${m}">${m >= 1000 ? m / 1000 + '\u2009GB' : m + '\u2009MB'}</option>`).join('')}</select></label>
        <label class="row">Old means untouched for <select id="lg-days">${[180, 365, 730, 1095].map((d) => `<option ${d === S.oldDays ? 'selected' : ''} value="${d}">${d / 365 >= 1 ? (d / 365).toFixed(d % 365 ? 1 : 0) + ' year' + (d > 365 ? 's' : '') : '6 months'}</option>`).join('')}</select></label>
        <span class="caption">Only files in your user folder, outside AppData and project build folders.</span></div>
      <div class="stack" style="gap:24px">
        <div><h3 style="margin-bottom:8px">Large files <span class="faint" style="font-weight:400">${r.large.length} · ${size(sum(r.large))}</span></h3>
          <div class="card scroll-box"><table class="tbl"><thead><tr><th></th><th>File</th><th class="num">Size</th><th class="num">Modified</th><th></th></tr></thead><tbody>
          ${r.large.map((f) => fileRow(f, date(f.modified))).join('') || '<tr><td colspan="5" class="muted">No files that large.</td></tr>'}</tbody></table></div></div>
        <div><h3 style="margin-bottom:8px">Not opened in a long time <span class="faint" style="font-weight:400">${r.old.length} · ${size(sum(r.old))}</span></h3>
          <div class="card scroll-box"><table class="tbl"><thead><tr><th></th><th>File</th><th class="num">Size</th><th class="num">Last used</th><th></th></tr></thead><tbody>
          ${r.old.map((f) => fileRow(f, ago(Math.max(f.modified, f.accessed)))).join('') || '<tr><td colspan="5" class="muted">Nothing over 10 MB has gone unused that long.</td></tr>'}</tbody></table></div>
          <div class="caption" style="margin-top:6px">Windows doesn't always record when a file was last opened, so check before removing.</div></div></div>`;
    body.querySelector('#lg-mb').onchange = (e) => { S.minMb = +e.target.value; renderLarge(); };
    body.querySelector('#lg-days').onchange = (e) => { S.oldDays = +e.target.value; renderLarge(); };
    wireRowActions(body);
  }

  // ---------------- Duplicates ----------------
  async function renderDups() {
    if (!S.dups) {
      body.innerHTML = `<div class="card pad"><div class="row"><span class="spinner"></span><b>Comparing files</b><span class="muted num" id="dp-n" style="margin-left:auto"></span></div>
        <div class="caption" style="margin-top:6px">Files of the same size are compared by content (first 64 KB, then the whole file). Only your user folder is checked.</div></div>`;
      const off = await listen('dup-progress', (n) => { const x = body.querySelector('#dp-n'); if (x) x.textContent = `${int(n)} checks`; });
      try { S.dups = await invoke('duplicates'); } catch (e) { off(); body.innerHTML = errorBox(e); return; }
      off();
      if (S.tab !== 'dups') return;
    }
    const g = S.dups.groups;
    const waste = g.reduce((a, b) => a + b.wasted, 0);
    body.innerHTML = g.length === 0 ? `<div class="card empty"><h2>No duplicates over 1\u2009MB</h2><p>Every large file in your user folder is unique.</p></div>` :
      `<div class="infobar"><div class="grow"><b>${size(waste)}</b> is taken by extra copies in ${int(g.length)} groups. The oldest copy in each group is treated as the original.
        Copies inside a git repository are never marked automatically, because code and results there may depend on them.</div>
        <button class="btn" id="dp-auto">Mark extra copies for review</button></div>
      ${g.slice(0, 300).map((grp, gi) => {
        const rowsHtml = (list, off) => list.map((f, j) => { const i = j + off; return `<tr><td style="width:36px"><input type="checkbox" data-g="${gi}" data-dup="${esc(f.path)}" data-size="${f.size}" ${review.has(f.path) ? 'checked' : ''}></td>
          <td><div class="path" style="font-size:13px;color:var(--text)">${esc(f.path)}</div></td>
          <td class="num muted">${i === 0 ? '<span class="pill">Original</span> ' : ''}${f.repo ? '<span class="pill">In a git repo</span> ' : ''}${date(f.created)}</td>
          <td class="act"><button class="btn subtle small" data-reveal="${esc(f.path)}">${icon('open', 14)}</button></td></tr>`; }).join('');
        const head = grp.files.slice(0, 6); const rest = grp.files.slice(6);
        return `<div class="card" style="margin-bottom:10px"><div class="row" style="padding:10px 14px;border-bottom:1px solid var(--stroke)">
        <b>${esc(base(grp.files[0].path))}</b><span class="muted">${grp.files.length} copies × ${size(grp.size)}</span><span class="caption" style="margin-left:auto">${size(grp.wasted)} extra</span></div>
        <table class="tbl"><tbody>${rowsHtml(head, 0)}</tbody></table>
        ${rest.length ? `<details><summary class="caption" style="padding:8px 14px;cursor:default">Show ${rest.length} more copies</summary><table class="tbl"><tbody>${rowsHtml(rest, 6)}</tbody></table></details>` : ''}</div>`;
      }).join('')}
      ${g.length > 300 ? `<div class="caption">Showing the 300 biggest groups.</div>` : ''}`;
    body.querySelectorAll('[data-dup]').forEach((cb) => (cb.onchange = () => {
      const grp = g[+cb.dataset.g];
      if (cb.checked && grp.files.every((f) => f.path === cb.dataset.dup || review.has(f.path))) {
        cb.checked = false;
        alert('Keep at least one copy of every file.');
        return;
      }
      review.toggle(cb.dataset.dup, +cb.dataset.size, base(cb.dataset.dup));
    }));
    body.querySelector('#dp-auto')?.addEventListener('click', () => {
      for (const grp of g) grp.files.slice(1).filter((f) => !f.repo).forEach((f) => review.add(f.path, f.size, base(f.path)));
      renderDups();
    });
    wireRowActions(body);
  }

  // ---------------- Project folders ----------------
  async function renderProjects() {
    body.innerHTML = loading();
    let a;
    try { a = await invoke('artifacts'); } catch (e) { body.innerHTML = errorBox(e); return; }
    const total = a.reduce((x, y) => x + y.size, 0);
    const kinds = {};
    for (const x of a) kinds[x.kind] = (kinds[x.kind] || 0) + x.size;
    body.innerHTML = `<div class="infobar"><div class="grow"><b>${size(total)}</b> in ${int(a.length)} dependency and build folders inside your projects.
      These are rebuilt by <span class="mono">npm install</span>, <span class="mono">pip install -r requirements.txt</span> or a build, so old ones are usually safe to review.</div></div>
      <div class="chips" style="margin-bottom:12px">${Object.entries(kinds).sort((x, y) => y[1] - x[1]).map(([k, s]) => `<span class="chip">${esc(k)} <span class="n">${size(s)}</span></span>`).join('')}</div>
      <div class="card scroll-box" style="max-height:640px"><table class="tbl"><thead><tr><th></th><th>Project</th><th>Kind</th><th class="num">Size</th><th class="num">Project last changed</th><th></th></tr></thead><tbody>
      ${a.slice(0, 500).map((x) => `<tr><td><input type="checkbox" data-rev="${esc(x.path)}" data-size="${x.size}" data-kind="folder" ${review.has(x.path) ? 'checked' : ''}></td>
        <td><div class="name">${esc(base(parent(x.path)))}</div><div class="path">${esc(x.path)}</div></td><td><span class="pill">${esc(x.kind)}</span></td>
        <td class="num">${size(x.size)}</td><td class="num muted">${ago(x.modified)}</td>
        <td class="act"><button class="btn subtle small" data-reveal="${esc(x.path)}">${icon('open', 14)}</button></td></tr>`).join('')}</tbody></table></div>`;
    wireRowActions(body);
  }

  // ---------------- Downloads ----------------
  async function renderDownloads() {
    body.innerHTML = loading('Reading Downloads…');
    let d;
    try { d = await cached('downloads', () => invoke('downloads_insights')); } catch (e) { body.innerHTML = errorBox(e); return; }
    const months = d.months.slice(-18);
    const mx = Math.max(1, ...months.map((m) => m.bytes));
    const busiest = [...d.months].sort((a, b) => b.count - a.count)[0];
    const fmx = Math.max(1, ...d.families.map((f) => f.bytes));
    const label = (m) => new Date(m + '-01').toLocaleDateString(undefined, { month: 'short' }) + (m.endsWith('-01') ? ` ’${m.slice(2, 4)}` : '');
    body.innerHTML = `<div class="row wrap" style="gap:40px;margin-bottom:20px">
        <div><div class="hero">${int(d.count)}</div><div class="caption">files · ${size(d.bytes)} in Downloads</div></div>
        ${busiest ? `<div><div class="hero">${new Date(busiest.month + '-01').toLocaleDateString(undefined, { month: 'short', year: 'numeric' })}</div><div class="caption">busiest month · ${int(busiest.count)} files</div></div>` : ''}
        ${d.hosts[0] ? `<div><div class="hero" style="font-size:28px">${esc(d.hosts[0].host)}</div><div class="caption">most common source · ${int(d.hosts[0].count)} files</div></div>` : ''}</div>
      <div class="grid-2" style="align-items:start">
        <div class="card pad"><h3>Downloaded per month (size)</h3><div class="bars-v" style="margin-top:14px">${months.map((m) => `<div class="c" title="${m.month}: ${int(m.count)} files, ${size(m.bytes)}"><i style="height:${pct(m.bytes, mx)}%"></i></div>`).join('')}</div>
          <div class="bars-x">${months.map((m) => `<span>${label(m.month)}</span>`).join('')}</div></div>
        <div class="card pad"><h3>By kind</h3><div class="list" style="margin-top:8px">${d.families.map((f) => `<div class="li"><span style="width:110px">${esc(f.family)}</span><span class="grow">${bar(f.bytes, fmx)}</span><span class="num muted" style="width:150px;text-align:right">${int(f.count)} · ${size(f.bytes)}</span></div>`).join('')}</div></div>
        <div class="card pad"><h3>Where downloads came from</h3><div class="caption">From the source address your browser saves with each download.</div><div class="list" style="margin-top:8px">${d.hosts.map((h) => `<div class="li"><span class="grow">${esc(h.host)}</span><span class="num muted">${int(h.count)} · ${size(h.bytes)}</span></div>`).join('') || '<div class="caption">No source information found.</div>'}</div></div>
        <div class="card pad"><h3>Most recent</h3><div class="list" style="margin-top:8px">${d.recent.map((r) => `<div class="li"><span class="grow" title="${esc(r.path)}">${esc(base(r.path))}</span><span class="num muted">${size(r.size)}</span><a data-reveal="${esc(r.path)}">${icon('open', 14)}</a></div>`).join('')}</div></div>
      </div>`;
    wireRowActions(body);
  }

  const onRecycled = () => { forget('downloads'); S.dups = null; render(); };
  window.addEventListener('soundings:recycled', onRecycled);
  window.addEventListener('soundings:restored', onRecycled);
  render();
  return () => {
    burst?.destroy();
    unsub.forEach((p) => p.then((f) => f()));
    window.removeEventListener('soundings:recycled', onRecycled);
    window.removeEventListener('soundings:restored', onRecycled);
  };
}
