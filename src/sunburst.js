// Canvas sunburst (DaisyDisk-style). Colour = category, lightness = depth. Click a ring to zoom, centre to go up.
import { CATS, category, size, esc } from './core.js';

function mix(hex, toward, t) {
  const p = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
  const [a, b] = [p(hex), p(toward)];
  return `rgb(${a.map((v, i) => Math.round(v + (b[i] - v) * t)).join(',')})`;
}

export class Sunburst {
  constructor(host, { onZoom, onUp, onHover }) {
    this.host = host;
    this.canvas = document.createElement('canvas');
    host.appendChild(this.canvas);
    this.tip = document.createElement('div');
    this.tip.className = 'tip';
    this.tip.hidden = true;
    document.body.appendChild(this.tip);
    this.onZoom = onZoom; this.onUp = onUp; this.onHover = onHover;
    this.arcs = [];
    this.hover = null;
    this.ro = new ResizeObserver(() => this.draw());
    this.ro.observe(host);
    this.canvas.addEventListener('mousemove', (e) => this.move(e));
    this.canvas.addEventListener('mouseleave', () => { this.hover = null; this.tip.hidden = true; this.draw(); this.onHover?.(null); });
    this.canvas.addEventListener('click', (e) => this.click(e));
    this.mq = matchMedia('(prefers-color-scheme: dark)');
    this.mq.addEventListener?.('change', () => this.draw());
  }
  destroy() { this.ro.disconnect(); this.tip.remove(); }

  set(data) { this.data = data; this.layout(); this.draw(); }

  layout() {
    const root = this.data.root;
    const levels = 5;
    this.arcs = [];
    const walk = (node, path, a0, a1, depth, cat) => {
      if (depth > levels || !node.children) return;
      const total = node.size || 1;
      let a = a0;
      for (const c of node.children) {
        const span = ((a1 - a0) * c.size) / total;
        if (span < 0.0045) { a += span; continue; }
        const cpath = `${path.replace(/\\$/, '')}\\${c.name}`;
        let ccat = null;
        if (!c.other) { const own = category(cpath); ccat = own === 'Other' && depth > 1 ? cat : own; }
        this.arcs.push({ node: c, path: cpath, a0: a, a1: a + span, depth, cat: ccat });
        if (!c.file && !c.other) walk(c, cpath, a, a + span, depth + 1, ccat);
        a += span;
      }
    };
    walk(root, this.data.path, -Math.PI / 2, Math.PI * 1.5, 1, category(this.data.path));
    this.levels = Math.max(1, ...this.arcs.map((a) => a.depth));
  }

  geom() {
    const w = this.host.clientWidth;
    const R = w / 2 - 6;
    const r0 = R * 0.26;
    const ring = (R - r0) / Math.max(this.levels, 3);
    return { w, R, r0, ring, cx: w / 2, cy: w / 2 };
  }

  draw() {
    if (!this.data) return;
    const dpr = devicePixelRatio || 1;
    const { w, r0, ring, cx, cy } = this.geom();
    this.canvas.width = w * dpr; this.canvas.height = w * dpr;
    const g = this.canvas.getContext('2d');
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, w);
    const cs = getComputedStyle(document.documentElement);
    const dark = cs.getPropertyValue('color-scheme').trim() === 'dark';
    const bgTo = dark ? '#202020' : '#ffffff';
    const gap = dark ? 'rgba(32,32,32,0.9)' : 'rgba(255,255,255,0.95)';
    const hovered = this.hover;
    for (const a of this.arcs) {
      const ri = r0 + (a.depth - 1) * ring;
      const ro = ri + ring - 1.5;
      let col;
      if (a.node.other) col = dark ? 'rgba(255,255,255,0.10)' : 'rgba(0,0,0,0.07)';
      else {
        const base = CATS[a.cat] || CATS.Other;
        col = mix(base, bgTo, Math.min(0.62, (a.depth - 1) * 0.14 + (a.node.file ? 0.12 : 0)));
      }
      const lit = hovered && (a === hovered || (a.path.startsWith(hovered.path + '\\')));
      g.globalAlpha = hovered && !lit ? 0.45 : 1;
      g.beginPath();
      g.arc(cx, cy, ro, a.a0, a.a1);
      g.arc(cx, cy, ri, a.a1, a.a0, true);
      g.closePath();
      g.fillStyle = col;
      g.fill();
      g.lineWidth = 1;
      g.strokeStyle = gap;
      g.stroke();
    }
    g.globalAlpha = 1;
    // centre
    g.beginPath();
    g.arc(cx, cy, r0 - 4, 0, Math.PI * 2);
    g.fillStyle = dark ? 'rgba(255,255,255,0.05)' : 'rgba(0,0,0,0.035)';
    g.fill();
    const text = cs.getPropertyValue('--text').trim() || '#000';
    const text2 = cs.getPropertyValue('--text-2').trim() || '#666';
    g.textAlign = 'center';
    g.fillStyle = text;
    const show = hovered ? hovered.node : this.data.root;
    g.font = `600 ${Math.max(16, r0 * 0.24)}px "Segoe UI Variable Display", "Segoe UI", sans-serif`;
    g.fillText(size(show.size), cx, cy + 2);
    g.font = `400 12px "Segoe UI Variable Text", "Segoe UI", sans-serif`;
    g.fillStyle = text2;
    const label = hovered ? hovered.node.name : (this.data.crumbs.length > 1 ? 'Click to go up' : this.data.root.name);
    g.fillText(label.length > 26 ? label.slice(0, 25) + '…' : label, cx, cy + 22);
  }

  hit(e) {
    const r = this.canvas.getBoundingClientRect();
    const { r0, ring, cx, cy } = this.geom();
    const x = e.clientX - r.left - cx; const y = e.clientY - r.top - cy;
    const d = Math.hypot(x, y);
    if (d < r0) return 'centre';
    let ang = Math.atan2(y, x);
    if (ang < -Math.PI / 2) ang += Math.PI * 2;
    const depth = Math.floor((d - r0) / ring) + 1;
    return this.arcs.find((a) => a.depth === depth && ang >= a.a0 && ang < a.a1) || null;
  }

  move(e) {
    const h = this.hit(e);
    const arc = h === 'centre' ? null : h;
    if (arc !== this.hover) { this.hover = arc; this.draw(); this.onHover?.(arc); }
    this.canvas.style.cursor = arc && !arc.node.file && !arc.node.other ? 'pointer' : h === 'centre' && this.data.crumbs.length > 1 ? 'pointer' : 'default';
    if (arc) {
      const parentSize = this.data.root.size || 1;
      this.tip.innerHTML = `<b>${esc(arc.node.name)}</b> · ${size(arc.node.size)} · ${((arc.node.size / parentSize) * 100).toFixed(1)}% of this folder`;
      this.tip.hidden = false;
      this.tip.style.left = `${e.clientX + 14}px`;
      this.tip.style.top = `${e.clientY + 14}px`;
    } else this.tip.hidden = true;
  }

  click(e) {
    const h = this.hit(e);
    if (h === 'centre') return this.onUp?.();
    if (h && h.node.id != null && !h.node.other) this.onZoom?.(h.node.id);
  }
}
