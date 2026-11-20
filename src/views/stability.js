import { invoke, cached, forget, int, esc, dateTime, loading, errorBox, icon, confirmDialog, toast, reveal } from '../core.js';

const S = { days: 365, filter: 'important' };
const ROWS = [
  ['bugcheck', 'Blue screens'],
  ['unexpected-shutdown', 'Hard restarts'],
  ['gpu-reset', 'GPU resets'],
  ['gpu-error', 'GPU driver errors'],
  ['low-memory', 'Low memory'],
  ['app-crash', 'App crashes'],
  ['hardware', 'Hardware errors'],
  ['storage', 'Disk errors'],
  ['thermal', 'Throttling'],
];
const HELP = {
  bugcheck: 'https://learn.microsoft.com/windows-hardware/drivers/debugger/bug-check-code-reference2',
  'gpu-reset': 'https://learn.microsoft.com/windows-hardware/drivers/display/timeout-detection-and-recovery',
  'gpu-error': 'https://learn.microsoft.com/windows-hardware/drivers/display/timeout-detection-and-recovery',
  'unexpected-shutdown': 'https://learn.microsoft.com/troubleshoot/windows-client/performance/event-id-41-restart',
  'low-memory': 'https://learn.microsoft.com/troubleshoot/windows-client/performance/troubleshoot-low-memory-issues',
  hardware: 'https://learn.microsoft.com/windows-hardware/drivers/whea/',
};
const SEV = { critical: 0, high: 1, medium: 2, low: 3, info: 4 };

// Group events within 15 minutes into one incident (one BSOD = K41 + 6008 + 1001 + re-reports).
export function incidents(ev) {
  const sorted = ev.filter((e) => e.severity !== 'info').sort((a, b) => new Date(a.time) - new Date(b.time));
  const out = [];
  for (const e of sorted) {
    const t = new Date(e.time).getTime();
    const last = out[out.length - 1];
    if (last && t - last.end <= 15 * 60 * 1000) { last.events.push(e); last.end = t; }
    else out.push({ start: t, end: t, events: [e] });
  }
  for (const i of out) {
    i.events.sort((a, b) => SEV[a.severity] - SEV[b.severity]);
    i.head = i.events.find((e) => e.category === 'bugcheck') || i.events[0];
    i.sev = i.head.severity;
  }
  return out.reverse();
}

export default function stability(el) {
  el.innerHTML = `<div class="page-head"><div><h1>Stability</h1><div class="sub">Crashes, GPU resets and hardware errors from the Windows event logs.</div></div>
    <div class="actions"><select id="sb-days">${[[30, 'Last 30 days'], [90, 'Last 90 days'], [365, 'Last year']].map(([d, l]) => `<option value="${d}" ${d === S.days ? 'selected' : ''}>${l}</option>`).join('')}</select>
    <button class="btn" id="sb-refresh">${icon('refresh', 16)} Refresh</button></div></div>
    <div id="sb-monitor"></div><div id="sb-exp"></div><div id="sb-body">${loading('Reading event logs…')}</div>`;
  let expTimer = null;

  const spark = (vals, max, label, unit, warnAt) => {
    const v = vals.filter((x) => x != null);
    const last = v[v.length - 1];
    const W = 220; const H = 36;
    const mx = max || Math.max(1, ...v);
    const pts = v.map((y, i) => `${(i / Math.max(1, v.length - 1)) * W},${H - (y / mx) * (H - 2) - 1}`).join(' ');
    const hot = warnAt != null && last >= warnAt;
    return `<div><div class="caption">${label}</div><div class="row" style="gap:10px"><b class="num" style="font-size:20px;line-height:28px;${hot ? 'color:var(--critical)' : ''}">${last != null ? Math.round(last) : '—'}<span class="caption"> ${unit}</span></b>
      <svg width="${W}" height="${H}" viewBox="0 0 ${W} ${H}" style="overflow:visible"><polyline points="${pts}" fill="none" stroke="${hot ? 'var(--critical)' : 'var(--accent)'}" stroke-width="1.5"/></svg></div></div>`;
  };

  async function experiment() {
    const box = el.querySelector('#sb-exp');
    let t;
    try { t = await invoke('telemetry_status'); } catch { box.innerHTML = ''; return; }
    if (!t.available) { box.innerHTML = ''; return; }
    const s = t.samples || [];
    box.innerHTML = `<div class="card" style="margin-bottom:20px"><div class="setting">${icon('stability', 20)}
      <div class="grow"><b>Watch an experiment</b><div class="caption">${t.running
        ? `Recording GPU temperature, power, GPU memory and system memory every 5 seconds to <span class="mono">${esc(t.file || '')}</span>. Lines are saved as they arrive, so the minutes before a crash survive it.`
        : 'Before a heavy training run, turn this on. It samples the NVIDIA GPU every 5 seconds while Soundings is open, and stops when you close it. Off by default because reading the GPU keeps it awake.'}</div></div>
      <button class="toggle ${t.running ? 'on' : ''}" id="sb-exp-toggle" role="switch" aria-checked="${t.running}"></button></div>
      ${t.running ? `<div class="row wrap" style="gap:28px;padding:4px 18px 16px">
        ${spark(s.map((x) => x.temp), 100, 'GPU temperature', '°C', 87)}
        ${spark(s.map((x) => x.power), null, 'GPU power', 'W')}
        ${spark(s.map((x) => (x.memTotal ? (x.memUsed / x.memTotal) * 100 : null)), 100, 'GPU memory', '%', 95)}
        ${spark(s.map((x) => x.commit), 100, 'System memory committed', '%', 90)}</div>` : ''}</div>`;
    box.querySelector('#sb-exp-toggle').onclick = async () => {
      try { await invoke(t.running ? 'telemetry_stop' : 'telemetry_start'); } catch (e) { toast(esc(e), 'critical'); }
      experiment();
    };
    clearTimeout(expTimer);
    if (t.running && document.body.contains(box)) expTimer = setTimeout(experiment, 5000);
  }
  const body = el.querySelector('#sb-body');
  el.querySelector('#sb-days').onchange = (e) => { S.days = +e.target.value; render(); };
  el.querySelector('#sb-refresh').onclick = () => { forget('stab-'); render(); monitor(); };

  async function monitor() {
    const box = el.querySelector('#sb-monitor');
    let m;
    try { m = await invoke('monitor_status'); } catch (e) { box.innerHTML = errorBox(e); return; }
    box.innerHTML = `<div class="card setting" style="margin-bottom:20px">${icon('shield', 20)}
      <div class="grow"><b>Record crashes in the background</b><div class="caption">${m.enabled
        ? `On. Windows wakes Soundings only when a crash or GPU error is logged, and once at sign-in, then shows a notification. Nothing runs in between. ${int(m.recorded)} events recorded so far.`
        : 'Off. When on, Windows starts a tiny recorder only when a crash, GPU reset or hardware error is logged, so it costs no battery while idle. It keeps a history longer than Windows does and notifies you.'}</div></div>
      <button class="toggle ${m.enabled ? 'on' : ''}" id="sb-toggle" role="switch" aria-checked="${m.enabled}" title="${m.enabled ? 'Turn off' : 'Turn on'}"></button></div>`;
    box.querySelector('#sb-toggle').onclick = async () => {
      const on = !m.enabled;
      if (on) {
        const ok = await confirmDialog({
          title: 'Turn on the crash recorder?',
          body: 'Soundings will add a task named “Soundings Crash Recorder” to Windows Task Scheduler. It runs only when a matching event is logged and at sign-in, as you, without admin rights. You can turn it off here at any time, which removes the task.',
          ok: 'Turn on',
        });
        if (!ok) return;
      }
      try { await invoke('monitor_enable', { on }); toast(on ? 'Crash recorder is on' : 'Crash recorder is off; task removed'); }
      catch (e) { toast(esc(e), 'critical'); }
      monitor();
    };
  }

  async function render() {
    body.innerHTML = loading('Reading event logs…');
    let ev;
    try { ev = await cached(`stab-${S.days}`, () => invoke('stability_history', { days: S.days })); } catch (e) { body.innerHTML = errorBox(e); return; }
    const inc = incidents(ev);
    const count = (c) => ev.filter((e) => e.category === c).length;
    const bsod = inc.filter((i) => i.events.some((e) => e.category === 'bugcheck')).length;
    const hard = inc.filter((i) => i.sev === 'critical' && !i.events.some((e) => e.category === 'bugcheck')).length;
    const codes = {};
    ev.filter((e) => e.category === 'bugcheck').forEach((e) => { const k = e.summary.replace('Blue screen ', ''); codes[k] = (codes[k] || 0) + 1; });
    const topCode = Object.entries(codes).sort((a, b) => b[1] - a[1])[0];
    const firstDay = ev.length ? new Date(ev[ev.length - 1].time) : null;

    body.innerHTML = `<div class="row wrap" style="gap:40px;margin-bottom:18px">
        <div><div class="hero">${bsod}</div><div class="caption">blue screens</div></div>
        <div><div class="hero">${hard}</div><div class="caption">other hard restarts</div></div>
        <div><div class="hero">${count('gpu-reset')}</div><div class="caption">GPU resets</div></div>
        <div><div class="hero">${count('low-memory')}</div><div class="caption">low-memory warnings</div></div></div>
      ${topCode ? `<div class="infobar ${bsod ? 'caution' : ''}"><div class="grow">Most common blue screen: <b>${esc(topCode[0])}</b> (${topCode[1]}×).
        ${/VIDEO_TDR|VIDEO_/.test(topCode[0]) ? 'The graphics card stopped responding and Windows could not reset it. Usually a driver, overheating, power, or GPU memory under heavy load.' : ''}</div></div>` : ''}
      <div class="card pad timeline" id="sb-tl"></div>
      <div class="caption" style="margin:6px 0 0">Filled red dot: crash · amber ring: GPU or app error · grey: warnings. Dashed blue lines: graphics driver installs · dotted: Windows updates.
        ${firstDay ? `Windows' own logs go back to ${firstDay.toLocaleDateString()}.` : ''}</div>
      <div class="section"><div class="row" style="margin-bottom:10px"><h3>Incidents</h3><span class="caption">Events within 15 minutes of each other are grouped.</span>
        <div class="chips" style="margin-left:auto"><button class="chip ${S.filter === 'important' ? 'on' : ''}" data-f="important">Crashes & GPU</button><button class="chip ${S.filter === 'all' ? 'on' : ''}" data-f="all">Everything</button></div></div>
        <div class="card" id="sb-list"></div></div>`;
    drawTimeline(body.querySelector('#sb-tl'), ev);
    const list = () => {
      const rows = inc.filter((i) => S.filter === 'all' || SEV[i.sev] <= 1);
      body.querySelector('#sb-list').innerHTML = rows.length ? rows.slice(0, 300).map((i) => `<details class="incident" style="border-bottom:1px solid var(--stroke)">
        <summary class="row" style="padding:10px 16px"><span class="pill ${i.sev === 'critical' ? 'critical' : i.sev === 'high' ? 'caution' : ''}" style="width:74px;text-align:center">${{ critical: 'Crash', high: 'Error', medium: 'Warning', low: 'Notice' }[i.sev]}</span>
          <span style="flex:1;min-width:0"><b>${esc(i.head.summary)}</b>${i.events.length > 1 ? ` <span class="faint">+${i.events.length - 1} related</span>` : ''}</span>
          <span class="caption num">${dateTime(i.head.time)}</span></summary>
        <div style="padding:4px 16px 14px 104px">${i.events.map((e) => `<div class="kv" style="margin-top:8px"><span class="k">${esc(e.source)} ${e.eventId}</span><span>${esc(e.summary)}<br><span class="caption">${esc(dateTime(e.time))}${e.detail ? ' · ' + esc(e.detail.slice(0, 260)) : ''}</span></span></div>`).join('')}
          ${HELP[i.head.category] ? `<div style="margin-top:10px"><a data-help="${HELP[i.head.category]}">What this means (Microsoft Learn)</a></div>` : ''}</div></details>`).join('')
        : '<div class="empty"><h2>Nothing in this range</h2><p>No crashes or GPU errors were logged.</p></div>';
      body.querySelectorAll('[data-help]').forEach((a) => (a.onclick = () => reveal(a.dataset.help)));
    };
    body.querySelectorAll('[data-f]').forEach((b) => (b.onclick = () => { S.filter = b.dataset.f; body.querySelectorAll('[data-f]').forEach((x) => x.classList.toggle('on', x === b)); list(); }));
    list();
  }

  function drawTimeline(host, ev) {
    const W = Math.max(600, host.clientWidth - 36);
    const L = 140; const rowH = 26; const top = 8;
    const H = top + ROWS.length * rowH + 26;
    const end = Date.now(); const start = end - S.days * 86400000;
    const x = (t) => L + ((t - start) / (end - start)) * (W - L - 8);
    let svg = `<svg viewBox="0 0 ${W} ${H}" height="${H}" role="img" aria-label="Timeline of crash and error events">`;
    ROWS.forEach(([, label], i) => {
      const y = top + i * rowH + rowH / 2;
      svg += `<text class="lbl" x="0" y="${y + 4}">${label}</text><line class="grid" x1="${L}" x2="${W - 8}" y1="${y}" y2="${y}"/>`;
    });
    // month ticks
    const d = new Date(start); d.setDate(1); d.setHours(0, 0, 0, 0);
    const step = S.days > 120 ? 1 : 0;
    while (d.getTime() < end) {
      if (d.getTime() >= start) {
        const tx = x(d.getTime());
        svg += `<line class="grid" x1="${tx}" x2="${tx}" y1="${top}" y2="${H - 22}"/><text class="tick" x="${tx + 3}" y="${H - 8}">${d.toLocaleDateString(undefined, { month: 'short' })}</text>`;
      }
      if (step) d.setMonth(d.getMonth() + 1); else { d.setDate(d.getDate() + 14); }
    }
    for (const e of ev) {
      const t = new Date(e.time).getTime();
      if (t < start) continue;
      if (e.category === 'driver-update' || e.category === 'windows-update') {
        svg += `<line class="${e.category === 'driver-update' ? 'hair-driver' : 'hair-update'}" x1="${x(t)}" x2="${x(t)}" y1="${top}" y2="${H - 22}"><title>${esc(e.summary)} · ${esc(dateTime(e.time))}</title></line>`;
      }
    }
    for (const e of ev) {
      const t = new Date(e.time).getTime();
      const ri = ROWS.findIndex(([c]) => c === e.category || (c === 'app-crash' && e.category === 'app-hang'));
      if (t < start || ri < 0) continue;
      const y = top + ri * rowH + rowH / 2;
      const cls = e.severity === 'critical' ? 'dot-crit' : e.severity === 'high' ? 'dot-high' : e.severity === 'medium' ? 'dot-med' : 'dot-low';
      svg += `<circle class="${cls}" cx="${x(t).toFixed(1)}" cy="${y}" r="${e.severity === 'critical' ? 4.5 : 4}"><title>${esc(e.summary)} · ${esc(dateTime(e.time))}</title></circle>`;
    }
    host.innerHTML = svg + '</svg>';
  }

  monitor();
  experiment();
  render();
  return () => clearTimeout(expTimer);
}
