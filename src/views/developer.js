import { invoke, cached, forget, size, int, esc, loading, errorBox, tabs, reveal, icon, bar } from '../core.js';

const S = { tab: 'runtimes' };
// How to clear a cache with the tool's own command (shown, never run)
const CLEAR = {
  'npm cache': 'npm cache clean --force', 'pnpm store': 'pnpm store prune', 'Yarn cache': 'yarn cache clean',
  'pip cache': 'pip cache purge', 'uv cache': 'uv cache clean', 'Conda packages': 'conda clean --all',
  'Conda packages (Anaconda)': 'conda clean --all', 'Cargo registry': 'cargo cache --autoclean (needs cargo-cache)',
  'NuGet packages': 'dotnet nuget locals all --clear', 'Hugging Face models': 'huggingface-cli delete-cache',
  'Ollama models': 'ollama list  →  ollama rm <model>', 'Docker disk image': 'docker system prune  (then compact the disk)',
  'Temporary files': 'Settings › System › Storage › Temporary files',
};

const cleanVersion = (v) => String(v)
  .replace(/^(\w+) version "([^"]+)".*$/, (_, a, b) => `${a === 'java' ? 'Java' : a === 'openjdk' ? 'OpenJDK' : a} ${b}`)
  .replace(/^git version ([\d.]+).*$/, 'Git $1')
  .replace(/\s*\([^)]*\)\s*/g, ' ')
  .trim();

export default function developer(el, opts) {
  if (opts.tab) S.tab = opts.tab;
  el.innerHTML = `<div class="page-head"><div><h1>Developer</h1><div class="sub">Every copy of your languages and tools, and the caches they leave behind.</div></div>
    <div class="actions"><button class="btn" id="dv-refresh">${icon('refresh', 16)} Refresh</button></div></div><div id="dv-tabs"></div><div id="dv-body"></div>`;
  const body = el.querySelector('#dv-body');
  el.querySelector('#dv-refresh').onclick = () => { forget('dev-'); render(); };

  async function render() {
    el.querySelector('#dv-tabs').innerHTML = tabs([['runtimes', 'Runtimes'], ['caches', 'Caches & models'], ['envs', 'Python environments']], S.tab);
    el.querySelectorAll('.tab').forEach((t) => (t.onclick = () => { S.tab = t.dataset.tab; render(); }));
    body.innerHTML = loading(S.tab === 'caches' ? 'Measuring caches…' : 'Finding every copy (this runs each one with --version)…');
    try { await ({ runtimes, caches, envs })[S.tab](); } catch (e) { body.innerHTML = errorBox(e); }
  }

  async function runtimes() {
    const d = await cached('dev-runtimes', (fresh) => invoke('dev_runtimes', { fresh }));
    const dupes = d.runtimes.filter((r) => r.count > 1);
    body.innerHTML = `${dupes.length ? `<div class="infobar"><div class="grow">More than one copy of ${dupes.map((r) => `<b>${esc(r.tool)}</b> (${r.count})`).join(', ')}. The one marked <span class="pill accent">Used</span> is what runs when you type the command.</div></div>` : ''}
      ${d.runtimes.map((r) => `<div class="card" style="margin-bottom:12px">
        <div class="row" style="padding:12px 16px;border-bottom:1px solid var(--stroke)"><h3>${esc(r.tool)}</h3><span class="muted">${r.count} cop${r.count === 1 ? 'y' : 'ies'}</span>
          <span class="muted num" style="margin-left:auto">${size(r.copies.reduce((t, c) => t + (c.size || 0), 0))}</span></div>
        <table class="tbl"><tbody>${r.copies.sort((a, b) => b.active - a.active || b.onPath - a.onPath).map((c) => `<tr>
          <td style="width:220px" title="${esc(c.version)}">${esc(cleanVersion(c.version))}</td><td><div class="path mono" style="color:var(--text-2)">${esc(c.exe)}</div></td>
          <td style="width:130px">${c.active ? '<span class="pill accent">Used</span>' : c.alias ? '<span class="pill">Store shortcut</span>' : c.onPath ? '<span class="pill">On PATH, shadowed</span>' : '<span class="pill">Not on PATH</span>'}</td>
          <td class="num" style="width:90px">${c.size ? size(c.size) : '<span class="faint">—</span>'}</td>
          <td class="act" style="width:44px"><button class="btn subtle small" data-reveal="${esc(c.root)}">${icon('open', 14)}</button></td></tr>`).join('')}</tbody></table></div>`).join('')}
      <div class="caption">Sizes are the install folder. Conda's base install excludes its environments and package cache, which are listed separately.</div>`;
    wire();
  }

  async function caches() {
    const c = await cached('dev-caches', (fresh) => invoke('dev_caches', { fresh }));
    const s = [...c].sort((a, b) => b.size - a.size);
    const total = s.reduce((a, b) => a + b.size, 0);
    const mx = s[0]?.size || 1;
    body.innerHTML = `<div class="row" style="gap:40px;margin-bottom:16px"><div><div class="hero">${size(total)}</div><div class="caption">in caches, model stores and temporary files</div></div></div>
      <div class="card"><table class="tbl"><thead><tr><th>Cache</th><th>Group</th><th class="num">Size</th><th style="width:140px"></th><th>Clear it with</th><th></th></tr></thead><tbody>
      ${s.map((x) => `<tr><td><div class="name">${esc(x.name)}</div><div class="path">${esc(x.path)}</div></td><td class="muted">${esc(x.group)}</td>
        <td class="num">${size(x.size)}</td><td>${bar(x.size, mx)}</td><td><span class="mono muted">${esc(CLEAR[x.name] || '')}</span></td>
        <td class="act"><button class="btn subtle small" data-reveal="${esc(x.path)}">${icon('open', 14)}</button></td></tr>`).join('')}</tbody></table></div>
      <div class="caption" style="margin-top:8px">Soundings doesn't clear caches itself. Each tool's own command (shown) is the safe way, because it knows what is still in use.</div>`;
    wire();
  }

  async function envs() {
    const d = await cached('dev-runtimes', (fresh) => invoke('dev_runtimes', { fresh }));
    let venvs = [];
    try { venvs = (await invoke('artifacts')).filter((a) => a.kind === 'Python venv'); } catch {}
    const all = [...d.condaEnvs.map((e) => ({ ...e, kind: 'Conda' })), ...venvs.map((v) => ({ name: v.path.split('\\').slice(-2, -1)[0], path: v.path, size: v.size, kind: 'venv' }))]
      .sort((a, b) => b.size - a.size);
    const total = all.reduce((t, e) => t + e.size, 0);
    body.innerHTML = `<div class="row" style="gap:40px;margin-bottom:16px"><div><div class="hero">${int(all.length)}</div><div class="caption">Python environments · ${size(total)}</div></div></div>
      ${venvs.length ? '' : '<div class="infobar"><div class="grow">Map your drive on the Storage page to include project venvs here.</div></div>'}
      <div class="card scroll-box" style="max-height:calc(100vh - 320px)"><table class="tbl"><thead><tr><th>Environment</th><th>Type</th><th class="num">Size</th><th></th></tr></thead><tbody>
      ${all.map((e) => `<tr><td><div class="name">${esc(e.name)}</div><div class="path">${esc(e.path)}</div></td><td><span class="pill">${e.kind}</span></td><td class="num">${size(e.size)}</td>
        <td class="act"><button class="btn subtle small" data-reveal="${esc(e.path)}">${icon('open', 14)}</button></td></tr>`).join('')}</tbody></table></div>`;
    wire();
  }

  function wire() { body.querySelectorAll('[data-reveal]').forEach((b) => (b.onclick = () => reveal(b.dataset.reveal))); }
  render();
}
