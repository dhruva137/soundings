import { invoke, cached, forget, size, int, esc, date, ago, loading, errorBox, reveal, icon } from '../core.js';

export default function games(el) {
  el.innerHTML = `<div class="page-head"><div><h1>Games</h1><div class="sub">Found through Steam, Epic, GOG, Xbox, Ubisoft, Rockstar, Minecraft and Roblox, plus game-engine files for anything installed on its own.</div></div>
    <div class="actions"><button class="btn" id="gm-refresh">${icon('refresh', 16)} Refresh</button></div></div><div id="gm-body">${loading('Looking for games…')}</div>`;
  const body = el.querySelector('#gm-body');
  el.querySelector('#gm-refresh').onclick = () => { forget('games'); render(); };

  async function render() {
    body.innerHTML = loading('Looking for games…');
    let g;
    try { g = await cached('games', (fresh) => invoke('games_list', { fresh })); } catch (e) { body.innerHTML = errorBox(e); return; }
    const total = g.games.reduce((t, x) => t + (x.size || 0), 0);
    const lsize = g.leftovers.reduce((t, x) => t + (x.size || 0), 0);
    const sorted = [...g.games].sort((a, b) => b.size - a.size);
    const row = (x) => `<tr><td><div class="name">${esc(x.name)}</div><div class="path">${esc(x.path)}</div></td>
      <td><span class="pill">${esc(x.source)}</span></td>
      <td class="num">${size(x.size)}</td><td class="num muted">${x.lastPlayed ? ago(x.lastPlayed) : '—'}</td>
      <td class="caption">${x.confidence < 0.9 ? esc(x.signals.join(', ')) : ''}</td>
      <td class="act">${x.path ? `<button class="btn subtle small" data-reveal="${esc(x.path)}">${icon('open', 14)}</button>` : ''}</td></tr>`;
    body.innerHTML = `<div class="row" style="gap:40px;margin-bottom:16px"><div><div class="hero">${int(g.games.length)}</div><div class="caption">games · ${size(total)}</div></div>
        ${g.leftovers.length ? `<div><div class="hero">${size(lsize)}</div><div class="caption">left behind by ${g.leftovers.length} uninstalled Steam games</div></div>` : ''}</div>
      ${g.games.length ? `<div class="card"><table class="tbl"><thead><tr><th>Game</th><th>From</th><th class="num">Size</th><th class="num">Last played</th><th>How it was found</th><th></th></tr></thead>
        <tbody>${sorted.map(row).join('')}</tbody></table></div>` : `<div class="card empty"><h2>No games found</h2><p>Nothing from the common launchers, and no folders that look like a game engine build.</p></div>`}
      ${g.maybe.length ? `<div class="section"><h3>Might be games</h3><div class="caption" style="margin-bottom:8px">Weak signals only. Listed so you can judge.</div>
        <div class="card"><table class="tbl"><tbody>${g.maybe.map(row).join('')}</tbody></table></div></div>` : ''}
      ${g.leftovers.length ? `<div class="section"><h3>Steam leftovers</h3><div class="caption" style="margin-bottom:8px">Folders in steamapps\\common with no installed game: shader caches, configs or half-finished downloads.</div>
        <div class="card"><table class="tbl"><tbody>${g.leftovers.sort((a, b) => b.size - a.size).map((x) => `<tr><td><div class="name">${esc(x.name)}</div><div class="path">${esc(x.path)}</div></td>
        <td class="num">${size(x.size)}</td><td class="act"><button class="btn subtle small" data-reveal="${esc(x.path)}">${icon('open', 14)}</button></td></tr>`).join('')}</tbody></table></div></div>` : ''}`;
    body.querySelectorAll('[data-reveal]').forEach((b) => (b.onclick = () => reveal(b.dataset.reveal)));
  }
  render();
}
