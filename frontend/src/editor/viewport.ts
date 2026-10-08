import { engine, type ChangeEvent } from '../engine/client';
import { editor, type Rect4 } from './editor';

export interface ViewState { zoom: number; x: number; y: number }
export interface ViewOptions {
  guides: boolean; frames: boolean; baseline: boolean; preview: boolean; threads: boolean; quality: 'fast' | 'typical' | 'high';
  overprint: boolean; plate: number | null; inkLimit: number | null; notes: boolean; changes: boolean; spelling: boolean;
}

interface Tile { key: string; level: number; i: number; j: number; rect: Rect4; bmp: ImageBitmap | null; stale: boolean; dirtyAt: number; used: number }

const T = 512;
const MAX_TILES = 180;
const MIN_ZOOM = 0.05;
const MAX_ZOOM = 40;

function intersects(a: Rect4, b: Rect4): boolean { return a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]; }

export class Viewport {
  view: ViewState = { zoom: 1, x: 0, y: 0 };
  opts: ViewOptions = { guides: true, frames: true, baseline: false, preview: false, threads: false, quality: 'high', overprint: false, plate: null, inkLimit: null, notes: true, changes: false, spelling: false };
  overlays: any = null;
  spell: number[][] = [];
  onView: (() => void) | null = null;
  private readonly views = new Map<number, ViewState>();
  private readonly caches = new Map<number, Map<string, Tile>>();
  private tiles = new Map<string, Tile>();
  private readonly tctx: CanvasRenderingContext2D;
  private readonly hctx: CanvasRenderingContext2D;
  private readonly octx: CanvasRenderingContext2D;
  private dpr = Math.max(1, window.devicePixelRatio || 1);
  private w = 1;
  private h = 1;
  private level = 1;
  private settle = 0;
  private inflight = false;
  private drawQueued = false;
  private overlayQueued = false;
  private overlayBusy = false;
  private overlayAgain = false;
  private uid: number | null = null;
  private bg: [number, number, number] = [28, 27, 29];
  private tick = 0;
  private pending = 0;

  constructor(readonly stage: HTMLElement, readonly tileCanvas: HTMLCanvasElement, readonly hiliteCanvas: HTMLCanvasElement, readonly overlayCanvas: HTMLCanvasElement) {
    this.tctx = tileCanvas.getContext('2d', { alpha: false }) as CanvasRenderingContext2D;
    this.hctx = hiliteCanvas.getContext('2d') as CanvasRenderingContext2D;
    this.octx = overlayCanvas.getContext('2d') as CanvasRenderingContext2D;
    new ResizeObserver(() => this.resize()).observe(stage);
    this.resize();
    editor.on('change', (e: ChangeEvent) => this.onChange(e));
    editor.on('layout', (p: { switched?: boolean } = {}) => this.onLayout(!!p.switched));
    editor.on('selection', () => this.refreshOverlays());
    editor.on('tool', () => this.refreshOverlays());
    matchMedia(`(resolution: ${this.dpr}dppx)`).addEventListener?.('change', () => { this.dpr = Math.max(1, window.devicePixelRatio || 1); this.resize(); });
  }

  get hasDoc(): boolean { return !!editor.layout && editor.hasDoc; }
  get scale(): number { return this.view.zoom * this.dpr; }
  get width(): number { return this.w; }
  get height(): number { return this.h; }

  readBackground(): void {
    const c = getComputedStyle(this.stage).getPropertyValue('--fd-pasteboard').trim() || getComputedStyle(this.stage).backgroundColor;
    const m = c.match(/#([0-9a-f]{6})/i);
    if (m) {
      const n = parseInt(m[1], 16);
      this.bg = [(n >> 16) & 255, (n >> 8) & 255, n & 255];
    } else {
      const r = c.match(/rgba?\((\d+)[,\s]+(\d+)[,\s]+(\d+)/);
      if (r) this.bg = [Number(r[1]), Number(r[2]), Number(r[3])];
    }
    this.invalidateAll();
  }

  private resize(): void {
    const r = this.stage.getBoundingClientRect();
    const w = Math.max(1, Math.round(r.width)), h = Math.max(1, Math.round(r.height));
    if (w === this.w && h === this.h && this.tileCanvas.width === Math.round(w * this.dpr)) return;
    this.w = w;
    this.h = h;
    for (const c of [this.tileCanvas, this.hiliteCanvas, this.overlayCanvas]) {
      c.width = Math.round(w * this.dpr);
      c.height = Math.round(h * this.dpr);
      c.style.width = `${w}px`;
      c.style.height = `${h}px`;
    }
    this.requestDraw();
    this.refreshOverlays();
  }

  toCanvas(sx: number, sy: number): [number, number] {
    return [this.view.x + sx / this.view.zoom, this.view.y + sy / this.view.zoom];
  }

  toScreen(cx: number, cy: number): [number, number] {
    return [(cx - this.view.x) * this.view.zoom, (cy - this.view.y) * this.view.zoom];
  }

  visibleRect(): Rect4 {
    return [this.view.x, this.view.y, this.view.x + this.w / this.view.zoom, this.view.y + this.h / this.view.zoom];
  }

  setView(v: ViewState, settle = true): void {
    const zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, v.zoom));
    this.view = { zoom, x: v.x, y: v.y };
    if (this.uid !== null) this.views.set(this.uid, { ...this.view });
    const want = this.scale;
    if (Math.abs(Math.log2(want / this.level)) > 1.05) this.level = want;
    if (settle) {
      window.clearTimeout(this.settle);
      this.settle = window.setTimeout(() => { if (Math.abs(this.level - this.scale) > 1e-6) { this.level = this.scale; this.requestDraw(); } }, 140);
    }
    this.requestDraw();
    this.refreshOverlays();
    this.onView?.();
  }

  zoomAt(factor: number, sx = this.w / 2, sy = this.h / 2): void {
    const [cx, cy] = this.toCanvas(sx, sy);
    const zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, this.view.zoom * factor));
    this.setView({ zoom, x: cx - sx / zoom, y: cy - sy / zoom });
  }

  zoomTo(zoom: number): void { this.zoomAt(zoom / this.view.zoom); }

  panBy(dx: number, dy: number): void {
    this.setView({ zoom: this.view.zoom, x: this.view.x - dx / this.view.zoom, y: this.view.y - dy / this.view.zoom }, false);
  }

  fitRect(r: Rect4, pad = 32): void {
    const w = Math.max(1, r[2] - r[0]), h = Math.max(1, r[3] - r[1]);
    const zoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM, Math.min((this.w - pad * 2) / w, (this.h - pad * 2) / h)));
    this.setView({ zoom, x: r[0] - (this.w / zoom - w) / 2, y: r[1] - (this.h / zoom - h) / 2 });
  }

  currentSpread(): number {
    const lay = editor.layout;
    if (!lay || !lay.slots.length) return 0;
    const [cx, cy] = this.toCanvas(this.w / 2, this.h / 2);
    let best = 0, bd = Infinity;
    lay.slots.forEach((s, i) => {
      const b = s.bounds;
      const dx = cx < b[0] ? b[0] - cx : cx > b[2] ? cx - b[2] : 0;
      const dy = cy < b[1] ? b[1] - cy : cy > b[3] ? cy - b[3] : 0;
      const d = dx * dx + dy * dy;
      if (d < bd) { bd = d; best = i; }
    });
    return best;
  }

  currentPage(): number {
    const lay = editor.layout;
    if (!lay || !lay.pages.length) return 0;
    const [cx, cy] = this.toCanvas(this.w / 2, this.h / 2);
    let best = 0, bd = Infinity;
    for (const p of lay.pages) {
      const r = p.rect;
      const dx = cx < r[0] ? r[0] - cx : cx > r[2] ? cx - r[2] : 0;
      const dy = cy < r[1] ? r[1] - cy : cy > r[3] ? cy - r[3] : 0;
      const d = dx * dx + dy * dy;
      if (d < bd) { bd = d; best = p.abs; }
    }
    return best;
  }

  reveal(r: Rect4, pad = 48): void {
    const vr = this.visibleRect(), m = pad / this.view.zoom;
    if (r[0] >= vr[0] + m && r[1] >= vr[1] + m && r[2] <= vr[2] - m && r[3] <= vr[3] - m) return;
    if (r[2] - r[0] > vr[2] - vr[0] - 2 * m || r[3] - r[1] > vr[3] - vr[1] - 2 * m) { this.fitRect(r, pad); return; }
    const cx = (r[0] + r[2]) / 2, cy = (r[1] + r[3]) / 2;
    this.setView({ zoom: this.view.zoom, x: cx - this.w / 2 / this.view.zoom, y: cy - this.h / 2 / this.view.zoom });
  }

  fitSpread(index = this.currentSpread()): void {
    const s = editor.layout?.slots[index];
    if (s) this.fitRect(s.bounds);
  }

  fitPage(abs = this.currentPage()): void {
    const p = editor.layout?.pages.find((x) => x.abs === abs);
    if (p) this.fitRect(p.rect);
  }

  showPage(abs: number): void {
    const p = editor.layout?.pages.find((x) => x.abs === abs);
    if (!p) return;
    const vr = this.visibleRect();
    if (p.rect[2] - p.rect[0] <= vr[2] - vr[0] && p.rect[3] - p.rect[1] <= vr[3] - vr[1]) {
      const cx = (p.rect[0] + p.rect[2]) / 2, cy = (p.rect[1] + p.rect[3]) / 2;
      this.setView({ zoom: this.view.zoom, x: cx - this.w / 2 / this.view.zoom, y: cy - this.h / 2 / this.view.zoom });
    } else this.fitPage(abs);
  }

  fitAll(): void {
    const lay = editor.layout;
    if (!lay || !lay.slots.length) return;
    const r: Rect4 = [Infinity, Infinity, -Infinity, -Infinity];
    for (const s of lay.slots) { r[0] = Math.min(r[0], s.bounds[0]); r[1] = Math.min(r[1], s.bounds[1]); r[2] = Math.max(r[2], s.bounds[2]); r[3] = Math.max(r[3], s.bounds[3]); }
    this.fitRect(r);
  }

  private onLayout(switched: boolean): void {
    const doc = editor.doc;
    if (switched || (doc && doc.uid !== this.uid)) {
      if (this.uid !== null) { this.views.set(this.uid, { ...this.view }); this.caches.set(this.uid, this.tiles); }
      this.uid = doc ? doc.uid : null;
      const open = new Set(editor.docs.map((d) => d.uid));
      for (const [uid, cache] of this.caches) {
        if (!open.has(uid)) { for (const t of cache.values()) t.bmp?.close(); this.caches.delete(uid); this.views.delete(uid); }
      }
      this.tiles = (this.uid !== null ? this.caches.get(this.uid) : undefined) ?? new Map();
      const saved = this.uid !== null ? this.views.get(this.uid) : undefined;
      if (saved) this.setView(saved);
      else if (editor.layout) { this.level = this.scale; this.fitSpread(0); }
      else this.requestDraw();
      return;
    }
    this.invalidateAll();
  }

  private onChange(e: ChangeEvent): void {
    if (e.docChanged) {
      if (e.damage === null || e.damage === undefined) this.invalidateAll();
      else for (const d of e.damage) this.invalidate([d[0] - 2, d[1] - 2, d[2] + 2, d[3] + 2]);
    }
    this.refreshOverlays();
  }

  invalidateAll(): void {
    const now = engine.rev;
    for (const t of this.tiles.values()) { t.stale = true; t.dirtyAt = now; }
    this.requestDraw();
  }

  private invalidate(r: Rect4): void {
    const now = engine.rev;
    for (const t of this.tiles.values()) if (intersects(t.rect, r)) { t.stale = true; t.dirtyAt = now; }
    this.requestDraw();
  }

  requestDraw(): void {
    if (this.drawQueued) return;
    this.drawQueued = true;
    requestAnimationFrame(() => { this.drawQueued = false; this.draw(); this.pump(); });
  }

  private tileRect(level: number, i: number, j: number): Rect4 {
    return [i * T / level, j * T / level, (i + 1) * T / level, (j + 1) * T / level];
  }

  private needed(): Array<{ i: number; j: number; d: number }> {
    const vr = this.visibleRect();
    const L = this.level;
    const lay = editor.layout;
    const out: Array<{ i: number; j: number; d: number }> = [];
    if (!lay) return out;
    const i0 = Math.floor(vr[0] * L / T), i1 = Math.floor(vr[2] * L / T);
    const j0 = Math.floor(vr[1] * L / T), j1 = Math.floor(vr[3] * L / T);
    const cx = (vr[0] + vr[2]) / 2 * L / T, cy = (vr[1] + vr[3]) / 2 * L / T;
    for (let j = j0; j <= j1; j++) {
      for (let i = i0; i <= i1; i++) {
        const r = this.tileRect(L, i, j);
        if (!lay.slots.some((s) => intersects(r, [s.bounds[0] - 40, s.bounds[1] - 40, s.bounds[2] + 40, s.bounds[3] + 40]))) continue;
        out.push({ i, j, d: (i + 0.5 - cx) ** 2 + (j + 0.5 - cy) ** 2 });
      }
    }
    return out.sort((a, b) => a.d - b.d);
  }

  private draw(): void {
    const ctx = this.tctx;
    const s = this.scale;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = `rgb(${this.bg[0]},${this.bg[1]},${this.bg[2]})`;
    ctx.fillRect(0, 0, this.tileCanvas.width, this.tileCanvas.height);
    if (!this.hasDoc) { this.drawOverlays(); return; }
    const vr = this.visibleRect();
    const cur = this.level;
    this.tick++;
    ctx.fillStyle = '#ffffff';
    for (const p of editor.layout?.pages ?? []) {
      if (!intersects(p.rect, vr)) continue;
      const x0 = Math.round((p.rect[0] - this.view.x) * s), y0 = Math.round((p.rect[1] - this.view.y) * s);
      ctx.fillRect(x0, y0, Math.round((p.rect[2] - this.view.x) * s) - x0, Math.round((p.rect[3] - this.view.y) * s) - y0);
    }
    const others = [...this.tiles.values()].filter((t) => t.bmp && t.level !== cur && intersects(t.rect, vr))
      .sort((a, b) => Math.abs(Math.log(b.level / s)) - Math.abs(Math.log(a.level / s)));
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = 'high';
    for (const t of others) this.blit(t);
    for (const t of this.tiles.values()) {
      if (t.level === cur && t.bmp && intersects(t.rect, vr)) { this.blit(t); t.used = this.tick; }
    }
    this.drawOverlays();
  }

  private blit(t: Tile): void {
    if (!t.bmp) return;
    const s = this.scale;
    const x0 = Math.round((t.rect[0] - this.view.x) * s), y0 = Math.round((t.rect[1] - this.view.y) * s);
    const x1 = Math.round((t.rect[2] - this.view.x) * s), y1 = Math.round((t.rect[3] - this.view.y) * s);
    this.tctx.drawImage(t.bmp, x0, y0, x1 - x0, y1 - y0);
  }

  private async pump(): Promise<void> {
    if (this.inflight || !this.hasDoc || !engine.connected) return;
    const L = this.level;
    const want = this.needed().filter(({ i, j }) => {
      const t = this.tiles.get(`${L}|${i}|${j}`);
      return !t || !t.bmp || t.stale;
    });
    if (!want.length) { this.evict(); return; }
    const batch = want.slice(0, this.scale > 3 ? 2 : 4);
    this.inflight = true;
    const uid = this.uid;
    const rev = engine.rev;
    const req = batch.map(({ i, j }) => ({ key: `${L}|${i}|${j}`, s: L, ox: i * T / L, oy: j * T / L, w: T, h: T }));
    for (const r of req) {
      if (!this.tiles.has(r.key)) {
        const [, i, j] = r.key.split('|').map(Number);
        this.tiles.set(r.key, { key: r.key, level: L, i, j, rect: this.tileRect(L, i, j), bmp: null, stale: false, dirtyAt: rev, used: this.tick });
      }
    }
    try {
      const rep = await engine.callBinary<{ tiles: Array<{ key: string; w: number; h: number; offset: number; len: number }> }>('tiles', {
        tiles: req, bg: this.bg, preview: this.opts.preview, quality: this.opts.quality, overprint: this.opts.overprint,
        notes: this.opts.notes, changes: this.opts.changes, ...(this.opts.plate != null ? { plate: this.opts.plate } : {}),
        ...(this.opts.inkLimit ? { inkLimit: this.opts.inkLimit } : {}),
      });
      if (uid !== this.uid) return;
      const bytes = new Uint8ClampedArray(rep.data);
      await Promise.all(rep.result.tiles.map(async (info) => {
        const t = this.tiles.get(String(info.key));
        if (!t) return;
        const px = bytes.slice(info.offset, info.offset + info.len);
        const bmp = await createImageBitmap(new ImageData(px, info.w, info.h));
        t.bmp?.close();
        t.bmp = bmp;
        t.stale = t.dirtyAt > rev;
        t.used = this.tick;
      }));
      this.pending = 0;
    } catch {
      this.pending++;
    } finally {
      this.inflight = false;
    }
    if (this.pending > 3) { this.pending = 0; return; }
    this.requestDraw();
  }

  private evict(): void {
    if (this.tiles.size <= MAX_TILES) return;
    const vr = this.visibleRect();
    const list = [...this.tiles.values()].filter((t) => !(t.level === this.level && intersects(t.rect, vr))).sort((a, b) => a.used - b.used);
    for (const t of list.slice(0, this.tiles.size - MAX_TILES)) { t.bmp?.close(); this.tiles.delete(t.key); }
  }

  refreshOverlays(): void {
    if (this.overlayQueued) return;
    this.overlayQueued = true;
    requestAnimationFrame(() => { this.overlayQueued = false; void this.fetchOverlays(); });
  }

  private async fetchOverlays(): Promise<void> {
    if (this.overlayBusy) { this.overlayAgain = true; return; }
    if (!this.hasDoc || !engine.connected) { this.overlays = null; this.drawOverlays(); return; }
    this.overlayBusy = true;
    try {
      const vr = this.visibleRect();
      this.overlays = (await engine.call('overlays', { zoom: this.view.zoom, rect: vr })).result;
      this.spell = this.opts.spelling ? ((await engine.call<{ lines: number[][] }>('spellMarks', { rect: vr })).result?.lines ?? []) : [];
    } catch { }
    this.overlayBusy = false;
    this.drawOverlays();
    if (this.overlayAgain) { this.overlayAgain = false; this.refreshOverlays(); }
  }

  drawOverlays(): void {
    const h = this.hctx, o = this.octx;
    h.setTransform(1, 0, 0, 1, 0, 0);
    o.setTransform(1, 0, 0, 1, 0, 0);
    h.clearRect(0, 0, this.hiliteCanvas.width, this.hiliteCanvas.height);
    o.clearRect(0, 0, this.overlayCanvas.width, this.overlayCanvas.height);
    const ov = this.overlays;
    if (!ov || ov.empty || !this.hasDoc) return;
    const s = this.scale, vx = this.view.x, vy = this.view.y, dpr = this.dpr;
    const X = (x: number) => (x - vx) * s, Y = (y: number) => (y - vy) * s;
    const line = (ctx: CanvasRenderingContext2D, ax: number, ay: number, bx: number, by: number) => {
      ctx.beginPath(); ctx.moveTo(Math.round(X(ax)) + 0.5, Math.round(Y(ay)) + 0.5); ctx.lineTo(Math.round(X(bx)) + 0.5, Math.round(Y(by)) + 0.5); ctx.stroke();
    };
    const poly = (ctx: CanvasRenderingContext2D, p: number[], close = false) => {
      if (p.length < 4) return;
      ctx.beginPath();
      ctx.moveTo(X(p[0]), Y(p[1]));
      for (let k = 2; k < p.length; k += 2) ctx.lineTo(X(p[k]), Y(p[k + 1]));
      if (close) ctx.closePath();
      ctx.stroke();
    };
    const rect = (ctx: CanvasRenderingContext2D, r: number[]) => {
      ctx.strokeRect(Math.round(X(r[0])) + 0.5, Math.round(Y(r[1])) + 0.5, Math.round((r[2] - r[0]) * s), Math.round((r[3] - r[1]) * s));
    };
    const preview = this.opts.preview;
    const c = ov.colors ?? {};
    h.lineWidth = dpr;
    if (!preview && this.opts.guides) {
      h.strokeStyle = c.bleed ?? '#ff3b30';
      for (const r of ov.bleed ?? []) rect(h, r);
      h.strokeStyle = c.margin ?? '#d946ef';
      for (const r of ov.margins ?? []) rect(h, r);
      h.strokeStyle = c.column ?? '#8b5cf6';
      for (const l of ov.columns ?? []) line(h, l[0], l[1], l[2], l[3]);
      if (this.opts.baseline) {
        h.strokeStyle = c.baseline ?? '#7dd3fc';
        h.globalAlpha = 0.7;
        for (const l of ov.baseline ?? []) line(h, l[0], l[1], l[2], l[3]);
        h.globalAlpha = 1;
      }
      h.strokeStyle = c.guide ?? '#4ae3ff';
      for (const g of ov.guides ?? []) line(h, g.a[0], g.a[1], g.b[0], g.b[1]);
    }
    if (!preview && this.opts.spelling && this.spell.length) {
      h.strokeStyle = getComputedStyle(this.stage).getPropertyValue('--danger').trim() || 'red';
      h.lineWidth = Math.max(1, dpr);
      const amp = 1.4 * dpr, step = 2.5 * dpr;
      for (const l of this.spell) {
        const ax = X(l[0]), ay = Y(l[1]), bx = X(l[2]), by = Y(l[3]);
        const len = Math.hypot(bx - ax, by - ay);
        if (len < 2) continue;
        const ux = (bx - ax) / len, uy = (by - ay) / len;
        h.beginPath();
        h.moveTo(ax, ay);
        for (let d = step, k = 1; d <= len; d += step, k++) { const o = k % 2 ? amp : -amp; h.lineTo(ax + ux * d - uy * o, ay + uy * d + ux * o); }
        h.stroke();
      }
    }
    if (!preview && this.opts.frames) {
      h.globalAlpha = 0.55;
      h.setLineDash([2 * dpr, 2 * dpr]);
      for (const f of ov.parentFrames ?? []) { h.strokeStyle = f.color; for (const p of f.polys) poly(h, p); }
      h.setLineDash([]);
      for (const f of ov.frames ?? []) {
        h.strokeStyle = f.color;
        for (const p of f.polys) poly(h, p);
        if (f.cross) { line(h, f.cross[0][0], f.cross[0][1], f.cross[1][0], f.cross[1][1]); line(h, f.cross[2][0], f.cross[2][1], f.cross[3][0], f.cross[3][1]); }
      }
      h.globalAlpha = 1;
    }
    const t = ov.text;
    if (t?.quads?.length) {
      h.fillStyle = 'rgba(80, 150, 255, 0.32)';
      for (const q of t.quads) {
        h.beginPath(); h.moveTo(X(q[0]), Y(q[1])); h.lineTo(X(q[2]), Y(q[3])); h.lineTo(X(q[4]), Y(q[5])); h.lineTo(X(q[6]), Y(q[7])); h.closePath(); h.fill();
      }
    }
    const sel = ov.selection;
    if (sel && sel.count > 0) {
      const col = sel.color ?? '#4f99ff';
      o.strokeStyle = col;
      o.lineWidth = dpr;
      for (const p of sel.outlines ?? []) poly(o, p);
      if (sel.contentBox) { o.setLineDash([3 * dpr, 2 * dpr]); o.strokeStyle = '#d4a017'; rect(o, sel.contentBox); o.setLineDash([]); o.strokeStyle = col; }
      const b = sel.bounds;
      if (b && !sel.direct && !t?.caret) {
        rect(o, b);
        const hs = 3.5 * dpr;
        const pts = [[b[0], b[1]], [(b[0] + b[2]) / 2, b[1]], [b[2], b[1]], [b[2], (b[1] + b[3]) / 2], [b[2], b[3]], [(b[0] + b[2]) / 2, b[3]], [b[0], b[3]], [b[0], (b[1] + b[3]) / 2]];
        o.fillStyle = '#ffffff';
        for (const [x, y] of pts) {
          const px = Math.round(X(x)), py = Math.round(Y(y));
          o.fillRect(px - hs, py - hs, hs * 2, hs * 2);
          o.strokeRect(px - hs + 0.5, py - hs + 0.5, hs * 2 - 1, hs * 2 - 1);
        }
      }
      for (const port of sel.ports ?? []) {
        const ps = 5 * dpr;
        for (const [pt, linked, isOut] of [[port.in, port.inLinked, false], [port.out, port.outLinked, true]] as Array<[number[], boolean, boolean]>) {
          const px = Math.round(X(pt[0]) + (isOut ? -ps * 2.6 : ps * 0.6)), py = Math.round(Y(pt[1]) + (isOut ? -ps * 2.6 : ps * 0.6));
          o.fillStyle = '#ffffff';
          o.fillRect(px, py, ps * 2, ps * 2);
          o.strokeStyle = port.color ?? col;
          o.strokeRect(px + 0.5, py + 0.5, ps * 2 - 1, ps * 2 - 1);
          if (isOut && port.overset) {
            o.fillStyle = '#e11d48';
            o.fillRect(px + ps * 0.5, py + ps * 0.5, ps, ps);
          } else if (linked) {
            o.fillStyle = port.color ?? col;
            o.beginPath(); o.moveTo(px + ps * 0.6, py + ps * 0.5); o.lineTo(px + ps * 1.5, py + ps); o.lineTo(px + ps * 0.6, py + ps * 1.5); o.closePath(); o.fill();
          }
        }
      }
      if (this.opts.threads || sel.threads?.length) {
        o.strokeStyle = col;
        for (const l of sel.threads ?? []) line(o, l[0], l[1], l[2], l[3]);
      }
      for (const a of sel.anchors ?? []) {
        o.strokeStyle = col;
        for (const hnd of [a.in, a.out]) if (hnd) { line(o, a.p[0], a.p[1], hnd[0], hnd[1]); o.beginPath(); o.arc(X(hnd[0]), Y(hnd[1]), 2.5 * dpr, 0, Math.PI * 2); o.fillStyle = col; o.fill(); }
        const ax = Math.round(X(a.p[0])), ay = Math.round(Y(a.p[1])), r = 3 * dpr;
        o.fillStyle = a.picked ? col : '#ffffff';
        o.fillRect(ax - r, ay - r, r * 2, r * 2);
        o.strokeRect(ax - r + 0.5, ay - r + 0.5, r * 2 - 1, r * 2 - 1);
      }
    }
    if (t?.caret && editor.wantsText) {
      o.strokeStyle = '#111111';
      o.lineWidth = Math.max(1, dpr);
      line(o, t.caret[0], t.caret[1], t.caret[2], t.caret[3]);
    }
    for (const it of ov.tool ?? []) {
      o.lineWidth = dpr;
      o.strokeStyle = it.color ?? '#4f99ff';
      o.setLineDash(it.dashed ? [4 * dpr, 3 * dpr] : []);
      if (it.kind === 'marquee') { o.setLineDash([4 * dpr, 3 * dpr]); o.strokeStyle = '#4f99ff'; rect(o, it.rect); }
      else if (it.kind === 'path') for (const p of it.polys) poly(o, p);
      else if (it.kind === 'line' || it.kind === 'guide' || it.kind === 'gap') { if (it.kind === 'guide') o.strokeStyle = '#4ae3ff'; line(o, it.a[0], it.a[1], it.b[0], it.b[1]); }
      if ((it.kind === 'measure' || it.kind === 'gap') && (it.text || it.label)) {
        const p = it.p ?? [(it.a[0] + it.b[0]) / 2, (it.a[1] + it.b[1]) / 2];
        this.label(o, String(it.text ?? it.label), X(p[0]) + 12 * dpr, Y(p[1]) + 16 * dpr);
      }
      o.setLineDash([]);
    }
  }

  private label(o: CanvasRenderingContext2D, text: string, x: number, y: number): void {
    const dpr = this.dpr;
    o.font = `${11 * dpr}px Inter, system-ui, sans-serif`;
    const w = o.measureText(text).width + 10 * dpr;
    o.fillStyle = 'rgba(20, 20, 22, 0.86)';
    o.fillRect(x, y - 14 * dpr, w, 19 * dpr);
    o.fillStyle = '#ffffff';
    o.fillText(text, x + 5 * dpr, y);
  }
}
