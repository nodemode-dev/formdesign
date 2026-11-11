import { editor, friendly } from '../../editor/editor';
import { engine } from '../../engine/client';
import { toast } from '../../ui/toast';
import { asUnit, type Unit } from './units';

export type Rect = [number, number, number, number];

export interface ItemInfo {
  id: number; name: string; kind: string; bounds: Rect; fill: string; stroke: { swatch: string; weight: number };
  story?: number; columns?: number; asset?: number; children?: ItemInfo[]; locked: boolean; hidden: boolean; layer: number;
}
export interface PageInfo { index: number; name: string; bounds: Rect; margins: { top: number; bottom: number; inside: number; outside: number }; columns: number; side: string }
export interface SwatchInfo { name: string; css: string; model: 'cmyk' | 'rgb' | 'lab' | 'gray' | 'special' | 'other'; spot: boolean; values: number[]; locked: boolean }
export interface TextSel { story: number; frame: number | null; anchor: number; focus: number }
export interface Attrs {
  story: number; paragraphStyle: string; characterStyle: string; paraOverrides: number; charOverrides: number;
  chars: Record<string, any>; para: Record<string, any>;
}
export interface DocSetup { width: number; height: number; pages: number; startPage: number; facingPages: boolean; intent: string; bleed: number[]; slug: number[]; binding: string }
export type Kind = 'text' | 'graphic' | 'shape' | 'line' | 'group';
export type Mode = 'nodoc' | 'none' | 'objects' | 'text';

export interface Snap {
  rev: number; uid: number | null; mode: Mode;
  ids: number[]; items: ItemInfo[]; kinds: Set<Kind>; text: TextSel | null; attrs: Attrs | null;
  xf: { rotation: number; scaleX: number; scaleY: number; shear: number } | null;
  bounds: Rect | null; page: PageInfo | null; pages: PageInfo[]; allPages: PageInfo[];
  units: Unit; vunits: Unit; strokeInBounds: boolean;
  swatches: SwatchInfo[]; swatch: Map<string, SwatchInfo>;
  paraStyles: string[]; charStyles: string[]; fonts: string[];
  setup: DocSetup | null; opacity: number | null; title: string;
}

const EMPTY: Snap = {
  rev: -1, uid: null, mode: 'nodoc', ids: [], items: [], kinds: new Set(), text: null, attrs: null, xf: null, bounds: null, page: null, pages: [], allPages: [],
  units: 'points', vunits: 'points', strokeInBounds: true, swatches: [], swatch: new Map(), paraStyles: [], charStyles: [], fonts: [],
  setup: null, opacity: null, title: '',
};

export function kindOf(it: ItemInfo): Kind {
  if (it.children) return 'group';
  if (it.story !== undefined) return 'text';
  if (it.asset !== undefined) return 'graphic';
  if (/line/i.test(it.kind)) return 'line';
  return 'shape';
}

const SPECIAL: Record<string, string> = { '[Paper]': '#ffffff', '[Black]': '#1a1a1a', '[Registration]': '#1a1a1a' };

function cmykCss(c: number, m: number, y: number, k: number): string {
  const ch = (x: number) => Math.round(255 * (1 - Math.min(1, x)) * (1 - Math.min(1, k)));
  return `rgb(${ch(c)}, ${ch(m)}, ${ch(y)})`;
}

function labCss(L: number, a: number, b: number): string {
  const fy = (L + 16) / 116, fx = fy + a / 500, fz = fy - b / 200;
  const f = (t: number) => (t ** 3 > 0.008856 ? t ** 3 : (t - 16 / 116) / 7.787);
  const X = 0.9642 * f(fx), Y = f(fy), Z = 0.8251 * f(fz);
  const lin = [3.1339 * X - 1.6169 * Y - 0.4906 * Z, -0.9788 * X + 1.9161 * Y + 0.0335 * Z, 0.0719 * X - 0.229 * Y + 1.4052 * Z];
  const g = (v: number) => Math.round(255 * Math.max(0, Math.min(1, v <= 0.0031308 ? 12.92 * v : 1.055 * v ** (1 / 2.4) - 0.055)));
  return `rgb(${g(lin[0])}, ${g(lin[1])}, ${g(lin[2])})`;
}

function parseAse(b64: string): Map<string, Omit<SwatchInfo, 'locked'>> {
  const out = new Map<string, Omit<SwatchInfo, 'locked'>>();
  let bin: string;
  try { bin = atob(b64); } catch { return out; }
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const dv = new DataView(bytes.buffer);
  if (bytes.length < 12) return out;
  const n = dv.getUint32(8);
  let pos = 12;
  for (let i = 0; i < n && pos + 6 <= bytes.length; i++) {
    const type = dv.getUint16(pos), len = dv.getUint32(pos + 2);
    const start = pos + 6;
    pos = start + len;
    if (type !== 1) continue;
    const nl = dv.getUint16(start);
    let name = '';
    for (let j = 0; j < nl; j++) name += String.fromCharCode(dv.getUint16(start + 2 + j * 2));
    name = name.replace(/\0+$/, '');
    let p = start + 2 + nl * 2;
    const model = String.fromCharCode(bytes[p], bytes[p + 1], bytes[p + 2], bytes[p + 3]);
    p += 4;
    const count = model === 'CMYK' ? 4 : model === 'Gray' ? 1 : 3;
    const v: number[] = [];
    for (let j = 0; j < count; j++) v.push(dv.getFloat32(p + j * 4));
    p += count * 4;
    const ctype = dv.getUint16(p);
    let css = '#888', m: SwatchInfo['model'] = 'other';
    if (model === 'CMYK') { css = cmykCss(v[0], v[1], v[2], v[3]); m = 'cmyk'; }
    else if (model === 'RGB ') { css = `rgb(${Math.round(v[0] * 255)}, ${Math.round(v[1] * 255)}, ${Math.round(v[2] * 255)})`; m = 'rgb'; }
    else if (model === 'LAB ') { css = labCss(v[0] * 100, v[1], v[2]); m = 'lab'; }
    else if (model === 'Gray') { const g = Math.round(255 * (1 - v[0])); css = `rgb(${g}, ${g}, ${g})`; m = 'gray'; }
    out.set(name, { name, css, model: m, spot: ctype === 1, values: v });
  }
  return out;
}

type Listener = (s: Snap) => void;
type Mem = { opacity?: number; wrap?: string; wrapOffset?: number; gutter?: number; inset?: number; vj?: string; autoSize?: string; fitting?: string; autoFit?: boolean; colGutter?: number };

async function query<T = any>(command: string, params: Record<string, unknown> = {}): Promise<T | null> {
  try { return (await engine.call<T>('exec', { command, params })).result; } catch { return null; }
}

class Ctx {
  snap: Snap = EMPTY;
  private readonly subs = new Set<Listener>();
  private pending = false;
  private running = false;
  private again = false;
  private docGen = 0;
  private fontsGen = 0;
  private lastRedo = '';
  private readonly cache = new Map<string, { key: string; value: Promise<any> }>();
  private readonly mem = new Map<string, Mem>();

  constructor() {
    for (const ev of ['selection', 'docs', 'layout', 'ready', 'connection'] as const) editor.on(ev, () => this.schedule());
    editor.on('change', (e: { docChanged?: boolean } | undefined) => { if (e?.docChanged) this.docGen++; this.schedule(); });
    // Cloud Fonts activated or removed: the family list is read again.
    document.addEventListener('formdesign:fonts-changed', () => { this.fontsGen++; this.schedule(); });
    editor.on('docs', () => {
      const d = editor.doc;
      const redo = d ? `${d.uid}:${d.canRedo}:${d.redoLabel ?? ''}` : '';
      if (d && d.canRedo && redo !== this.lastRedo) this.forget(d.uid);
      this.lastRedo = redo;
    });
  }

  on(fn: Listener): void { this.subs.add(fn); fn(this.snap); }

  schedule(): void {
    if (this.pending) return;
    this.pending = true;
    requestAnimationFrame(() => { this.pending = false; void this.refresh(); });
  }

  bump(): void { this.docGen++; this.schedule(); }

  private memo<T>(name: string, key: string, fn: () => Promise<T>): Promise<T> {
    const c = this.cache.get(name);
    if (c && c.key === key) return c.value;
    const value = fn();
    this.cache.set(name, { key, value });
    return value;
  }

  remember(ids: number[], patch: Mem): void {
    const uid = editor.doc?.uid;
    if (uid === undefined) return;
    for (const id of ids) { const k = `${uid}:${id}`; this.mem.set(k, { ...this.mem.get(k), ...patch }); }
  }

  recall<K extends keyof Mem>(ids: number[], key: K): Mem[K] | null {
    const uid = editor.doc?.uid;
    if (uid === undefined || !ids.length) return null;
    let v: Mem[K] | undefined;
    for (const id of ids) {
      const x = this.mem.get(`${uid}:${id}`)?.[key];
      if (x === undefined || (v !== undefined && x !== v)) return null;
      v = x;
    }
    return v ?? null;
  }

  private forget(uid: number): void { for (const k of [...this.mem.keys()]) if (k.startsWith(`${uid}:`)) this.mem.delete(k); }

  private async refresh(): Promise<void> {
    if (this.running) { this.again = true; return; }
    this.running = true;
    try { await this.build(); } catch (err) { console.error('[formdesign] properties', err); }
    this.running = false;
    if (this.again) { this.again = false; this.schedule(); }
  }

  private async build(): Promise<void> {
    if (!editor.hasDoc || !engine.connected) {
      if (this.snap.mode !== 'nodoc') this.publish({ ...EMPTY, fonts: this.snap.fonts });
      return;
    }
    const rev = engine.rev, uid = editor.doc?.uid ?? null, gen = `${uid}:${this.docGen}`;
    const fontsP = this.memo('fonts', `fonts${this.fontsGen}`, () => engine.call<string[]>('fonts.families').then((r) => r.result ?? []).catch(() => [] as string[]));
    const prefsP = this.memo('prefs', gen, () => query('document.preferences'));
    const appP = this.memo('app', 'once', () => query('prefs.set'));
    const aseP = this.memo('ase', gen, () => query<{ base64: string }>('swatch.save'));
    const data = await editor.inspect();
    if (!data) return;
    const sel = (editor.selection ?? data.selection ?? { items: [] }) as { items?: number[]; text?: TextSel | null; cells?: unknown };
    const index = new Map<number, ItemInfo>();
    const spreadOf = new Map<number, number>();
    const walk = (list: ItemInfo[], si: number) => { for (const it of list) { index.set(it.id, it); spreadOf.set(it.id, si); if (it.children) walk(it.children, si); } };
    (data.spreads as Array<{ index: number; items: ItemInfo[] }>).forEach((s) => walk(s.items, s.index));
    const ids = (sel.items ?? []).filter((id) => index.has(id));
    const text = sel.text ?? null;
    const mode: Mode = text || sel.cells ? 'text' : ids.length ? 'objects' : 'none';
    const items = ids.map((id) => index.get(id) as ItemInfo);
    const kinds = new Set<Kind>(items.map(kindOf));
    const wantAttrs = mode === 'text' || kinds.has('text');
    const [attrs, xf, opaque, setup] = await Promise.all([
      wantAttrs ? this.memo('attrs', `${uid}:${rev}:${engine.selRev}`,() => query<Attrs>('type.selectionAttrs')) : Promise.resolve(null),
      mode === 'objects' ? this.memo('xf', `${uid}:${rev}:${engine.selRev}`,() => query('transform.info')) : Promise.resolve(null),
      mode === 'objects' ? this.memo('opaque', gen, () => query<{ ids: number[] }>('find.objects', { opacity: 1, select: false })) : Promise.resolve(null),
      mode === 'none' ? this.memo('setup', gen, () => query<DocSetup>('layout.documentSetup')) : Promise.resolve(null),
    ]);
    let opacity: number | null = null;
    if (mode === 'objects') {
      const o = new Set(opaque?.ids ?? []);
      if (ids.every((id) => o.has(id))) opacity = 1;
      else {
        const r = this.recall(ids, 'opacity');
        if (typeof r === 'number') {
          const hit = await this.memo('opacity:' + r, gen, () => query<{ ids: number[] }>('find.objects', { opacity: r, select: false }));
          if (ids.every((id) => hit?.ids?.includes(id))) opacity = r;
        }
      }
    }
    const [fonts, prefs, app, ase] = await Promise.all([fontsP, prefsP, appP, aseP]);
    if (engine.rev !== rev && this.again) return;
    const colours = parseAse(ase?.base64 ?? '');
    const swatches: SwatchInfo[] = (data.swatches as string[]).map((name) => {
      const c = colours.get(name);
      if (c) return { ...c, locked: false };
      if (name === '[None]') return { name, css: 'none', model: 'special', spot: false, values: [], locked: true };
      if (SPECIAL[name]) return { name, css: SPECIAL[name], model: 'special', spot: false, values: [], locked: true };
      return { name, css: 'linear-gradient(90deg, var(--text-primary), var(--surface-raised))', model: 'other', spot: false, values: [], locked: false };
    });
    let bounds: Rect | null = null;
    const strokeIn = !!app?.dimensionsIncludeStroke;
    for (const it of items) {
      const w = strokeIn && it.stroke && it.stroke.swatch !== '[None]' ? it.stroke.weight / 2 : 0;
      const b: Rect = [it.bounds[0] - w, it.bounds[1] - w, it.bounds[2] + w, it.bounds[3] + w];
      bounds = bounds ? [Math.min(bounds[0], b[0]), Math.min(bounds[1], b[1]), Math.max(bounds[2], b[2]), Math.max(bounds[3], b[3])] : b;
    }
    const spreadIdx = ids.length ? spreadOf.get(ids[0]) ?? 0 : (text?.frame !== undefined && text?.frame !== null ? spreadOf.get(text.frame) ?? 0 : 0);
    const spread = (data.spreads as Array<{ index: number; pages: PageInfo[] }>)[spreadIdx];
    const pages = spread?.pages ?? [];
    let page: PageInfo | null = null;
    if (bounds && pages.length) {
      const cx = (bounds[0] + bounds[2]) / 2;
      page = pages.find((p) => cx >= p.bounds[0] && cx < p.bounds[2]) ?? (cx < pages[0].bounds[0] ? pages[0] : pages[pages.length - 1]);
    } else if (!ids.length) page = (data.spreads as Array<{ pages: PageInfo[] }>)[0]?.pages[0] ?? null;
    this.publish({
      rev, uid, mode, ids, items, kinds, text, attrs: attrs && typeof attrs === 'object' ? attrs : null, xf, bounds, page, pages,
      allPages: (data.spreads as Array<{ pages: PageInfo[] }>).flatMap((sp) => sp.pages),
      units: asUnit(prefs?.horizontalUnits), vunits: asUnit(prefs?.verticalUnits), strokeInBounds: strokeIn,
      swatches, swatch: new Map(swatches.map((s) => [s.name, s])), paraStyles: data.paragraphStyles ?? [], charStyles: data.characterStyles ?? [],
      fonts, setup, opacity, title: data.title ?? '',
    });
  }

  private publish(s: Snap): void {
    this.snap = s;
    for (const fn of this.subs) { try { fn(s); } catch (err) { console.error('[formdesign] properties view', err); } }
  }

  async run(command: string, params: Record<string, unknown> = {}): Promise<boolean> {
    const r = await editor.exec(command, params);
    this.bump();
    return r !== undefined;
  }

  async batch(commands: Array<{ command: string; params?: Record<string, unknown> }>): Promise<boolean> {
    if (commands.length === 1) return this.run(commands[0].command, commands[0].params ?? {});
    const r = await editor.batch(commands);
    this.bump();
    return r !== undefined;
  }

  async ask<T = any>(command: string, params: Record<string, unknown> = {}): Promise<T | null> { return query<T>(command, params); }

  fail(err: unknown): void { toast(friendly((err as Error)?.message ?? String(err)), 'error'); }
}

export const ctx = new Ctx();
