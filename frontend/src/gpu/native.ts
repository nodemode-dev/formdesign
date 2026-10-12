import { inTauri, type Compositor, type Frame, type Rect4, type TileSource } from './index';

// The native canvas of the desktop app (src-tauri/src/native.rs over native/formdes-gpu): the same frame model as the web
// paths, drawn by wgpu in the window beneath the web view. It exists only inside Tauri; in a browser init() says no and the
// selector moves on.
//
// The web viewport rasters 512 pixel tiles over the whole canvas at any scale; formdes-gpu composes 256 pixel tiles per
// page at power-of-two levels. This class bridges the two: it keeps the web tiles it was given, and for every native tile
// the frame needs it paints the web tiles that cover it into a 256 pixel canvas, reads the pixels back and sends them as raw
// bytes, again only when the tiles under it changed. Commands are sent one at a time in order, and only the newest queued
// frame is kept. The first complete frame (presented, nothing missing) fires onShown, after which the owner turns its web
// canvas region transparent; until then the web canvas stays, so the switch has no flash.

const TILE = 256;
const MIN_LEVEL = -8;
const MAX_LEVEL = 6;

type Invoke = (cmd: string, args?: unknown, options?: { headers?: Record<string, string> }) => Promise<any>;

function tauriInvoke(): Invoke | null {
  const w = window as unknown as { __TAURI_INTERNALS__?: { invoke?: Invoke }; __TAURI__?: { core?: { invoke?: Invoke } } };
  return w.__TAURI_INTERNALS__?.invoke ?? w.__TAURI__?.core?.invoke ?? null;
}

interface KeyOut { page: number; level: number; tx: number; ty: number }
interface FrameOut { presented: boolean; missing: KeyOut[]; evicted: KeyOut[] }
interface Source { src: TileSource; version: number }
type Job =
  | { kind: 'tile'; key: string; w: number; h: number; bytes: Uint8Array }
  | { kind: 'resize'; rect: Rect4; dpr: number }
  | { kind: 'frame'; payload: unknown };

const keyString = (k: KeyOut): string => `${k.page}|${k.level}|${k.tx}|${k.ty}`;

/** The level for a scale in device pixels per canvas unit: ceil(log2(scale)), clamped, as formdes-gpu picks it. */
export function nativeLevel(scale: number): number {
  return Math.min(MAX_LEVEL, Math.max(MIN_LEVEL, Math.ceil(Math.log2(Math.max(scale, 1e-6)) - 1e-3)));
}

export class NativeCompositor implements Compositor {
  readonly name = 'Native';
  canvas: HTMLCanvasElement | null = null;
  onLost: ((why: string) => void) | null = null;
  onShown: (() => void) | null = null;
  private invoke: Invoke | null = null;
  private readonly sources = new Map<string, Source>();
  private version = 0;
  private readonly pageIds = new Map<string, number>();
  /** Native tile key to the signature of the web tiles it was composed from. */
  private readonly sent = new Map<string, string>();
  private scratch: OffscreenCanvas | HTMLCanvasElement | null = null;
  private sctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null = null;
  private queue: Job[] = [];
  private running = false;
  private dead = false;
  private shown = false;
  private failures = 0;
  private rect: Rect4 = [0, 0, 1, 1];
  private dpr = 1;

  async init(canvas: HTMLCanvasElement): Promise<boolean> {
    if (!inTauri()) return false;
    const invoke = tauriInvoke();
    if (!invoke) return false;
    try {
      this.dpr = Math.max(1, window.devicePixelRatio || 1);
      this.rect = this.measure(canvas);
      const ok = await invoke('native_canvas_start', { rect: this.rect, dpr: this.dpr });
      if (ok !== true) return false;
    } catch {
      return false;
    }
    this.invoke = invoke;
    this.canvas = canvas;
    return true;
  }

  private measure(canvas: HTMLCanvasElement): Rect4 {
    const r = canvas.getBoundingClientRect();
    return [r.left, r.top, r.width, r.height];
  }

  upload(key: string, source: TileSource): void {
    if (this.dead) return;
    this.sources.set(key, { src: source, version: ++this.version });
  }

  has(key: string): boolean { return this.sources.has(key); }

  evict(key: string): void { this.sources.delete(key); }

  draw(frame: Frame): string[] {
    const invoke = this.invoke;
    if (!invoke || this.dead || !this.canvas) return [];
    this.syncRect(frame.dpr);
    const scale = frame.scale;
    const level = nativeLevel(scale);
    const k = 2 ** level;
    const r = scale / k;
    const pages: Array<{ index: number; x: number; y: number; w: number; h: number }> = [];
    const wanted: Array<{ key: string; page: number; tx: number; ty: number; w: number; h: number; rect: Rect4 }> = [];
    for (const p of frame.pages) {
      const index = this.pageId(p);
      const [pw, ph] = [p[2] - p[0], p[3] - p[1]];
      pages.push({ index, x: p[0], y: p[1], w: pw, h: ph });
      const dx = (p[0] - frame.x) * scale, dy = (p[1] - frame.y) * scale;
      if (dx >= frame.width || dy >= frame.height || dx + pw * scale <= 0 || dy + ph * scale <= 0) continue;
      const rw = Math.ceil(pw * k), rh = Math.ceil(ph * k);
      const nx = Math.ceil(rw / TILE), ny = Math.ceil(rh / TILE);
      const step = TILE * r;
      if (nx < 1 || ny < 1 || step <= 0) continue;
      const span = (origin: number, extent: number, n: number): [number, number] =>
        [Math.max(0, Math.floor(-origin / step)), Math.min(n, Math.ceil((extent - origin) / step))];
      const [i0, i1] = span(dx, frame.width, nx), [j0, j1] = span(dy, frame.height, ny);
      for (let ty = j0; ty < j1; ty++) {
        for (let tx = i0; tx < i1; tx++) {
          const w = Math.min(TILE, rw - tx * TILE), h = Math.min(TILE, rh - ty * TILE);
          const x0 = p[0] + tx * TILE / k, y0 = p[1] + ty * TILE / k;
          wanted.push({ key: `${index}|${level}|${tx}|${ty}`, page: index, tx, ty, w, h, rect: [x0, y0, x0 + w / k, y0 + h / k] });
        }
      }
    }
    for (const t of wanted) {
      let sig = '';
      for (const ft of frame.tiles) {
        const s = this.sources.get(ft.key);
        if (s && ft.rect[0] < t.rect[2] && ft.rect[2] > t.rect[0] && ft.rect[1] < t.rect[3] && ft.rect[3] > t.rect[1]) sig += `${ft.key}#${s.version};`;
      }
      if (!sig || this.sent.get(t.key) === sig) continue;
      const bytes = this.compose(frame, t.rect, k, t.w, t.h);
      if (!bytes) continue;
      this.sent.set(t.key, sig);
      this.queue.push({ kind: 'tile', key: t.key, w: t.w, h: t.h, bytes });
    }
    const css = scale / frame.dpr;
    const payload = {
      view: [css, -frame.x * css, -frame.y * css],
      pages,
      pasteboard: [frame.bg[0] / 255, frame.bg[1] / 255, frame.bg[2] / 255, 1],
    };
    // Only the newest frame waits in the queue; the tiles ahead of it stay.
    const at = this.queue.findIndex((j) => j.kind === 'frame');
    if (at >= 0) this.queue.splice(at, 1);
    this.queue.push({ kind: 'frame', payload });
    void this.run();
    return [];
  }

  /** Sends the canvas rectangle again when the stage moved or the pixel ratio changed. */
  private syncRect(dpr: number): void {
    if (!this.canvas || !this.invoke) return;
    const next = this.measure(this.canvas);
    const same = next.every((v, i) => Math.abs(v - this.rect[i]) < 0.01) && Math.abs(dpr - this.dpr) < 1e-6;
    if (same) return;
    this.rect = next;
    this.dpr = dpr;
    // Ordered with the tiles and frames around it.
    this.queue.push({ kind: 'resize', rect: next, dpr });
  }

  resize(_width: number, _height: number, dpr: number): void {
    if (this.canvas) this.syncRect(dpr);
  }

  private pageId(p: Rect4): number {
    const key = p.map((v) => Math.round(v * 1000)).join(',');
    let id = this.pageIds.get(key);
    if (id === undefined) { id = this.pageIds.size; this.pageIds.set(key, id); }
    return id;
  }

  private context(): CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null {
    if (this.sctx) return this.sctx;
    const c = typeof OffscreenCanvas !== 'undefined' ? new OffscreenCanvas(TILE, TILE) : Object.assign(document.createElement('canvas'), { width: TILE, height: TILE });
    this.scratch = c;
    this.sctx = c.getContext('2d', { willReadFrequently: true }) as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null;
    return this.sctx;
  }

  /** Paints the web tiles over a native tile's rectangle (older levels first, as the frame lists them) and reads the pixels. */
  private compose(frame: Frame, rect: Rect4, k: number, w: number, h: number): Uint8Array | null {
    const ctx = this.context();
    if (!ctx) return null;
    ctx.clearRect(0, 0, TILE, TILE);
    ctx.imageSmoothingEnabled = true;
    ctx.imageSmoothingQuality = 'high';
    for (const ft of frame.tiles) {
      const s = this.sources.get(ft.key);
      if (!s || !(ft.rect[0] < rect[2] && ft.rect[2] > rect[0] && ft.rect[1] < rect[3] && ft.rect[3] > rect[1])) continue;
      const x0 = Math.round((ft.rect[0] - rect[0]) * k), y0 = Math.round((ft.rect[1] - rect[1]) * k);
      const x1 = Math.round((ft.rect[2] - rect[0]) * k), y1 = Math.round((ft.rect[3] - rect[1]) * k);
      if (x1 <= x0 || y1 <= y0) continue;
      try {
        if (typeof ImageData !== 'undefined' && s.src instanceof ImageData) {
          // Rare (the viewport hands bitmaps); a one-off canvas keeps the scaling path the same.
          const tmp = document.createElement('canvas');
          tmp.width = s.src.width; tmp.height = s.src.height;
          tmp.getContext('2d')?.putImageData(s.src, 0, 0);
          ctx.drawImage(tmp, x0, y0, x1 - x0, y1 - y0);
        } else {
          ctx.drawImage(s.src as ImageBitmap, x0, y0, x1 - x0, y1 - y0);
        }
      } catch { /* a closed bitmap draws nothing; the tile is composed again when its replacement arrives */ }
    }
    const data = ctx.getImageData(0, 0, w, h).data;
    return new Uint8Array(data.buffer, data.byteOffset, data.byteLength);
  }

  private async run(): Promise<void> {
    if (this.running || !this.invoke) return;
    this.running = true;
    try {
      while (this.queue.length && !this.dead) {
        const job = this.queue.shift()!;
        try {
          if (job.kind === 'tile') {
            await this.invoke!('native_canvas_tile', job.bytes, { headers: { 'x-key': job.key, 'x-w': String(job.w), 'x-h': String(job.h) } });
          } else if (job.kind === 'resize') {
            await this.invoke!('native_canvas_resize', { rect: job.rect, dpr: job.dpr });
          } else {
            const out = (await this.invoke!('native_canvas_frame', { frame: job.payload })) as FrameOut;
            this.afterFrame(out);
          }
          this.failures = 0;
        } catch (err) {
          if (++this.failures >= 3 && !this.dead) {
            this.dead = true;
            this.onLost?.(String((err as { message?: string })?.message ?? err).slice(0, 80));
          }
        }
      }
    } finally {
      this.running = false;
    }
  }

  private afterFrame(out: FrameOut): void {
    // What the GPU dropped or never had is composed and sent again by the next frame.
    for (const k of out.evicted) this.sent.delete(keyString(k));
    for (const k of out.missing) this.sent.delete(keyString(k));
    if (!this.shown && out.presented && out.missing.length === 0) {
      this.shown = true;
      this.onShown?.();
    }
  }

  destroy(): void {
    this.dead = true;
    this.queue = [];
    this.sources.clear();
    this.sent.clear();
    const invoke = this.invoke;
    this.invoke = null;
    this.canvas = null;
    this.sctx = null;
    this.scratch = null;
    if (invoke) void invoke('native_canvas_stop').catch(() => { });
  }
}
