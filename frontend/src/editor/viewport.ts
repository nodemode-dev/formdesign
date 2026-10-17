import { engine, type ChangeEvent } from '../engine/client';
import { editor, type Rect4 } from './editor';
import { Canvas2dCompositor } from '../gpu/canvas2d';
import { inTauri, type Compositor, type Frame, type FrameTile, type Preference } from '../gpu/index';
import { chainFor, selectCompositor, type CanvasFactory, type PathId } from '../gpu/select';

const RENDERER_KEY = 'formdes.renderer';
const PATH_LABEL: Record<PathId, string> = { native: 'Native', webgpu: 'WebGPU', webgl2: 'WebGL2', canvas2d: 'Canvas 2D' };
/** Set on the root while the native canvas is showing through the page's transparent stage. */
const NATIVE_CLASS = 'is-native-canvas';

export interface ViewState { zoom: number; x: number; y: number }
export interface ViewOptions {
  guides: boolean; frames: boolean; baseline: boolean; preview: boolean; threads: boolean; quality: 'fast' | 'typical' | 'high';
  overprint: boolean; plate: number | null; inkLimit: number | null; notes: boolean; changes: boolean; spelling: boolean;
  proof: { target: string; intent: string; simulatePaper: boolean } | null;
}

interface Tile { key: string; gkey: string; level: number; i: number; j: number; rect: Rect4; bmp: ImageBitmap | null; stale: boolean; dirtyAt: number; used: number }

const T = 512;
const MAX_TILES = 180;
const MIN_ZOOM = 0.05;
const MAX_ZOOM = 40;

function intersects(a: Rect4, b: Rect4): boolean { return a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]; }

export class Viewport {
  view: ViewState = { zoom: 1, x: 0, y: 0 };
  opts: ViewOptions = { guides: true, frames: true, baseline: false, preview: false, threads: false, quality: 'high', overprint: false, plate: null, inkLimit: null, notes: true, changes: false, spelling: false, proof: null };
  overlays: any = null;
  spell: number[][] = [];
  onView: (() => void) | null = null;
  private readonly views = new Map<number, ViewState>();
  private readonly caches = new Map<number, Map<string, Tile>>();
  private tiles = new Map<string, Tile>();
  /** The display path in use, and the Canvas 2D one on the page's own tile canvas, which is always kept ready as the fallback. */
  private comp: Compositor;
  private readonly base: Canvas2dCompositor;
  private activeId: PathId = 'canvas2d';
  private rendererPref: Preference = 'auto';
  private swapToken = 0;
  /** While the native canvas has not shown its first full frame: the path that is still on screen and still drawn. */
  private nativeWait: Compositor | null = null;
  private snapshotTimer = 0;
  private rendererNote = '';
  private readonly rendererListeners: Array<() => void> = [];
  private readonly extraCanvases = new Set<HTMLCanvasElement>();
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
    // Canvas2dCompositor.init does its work before its first await, so the page draws from the first frame as before.
    this.base = new Canvas2dCompositor();
    void this.base.init(tileCanvas);
    this.comp = this.base;
    this.hctx = hiliteCanvas.getContext('2d') as CanvasRenderingContext2D;
    this.octx = overlayCanvas.getContext('2d') as CanvasRenderingContext2D;
    new ResizeObserver(() => this.resize()).observe(stage);
    this.resize();
    editor.on('change', (e: ChangeEvent) => this.onChange(e));
    editor.on('layout', (p: { switched?: boolean } = {}) => this.onLayout(!!p.switched));
    editor.on('selection', () => this.refreshOverlays());
    editor.on('tool', () => this.refreshOverlays());
    matchMedia(`(resolution: ${this.dpr}dppx)`).addEventListener?.('change', () => { this.dpr = Math.max(1, window.devicePixelRatio || 1); this.resize(); });
    try { const p = localStorage.getItem(RENDERER_KEY); if (p === 'auto' || p === 'webgpu' || p === 'webgl2' || p === 'canvas2d' || (p === 'native' && inTauri())) this.rendererPref = p; } catch { }
    // The desktop shell can start the page on the native canvas (FORMDES_RENDERER=native), ahead of the remembered choice.
    if (inTauri() && (window as unknown as { __FORMDES_RENDERER__?: string }).__FORMDES_RENDERER__ === 'native') this.rendererPref = 'native';
    if (this.rendererPref !== 'canvas2d') void this.chooseRenderer(this.rendererPref);
    else this.notifyRenderer();
  }

  // The display path. Canvas 2D is on the page's own tile canvas and always alive; WebGPU and WebGL2 each draw on a canvas of
  // their own that is inserted once, under the highlight and overlay canvases, when that path is first chosen. A switch builds
  // the new path beside the old one, uploads the tiles it holds, draws a full frame into it and only then hides the old
  // canvas, all in one task, so the next paint already shows the new frame.
  get renderer(): { pref: Preference; id: PathId; name: string; note: string } {
    return { pref: this.rendererPref, id: this.activeId, name: this.comp.name, note: this.rendererNote };
  }

  onRenderer(fn: () => void): void { this.rendererListeners.push(fn); }

  private notifyRenderer(): void { for (const fn of this.rendererListeners) fn(); }

  setRenderer(pref: Preference): Promise<void> {
    this.rendererPref = pref;
    try { localStorage.setItem(RENDERER_KEY, pref); } catch { }
    return this.chooseRenderer(pref);
  }

  private factory(): CanvasFactory {
    return {
      create: (id) => {
        if (id === 'canvas2d') return this.tileCanvas;
        const c = document.createElement('canvas');
        c.className = `cv-canvas fd-tiles-gpu fd-tiles-${id}`;
        c.setAttribute('aria-hidden', 'true');
        c.style.visibility = 'hidden';
        this.tileCanvas.after(c);
        this.extraCanvases.add(c);
        return c;
      },
      discard: (c) => { this.extraCanvases.delete(c); c.remove(); },
    };
  }

  private async chooseRenderer(pref: Preference, after?: PathId): Promise<void> {
    if (!after && chainFor(pref)[0] === this.activeId && this.comp.canvas) { this.rendererNote = ''; this.notifyRenderer(); return; }
    const token = ++this.swapToken;
    const sel = await selectCompositor(pref, this.factory(), after, new Map([['canvas2d', this.base]]));
    if (token !== this.swapToken) {
      // A newer choice superseded this one while it was starting.
      if (sel && sel.compositor !== this.base && sel.compositor !== this.comp) this.dispose(sel.compositor);
      return;
    }
    if (!sel) return;
    const want = chainFor(pref)[0];
    this.rendererNote = sel.id !== want && pref !== 'auto' ? `${PATH_LABEL[want]} is not available here` : '';
    this.adopt(sel.compositor, sel.id);
  }

  private adopt(next: Compositor, id: PathId): void {
    if (next === this.comp) { this.activeId = id; this.notifyRenderer(); return; }
    const prev = this.comp;
    next.resize(this.w, this.h, this.dpr);
    this.uploadAll(next);
    next.onLost = (why) => this.onPathLost(next, id, why);
    if (id === 'native') {
      // The native canvas draws in the window beneath the web view. The previous path keeps drawing and stays visible until
      // the native one has shown a full frame; then the stage turns transparent in one paint.
      const wait = this.nativeWait ?? prev;
      this.nativeWait = wait;
      next.onShown = () => {
        if (this.comp !== next) return;
        const held = this.nativeWait;
        this.nativeWait = null;
        document.documentElement.classList.add(NATIVE_CLASS);
        this.tileCanvas.style.visibility = 'hidden';
        for (const c of this.extraCanvases) if (c !== next.canvas) c.style.visibility = 'hidden';
        if (held && held !== this.base && held !== next) window.setTimeout(() => this.dispose(held), 120);
      };
      this.comp = next;
      this.activeId = id;
      this.draw();
      this.notifyRenderer();
      return;
    }
    const wasNative = prev.name === 'Native';
    if (wasNative) this.leaveNative();
    if (next.canvas) next.canvas.style.visibility = 'visible';
    this.comp = next;
    this.activeId = id;
    this.draw();
    // The new canvas has its frame; the old one can go. Both changes land in the same paint.
    if (prev !== this.base && prev.canvas && prev.canvas !== next.canvas) prev.canvas.style.visibility = 'hidden';
    if (prev !== this.base) window.setTimeout(() => this.dispose(prev), 120);
    this.notifyRenderer();
  }

  /** Back to the page's own canvases: the stage paints again and the held path (if the native one never showed) is let go. */
  private leaveNative(): void {
    document.documentElement.classList.remove(NATIVE_CLASS);
    this.tileCanvas.style.visibility = 'visible';
    const held = this.nativeWait;
    this.nativeWait = null;
    if (held && held !== this.base && held !== this.comp) window.setTimeout(() => this.dispose(held), 120);
  }

  private dispose(c: Compositor): void {
    const canvas = c.canvas;
    c.onLost = null;
    try { c.destroy(); } catch { }
    if (canvas && canvas !== this.tileCanvas) { this.extraCanvases.delete(canvas); canvas.remove(); }
  }

  private onPathLost(c: Compositor, id: PathId, why: string): void {
    if (c !== this.comp) return;
    // Bridge at once on Canvas 2D from the tiles already held, so the lost canvas is never what the next paint shows,
    // then try the paths after the lost one.
    const lostCanvas = c.canvas;
    if (id === 'native') this.leaveNative();
    this.rendererNote = `${PATH_LABEL[id]} lost (${why.slice(0, 60)})`;
    this.uploadAll(this.base);
    this.base.resize(this.w, this.h, this.dpr);
    this.comp = this.base;
    this.activeId = 'canvas2d';
    this.tileCanvas.style.visibility = 'visible';
    this.draw();
    if (lostCanvas) lostCanvas.style.visibility = 'hidden';
    window.setTimeout(() => this.dispose(c), 120);
    this.notifyRenderer();
    const note = this.rendererNote;
    void this.chooseRenderer(this.rendererPref, id).then(() => { if (this.activeId === 'canvas2d') this.rendererNote = note; this.notifyRenderer(); });
  }

  /** Drops a tile from every path that holds it (the base path only references the bitmap, which is about to close). */
  private forget(t: Tile): void {
    this.comp.evict(t.gkey);
    if (this.comp !== this.base) this.base.evict(t.gkey);
  }

  private uploadAll(c: Compositor): void {
    for (const cache of [...this.caches.values(), this.tiles]) {
      for (const t of cache.values()) if (t.bmp && !c.has(t.gkey)) c.upload(t.gkey, t.bmp);
    }
  }

  /** Test hook: asks the active GPU path to report itself lost, as a real device or context loss would. */
  simulateLoss(): void { const c = this.comp; if (c !== this.base) c.onLost?.('simulated'); }

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
    const ow = this.w, oh = this.h;
    this.w = w;
    this.h = h;
    for (const c of [this.hiliteCanvas, this.overlayCanvas]) {
      c.width = Math.round(w * this.dpr);
      c.height = Math.round(h * this.dpr);
      c.style.width = `${w}px`;
      c.style.height = `${h}px`;
    }
    this.base.resize(w, h, this.dpr);
    if (this.comp !== this.base) this.comp.resize(w, h, this.dpr);
    if (this.hasDoc && ow > 1 && oh > 1 && (ow !== w || oh !== h)) {
      const z = this.view.zoom;
      this.setView({ zoom: z, x: this.view.x + (ow - w) / 2 / z, y: this.view.y + (oh - h) / 2 / z }, false);
    }
    this.draw();
    this.drawOverlays();
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
        if (!open.has(uid)) { for (const t of cache.values()) { this.forget(t); t.bmp?.close(); } this.caches.delete(uid); this.views.delete(uid); }
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
    const s = this.scale;
    const frame: Frame = { width: Math.round(this.w * this.dpr), height: Math.round(this.h * this.dpr), dpr: this.dpr, bg: this.bg, scale: s, x: this.view.x, y: this.view.y, pages: [], tiles: [] };
    if (this.hasDoc) {
      const vr = this.visibleRect();
      const cur = this.level;
      this.tick++;
      for (const p of editor.layout?.pages ?? []) if (intersects(p.rect, vr)) frame.pages.push(p.rect);
      // Tiles of other levels first, the furthest from the view's scale first, then the current level on top.
      const others = [...this.tiles.values()].filter((t) => t.bmp && t.level !== cur && intersects(t.rect, vr))
        .sort((a, b) => Math.abs(Math.log(b.level / s)) - Math.abs(Math.log(a.level / s)));
      const ft = (t: Tile): FrameTile => ({ key: t.gkey, rect: t.rect, level: t.level });
      for (const t of others) frame.tiles.push(ft(t));
      for (const t of this.tiles.values()) {
        if (t.level === cur && t.bmp && intersects(t.rect, vr)) { frame.tiles.push(ft(t)); t.used = this.tick; }
      }
    }
    const missing = this.comp.draw(frame);
    if (missing.length) {
      // Tiles that arrived while another path was drawing: hand them to this one and draw again.
      const want = new Set(missing);
      for (const t of this.tiles.values()) if (t.bmp && want.has(t.gkey)) this.comp.upload(t.gkey, t.bmp);
      this.comp.draw(frame);
    }
    if (this.nativeWait) {
      // Native is not showing yet: the path still on screen keeps up with the view.
      const held = this.nativeWait;
      for (const t of this.tiles.values()) if (t.bmp && !held.has(t.gkey)) held.upload(t.gkey, t.bmp);
      held.draw(frame);
    }
    if (this.comp !== this.base && this.activeId !== 'native') this.snapshotLater(frame);
    this.drawOverlays();
  }

  // A lost GPU context or device blanks its canvas before the page hears about it, so the page's own Canvas 2D canvas
  // stays visible underneath the GPU canvases and is repainted with the latest frame whenever drawing goes quiet. If the
  // GPU canvas goes blank, what shows through is the last settled frame, never the empty stage.
  private snapshotLater(frame: Frame): void {
    window.clearTimeout(this.snapshotTimer);
    this.snapshotTimer = window.setTimeout(() => {
      if (this.comp === this.base) return;
      this.uploadAll(this.base);
      this.base.draw(frame);
    }, 160);
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
        this.tiles.set(r.key, { key: r.key, gkey: `${uid}|${r.key}`, level: L, i, j, rect: this.tileRect(L, i, j), bmp: null, stale: false, dirtyAt: rev, used: this.tick });
      }
    }
    try {
      const rep = await engine.callBinary<{ tiles: Array<{ key: string; w: number; h: number; offset: number; len: number }> }>('tiles', {
        tiles: req, bg: this.bg, preview: this.opts.preview, quality: this.opts.quality, overprint: this.opts.overprint,
        notes: this.opts.notes, changes: this.opts.changes, ...(this.opts.plate != null ? { plate: this.opts.plate } : {}),
        ...(this.opts.inkLimit ? { inkLimit: this.opts.inkLimit } : {}), ...(this.opts.proof ? { proof: this.opts.proof } : {}),
      });
      if (uid !== this.uid) return;
      const bytes = new Uint8ClampedArray(rep.data);
      await Promise.all(rep.result.tiles.map(async (info) => {
        const t = this.tiles.get(String(info.key));
        if (!t) return;
        const px = bytes.slice(info.offset, info.offset + info.len);
        const bmp = await createImageBitmap(new ImageData(px, info.w, info.h));
        if (this.comp !== this.base) this.base.evict(t.gkey);
        t.bmp?.close();
        t.bmp = bmp;
        this.comp.upload(t.gkey, bmp);
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
    for (const t of list.slice(0, this.tiles.size - MAX_TILES)) { this.forget(t); t.bmp?.close(); this.tiles.delete(t.key); }
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
    if (ov.isolated) {
      const r = ov.isolated;
      h.fillStyle = 'rgba(20, 20, 23, 0.5)';
      h.beginPath();
      h.rect(0, 0, this.hiliteCanvas.width, this.hiliteCanvas.height);
      h.rect(X(r[0]), Y(r[1]), (r[2] - r[0]) * s, (r[3] - r[1]) * s);
      h.fill('evenodd');
    }
    if (!preview && this.opts.guides) {
      h.strokeStyle = c.bleed ?? '#ff3b30';
      for (const r of ov.bleed ?? []) rect(h, r);
      h.strokeStyle = c.margin ?? '#d946ef';
      for (const r of ov.margins ?? []) rect(h, r);
      h.strokeStyle = '#9ca3af';
      for (const a of ov.artboards ?? []) {
        if (a.active) h.strokeStyle = '#4f99ff';
        rect(h, a.rect);
        if (a.active) {
          const hs = 3.5 * dpr;
          h.fillStyle = '#ffffff';
          for (const [x, y] of [[a.rect[0], a.rect[1]], [a.rect[2], a.rect[1]], [a.rect[2], a.rect[3]], [a.rect[0], a.rect[3]]]) {
            const px = Math.round(X(x)), py = Math.round(Y(y));
            h.fillRect(px - hs, py - hs, hs * 2, hs * 2);
            h.strokeRect(px - hs + 0.5, py - hs + 0.5, hs * 2 - 1, hs * 2 - 1);
          }
          h.strokeStyle = '#9ca3af';
        }
        if (a.name) this.label(h, String(a.name), X(a.rect[0]), Y(a.rect[1]) - 6 * dpr);
      }
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
    for (const m of ov.mesh ?? []) {
      const cols = Number(m.cols), rows = Number(m.rows), pts: number[][] = m.points ?? [];
      if (pts.length !== (cols + 1) * (rows + 1)) continue;
      const at = (i: number, j: number) => pts[j * (cols + 1) + i];
      h.strokeStyle = '#4f99ff';
      h.lineWidth = dpr;
      for (let j = 0; j <= rows; j++) for (let i = 0; i <= cols; i++) {
        if (i < cols) line(h, at(i, j)[0], at(i, j)[1], at(i + 1, j)[0], at(i + 1, j)[1]);
        if (j < rows) line(h, at(i, j)[0], at(i, j)[1], at(i, j + 1)[0], at(i, j + 1)[1]);
      }
      const hs = 3 * dpr;
      h.fillStyle = '#ffffff';
      for (const q of pts) {
        const px = Math.round(X(q[0])), py = Math.round(Y(q[1]));
        h.fillRect(px - hs, py - hs, hs * 2, hs * 2);
        h.strokeRect(px - hs + 0.5, py - hs + 0.5, hs * 2 - 1, hs * 2 - 1);
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
      for (const c of sel.corners ?? []) {
        const vx = X(c.v[0]), vy = Y(c.v[1]);
        const dx = X(c.p[0]) - vx, dy = Y(c.p[1]) - vy, len = Math.hypot(dx, dy);
        if (len < 1e-3) continue;
        const reach = Math.max(len, 14 * dpr);
        const cx = vx + (dx / len) * reach, cy = vy + (dy / len) * reach;
        o.strokeStyle = col;
        o.fillStyle = '#ffffff';
        o.beginPath(); o.arc(cx, cy, 4 * dpr, 0, Math.PI * 2); o.fill(); o.stroke();
        o.fillStyle = c.picked ? col : '#ffffff';
        o.beginPath(); o.arc(cx, cy, 1.6 * dpr, 0, Math.PI * 2); o.fill();
        if (!c.picked) { o.fillStyle = col; o.beginPath(); o.arc(cx, cy, 1 * dpr, 0, Math.PI * 2); o.fill(); }
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
