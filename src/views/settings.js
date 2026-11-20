import { invoke, esc, icon, toast, reveal, errorBox } from '../core.js';

export default function settings(el) {
  el.innerHTML = `<div class="page-head"><div><h1>Settings</h1></div></div><div id="set-body"></div>`;
  const body = el.querySelector('#set-body');

  async function render() {
    let s;
    try { s = await invoke('get_settings'); } catch (e) { body.innerHTML = errorBox(e); return; }
    const ex = s.excluded || [];
    body.innerHTML = `<h3 style="margin-bottom:8px">Scanning</h3>
      <div class="card">
        <div class="setting">${icon('folder')}<div class="grow"><b>Folders to skip</b><div class="caption">Never read, sized or listed. Good for private folders such as family photos.</div>
          <div class="list" style="margin-top:8px">${ex.map((p, i) => `<div class="li"><span class="grow mono">${esc(p)}</span><button class="btn subtle small" data-rm="${i}">Remove</button></div>`).join('') || '<div class="caption">None yet.</div>'}</div>
          <div class="row" style="margin-top:8px"><input type="text" id="set-add" placeholder="C:\\Users\\you\\Desktop\\Private" style="flex:1"><button class="btn" id="set-add-btn">Add</button></div></div></div>
      </div>
      <h3 style="margin:24px 0 8px">Privacy and safety</h3>
      <div class="card">
        <div class="setting">${icon('shield')}<div class="grow"><b>How Soundings treats your files</b><div class="caption">It reads names, sizes and dates. It opens file contents only to compare possible duplicates, on your request. It never deletes: the only removal is “Move to Recycle Bin” from the Review list, and that can be undone. Nothing leaves your PC.</div></div></div>
        <div class="setting">${icon('file')}<div class="grow"><b>Soundings data</b><div class="caption mono">%LOCALAPPDATA%\\Soundings</div></div><button class="btn" id="set-open">Open folder</button></div>
      </div>
      <h3 style="margin:24px 0 8px">About</h3>
      <div class="card"><div class="setting"><img src="icon.png" width="32" height="32" alt="" onerror="this.remove()"><div class="grow"><b>Soundings 0.1.0</b><div class="caption">Open source under the MIT licence. Built with Tauri and Rust.</div></div></div></div>`;
    body.querySelectorAll('[data-rm]').forEach((b) => (b.onclick = async () => { ex.splice(+b.dataset.rm, 1); await invoke('set_settings', { settings: { ...s, excluded: ex } }); render(); }));
    body.querySelector('#set-add-btn').onclick = async () => {
      const v = body.querySelector('#set-add').value.trim();
      if (!v) return;
      ex.push(v);
      await invoke('set_settings', { settings: { ...s, excluded: ex } });
      toast('Saved. It will be skipped from the next scan.');
      render();
    };
    body.querySelector('#set-open').onclick = async () => { const o = await invoke('overview'); reveal(o.dataDir); };
  }
  render();
}
