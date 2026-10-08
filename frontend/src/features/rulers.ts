import type { Feature, Workspace } from '../ui/workspace';
import { editor } from '../editor/editor';
import { el } from '../ui/dom';
import { readUnits, type Unit } from './document/units';

const KEY = 'formdes.rulers';
const SIZE = 18;
const PER: Record<Unit, number> = { pt: 1, pc: 12, in: 72, mm: 72 / 25.4, px: 1 };
const STEPS: Record<Unit, number[]> = {
  pt: [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000],
  px: [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000],
  mm: [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000],
  pc: [1, 2, 3, 6, 12, 24, 48, 96, 192, 384],
  in: [0.125, 0.25, 0.5, 1, 2, 4, 8, 16, 32],
};

interface Slot { spread: unknown; bounds: [number, number, number, number]; xf: number[] }

function invert(m: number[], x: number, y: number): [number, number] {
  const [a, b, c, d, e, f] = m;
  const det = a * d - b * c || 1;
  const dx = x - e, dy = y - f;
  return [(d * dx - c * dy) / det, (a * dy - b * dx) / det];
}

function minors(u: Unit, step: number): number {
  if (u === 'in') return step >= 1 ? 8 : step >= 0.5 ? 4 : 2;
  if (u === 'pc') return step === 1 ? 2 : step % 6 === 0 ? 6 : step;
  const lead = step / 10 ** Math.floor(Math.log10(step));
  return lead === 1 ? 10 : lead === 2 ? 4 : 5;
}

function label(v: number): string {
  const n = Math.round(v * 1000) / 1000;
  return String(Object.is(n, -0) ? 0 : n);
}

class Rulers {
  private readonly h: HTMLCanvasElement;
  private readonly v: HTMLCanvasElement;
  private readonly ghost = el('<div class="fd-guide-ghost" hidden></div>');
  private readonly probe = el('<span class="fd-ruler-probe" aria-hidden="true"></span>');
  private on = true;
  private queued = false;
  private mouse: [number, number] | null = null;
  private drag: 'h' | 'v' | null = null;

  constructor(private readonly ws: Workspace) {
    const stage = ws.viewport.stage;
    this.h = stage.querySelector('.cv-ruler-h') as HTMLCanvasElement;
    this.v = stage.querySelector('.cv-ruler-v') as HTMLCanvasElement;
    stage.append(this.ghost, this.probe);
    try { this.on = localStorage.getItem(KEY) !== '0'; } catch { this.on = true; }
    const vp = ws.viewport;
    const prev = vp.onView;
    vp.onView = () => { prev?.(); this.schedule(); };
    editor.on('layout', () => this.schedule());
    window.addEventListener('formdes:units', () => this.schedule());
    new ResizeObserver(() => this.schedule()).observe(stage);
    stage.addEventListener('pointermove', (e) => {
      const r = stage.getBoundingClientRect();
      this.mouse = [e.clientX - r.left, e.clientY - r.top];
      if (this.on) this.schedule();
    }, { passive: true });
    stage.addEventListener('pointerleave', () => { this.mouse = null; if (this.on) this.schedule(); });
    for (const [c, kind] of [[this.h, 'h'], [this.v, 'v']] as const) {
      c.addEventListener('pointerdown', (e) => this.start(e, kind));
      c.addEventListener('pointermove', (e) => this.move(e));
      c.addEventListener('pointerup', (e) => void this.end(e));
      c.addEventListener('pointercancel', () => this.cancel());
    }
    this.schedule();
  }

  get shown(): boolean { return this.on; }

  toggle(): void {
    this.on = !this.on;
    try { localStorage.setItem(KEY, this.on ? '1' : '0'); } catch { }
    this.schedule();
  }

  schedule(): void {
    if (this.queued) return;
    this.queued = true;
    requestAnimationFrame(() => { this.queued = false; this.apply(); });
  }

  private apply(): void {
    const show = this.on && this.ws.viewport.hasDoc;
    if (this.h.hidden === show) { this.h.hidden = !show; this.v.hidden = !show; }
    if (show) this.draw();
  }

  private color(token: string): string {
    this.probe.style.color = `var(${token})`;
    return getComputedStyle(this.probe).color;
  }

  private slot(): Slot | null {
    const lay = editor.layout;
    if (!lay || !lay.slots.length) return null;
    return lay.slots[this.ws.viewport.currentSpread()] as unknown as Slot;
  }

  private draw(): void {
    const vp = this.ws.viewport, s = this.slot();
    if (!s) return;
    const dpr = Math.max(1, window.devicePixelRatio || 1);
    const w = vp.width, ht = vp.height;
    const u = readUnits();
    const bg = this.color('--surface-1'), ink = this.color('--text-muted'), line = this.color('--line-1'), mark = this.color('--accent-color');
    const pxPerUnit = vp.view.zoom * PER[u];
    const step = STEPS[u].find((x) => x * pxPerUnit >= 54) ?? STEPS[u][STEPS[u].length - 1];
    const sub = minors(u, step);
    const ox = s.xf[4] ?? 0, oy = s.xf[5] ?? 0;
    for (const [c, horiz] of [[this.h, true], [this.v, false]] as const) {
      const cw = Math.round((horiz ? w : SIZE) * dpr), ch = Math.round((horiz ? SIZE : ht) * dpr);
      if (c.width !== cw || c.height !== ch) { c.width = cw; c.height = ch; }
      const g = c.getContext('2d') as CanvasRenderingContext2D;
      g.setTransform(dpr, 0, 0, dpr, 0, 0);
      g.fillStyle = bg;
      g.fillRect(0, 0, horiz ? w : SIZE, horiz ? SIZE : ht);
      g.fillStyle = line;
      if (horiz) g.fillRect(0, SIZE - 1, w, 1); else g.fillRect(SIZE - 1, 0, 1, ht);
      const len = horiz ? w : ht;
      const origin = horiz ? ox : oy;
      const viewStart = horiz ? vp.view.x : vp.view.y;
      const toScreen = (val: number) => ((val * PER[u] + origin) - viewStart) * vp.view.zoom;
      const first = Math.floor((viewStart - origin) / PER[u] / step) * step;
      g.fillStyle = ink;
      g.font = '9px Inter, system-ui, sans-serif';
      g.textBaseline = 'top';
      for (let val = first; toScreen(val) < len + 1; val += step) {
        for (let k = 0; k < sub; k++) {
          const p = Math.round(toScreen(val + (step * k) / sub)) + 0.5;
          if (p < SIZE || p > len) continue;
          const tick = k === 0 ? SIZE : k * 2 === sub ? 7 : 4;
          if (horiz) g.fillRect(p - 0.5, SIZE - tick, 1, tick); else g.fillRect(SIZE - tick, p - 0.5, tick, 1);
        }
        const p = Math.round(toScreen(val));
        if (p >= SIZE - 2 && p <= len) {
          const text = label(val);
          if (horiz) g.fillText(text, p + 3, 2);
          else { g.save(); g.translate(2, p + 3); g.rotate(Math.PI / 2); g.translate(0, -9); g.fillText(text, 0, 0); g.restore(); }
        }
      }
      if (this.mouse) {
        const p = Math.round(horiz ? this.mouse[0] : this.mouse[1]) + 0.5;
        g.fillStyle = mark;
        if (p > SIZE) { if (horiz) g.fillRect(p - 0.5, 0, 1, SIZE - 1); else g.fillRect(0, p - 0.5, SIZE - 1, 1); }
      }
      if (!horiz) { g.fillStyle = bg; g.fillRect(0, 0, SIZE, SIZE); g.fillStyle = line; g.fillRect(0, SIZE - 1, SIZE, 1); g.fillRect(SIZE - 1, 0, 1, SIZE); }
    }
  }

  private local(e: PointerEvent): [number, number] {
    const r = this.ws.viewport.stage.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  }

  private start(e: PointerEvent, kind: 'h' | 'v'): void {
    e.stopPropagation();
    if (e.button !== 0 || !this.ws.viewport.hasDoc) return;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    this.drag = kind;
    this.ghost.classList.toggle('is-v', kind === 'v');
    this.move(e);
  }

  private move(e: PointerEvent): void {
    if (!this.drag) return;
    const [x, y] = this.local(e);
    const pos = this.drag === 'h' ? y : x;
    this.ghost.style.transform = this.drag === 'h' ? `translateY(${Math.round(pos)}px)` : `translateX(${Math.round(pos)}px)`;
    if (this.ghost.hidden) this.ghost.hidden = false;
  }

  private cancel(): void {
    this.drag = null;
    this.ghost.hidden = true;
  }

  private async end(e: PointerEvent): Promise<void> {
    const kind = this.drag;
    this.cancel();
    if (!kind) return;
    const vp = this.ws.viewport;
    const [sx, sy] = this.local(e);
    if ((kind === 'h' ? sy : sx) <= SIZE || sx > vp.width || sy > vp.height) return;
    const [cx, cy] = vp.toCanvas(sx, sy);
    const lay = editor.layout;
    if (!lay) return;
    const slots = lay.slots as unknown as Slot[];
    const s = slots.find((x) => cx >= x.bounds[0] && cx <= x.bounds[2] && cy >= x.bounds[1] && cy <= x.bounds[3]) ?? slots[vp.currentSpread()];
    if (!s) return;
    const [px, py] = invert(s.xf, cx, cy);
    const onPage = lay.pages.some((p) => cx >= p.rect[0] && cx <= p.rect[2] && cy >= p.rect[1] && cy <= p.rect[3]);
    const horizontal = kind === 'h';
    await editor.exec('guide.add', {
      orientation: horizontal ? 'horizontal' : 'vertical', position: horizontal ? py : px, at: horizontal ? px : py,
      spread: s.spread, spreadGuide: !onPage || e.ctrlKey || e.metaKey,
    });
  }
}

let rulers: Rulers | null = null;

export const rulersFeature: Feature = {
  install(ws: Workspace): void {
    rulers = new Rulers(ws);
    editor.on('docs', () => rulers?.schedule());
    ws.addCommands({ id: 'ui.rulers', label: 'Rulers', keys: ['Mod+R'], checked: () => !!rulers?.shown, run: () => rulers?.toggle() });
  },
};
