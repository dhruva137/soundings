import { invoke, esc, icon, toast, reveal, errorBox, confirmDialog } from '../core.js';

export default function settings(el) {
  el.innerHTML = `<div class="page-head"><div><h1>Settings</h1></div></div><div id="set-body"></div>`;
  const body = el.querySelector('#set-body');

  async function render() {
    let s; let c;
    try { [s, c] = await Promise.all([invoke('get_settings'), invoke('claude_info')]); } catch (e) { body.innerHTML = errorBox(e); return; }
    const ex = s.excluded || [];
    const codeCmd = `claude mcp add soundings --scope user -- "${c.exe}" --mcp`;
    body.innerHTML = `<h3 style="margin-bottom:8px">Scanning</h3>
      <div class="card">
        <div class="setting">${icon('folder')}<div class="grow"><b>Folders to skip</b><div class="caption">Never read, sized or listed, and never shown to Claude. Good for private folders such as family photos.</div>
          <div class="list" style="margin-top:8px">${ex.map((p, i) => `<div class="li"><span class="grow mono">${esc(p)}</span><button class="btn subtle small" data-rm="${i}">Stop skipping</button></div>`).join('') || '<div class="caption">None yet.</div>'}</div>
          <div class="row" style="margin-top:8px"><input type="text" id="set-add" placeholder="C:\\Users\\you\\Desktop\\Private" style="flex:1"><button class="btn" id="set-add-btn">Skip this folder</button></div></div></div>
      </div>

      <h3 style="margin:24px 0 8px">Connect Claude</h3>
      <div class="card">
        <div class="setting">${icon('shield')}<div class="grow"><b>Let Claude ask Soundings about this PC</b>
          <div class="caption">Claude can then answer things like “what's filling my drive?” or “why does my PC keep crashing?” using Soundings' saved map, apps, games, developer tools and crash history.
          It's read-only: Claude cannot change, move or delete anything. What Claude reads becomes part of your chat with it, so this stays off until you turn it on.</div></div>
          <button class="toggle ${c.enabled ? 'on' : ''}" id="set-claude" role="switch" aria-checked="${c.enabled}" title="${c.enabled ? 'Turn off' : 'Turn on'}"></button></div>
        ${c.enabled ? `
        <div class="setting">${icon('apps')}<div class="grow"><b>Claude Desktop</b><div class="caption">${c.desktopFound
          ? (c.desktopConnected ? 'Connected. Restart Claude Desktop if Soundings doesn\u2019t show up in its tools.' : 'Adds Soundings to Claude Desktop\u2019s settings file (a backup of that file is kept). Restart Claude Desktop afterwards.')
          : 'Claude Desktop isn\u2019t installed on this PC.'}</div></div>
          ${c.desktopFound ? `<button class="btn ${c.desktopConnected ? '' : 'accent'}" id="set-desk">${c.desktopConnected ? 'Disconnect' : 'Connect'}</button>` : ''}</div>
        <div class="setting">${icon('developer')}<div class="grow"><b>Claude Code</b><div class="caption">Run this once in a terminal:</div>
          <div class="mono caption" style="margin-top:6px;user-select:all;word-break:break-all">${esc(codeCmd)}</div></div>
          <button class="btn" id="set-copy">Copy</button></div>` : ''}
      </div>

      <h3 style="margin:24px 0 8px">Privacy</h3>
      <div class="card">
        <div class="setting">${icon('shield')}<div class="grow"><b>How Soundings treats your files</b><div class="caption">
          It reads names, sizes and dates. It opens file contents only to compare possible duplicates, and only when you open the Duplicates tab.
          It has no way to delete, move or change your files: to remove something, use “Show in File Explorer” and do it there.
          It makes no internet connections and has no analytics or tracking.</div></div></div>
        <div class="setting">${icon('file')}<div class="grow"><b>What Soundings keeps</b><div class="caption">Your settings, shortlist, the last disk map and a few caches, all on this PC in <span class="mono">%LOCALAPPDATA%\\Soundings</span>.</div></div><button class="btn" id="set-open">Open folder</button></div>
      </div>

      <h3 style="margin:24px 0 8px">About</h3>
      <div class="card"><div class="setting"><div class="grow"><b>Soundings 0.2.0</b><div class="caption">Free and open source (MIT). github.com/dhruva137/soundings</div></div></div></div>`;

    const save = async (next) => { await invoke('set_settings', { settings: next }); Object.assign(s, next); };
    body.querySelectorAll('[data-rm]').forEach((b) => (b.onclick = async () => { ex.splice(+b.dataset.rm, 1); await save({ ...s, excluded: ex }); render(); }));
    body.querySelector('#set-add-btn').onclick = async () => {
      const v = body.querySelector('#set-add').value.trim();
      if (!v) return;
      ex.push(v);
      await save({ ...s, excluded: ex });
      toast('Saved. It will be skipped from the next map.');
      render();
    };
    body.querySelector('#set-claude').onclick = async () => {
      const on = !c.enabled;
      if (on && !(await confirmDialog({
        title: 'Let Claude read this PC\u2019s info?',
        body: 'When Claude uses Soundings, the answers (file names, sizes, apps, crash history) are sent to Claude as part of your conversation. Skipped folders are never included, and Claude can\u2019t change anything. You can turn this off any time.',
        ok: 'Turn on',
      }))) return;
      await save({ ...s, claude: on });
      toast(on ? 'Claude can now ask Soundings about this PC.' : 'Claude access is off.');
      render();
    };
    body.querySelector('#set-desk')?.addEventListener('click', async () => {
      try { await invoke('claude_desktop_connect', { on: !c.desktopConnected }); toast(c.desktopConnected ? 'Removed from Claude Desktop.' : 'Added. Restart Claude Desktop to use it.'); }
      catch (e) { toast(esc(String(e)), 'caution'); }
      render();
    });
    body.querySelector('#set-copy')?.addEventListener('click', async () => {
      try { await navigator.clipboard.writeText(codeCmd); toast('Copied.'); } catch { toast('Could not copy.', 'caution'); }
    });
    body.querySelector('#set-open').onclick = async () => { const o = await invoke('overview'); reveal(o.dataDir); };
  }
  render();
}
