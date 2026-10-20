import { el, setClass, setTitle } from '../../ui/dom';
import { ctx, type Snap } from './ctx';
import { iconBtn, NumField, same, SwatchPick, Toggle, type NumVal } from './fields';

const REF_KEY = 'formdesign.props.ref';
const LINK_KEY = 'formdesign.props.constrain';
const REF_NAMES = ['top left', 'top centre', 'top right', 'left centre', 'centre', 'right centre', 'bottom left', 'bottom centre', 'bottom right'];

function load(key: string, def: number): number { try { const v = Number(localStorage.getItem(key)); return localStorage.getItem(key) === null || !Number.isFinite(v) ? def : v; } catch { return def; } }
function save(key: string, v: number): void { try { localStorage.setItem(key, String(v)); } catch { } }

export const shared = {
  ref: load(REF_KEY, 0),
  link: load(LINK_KEY, 0) === 1,
  subs: new Set<() => void>(),
  setRef(i: number): void { this.ref = i; save(REF_KEY, i); for (const f of this.subs) f(); ctx.schedule(); },
  setLink(on: boolean): void { this.link = on; save(LINK_KEY, on ? 1 : 0); for (const f of this.subs) f(); },
};

export class RefGrid {
  readonly el: HTMLElement;
  private readonly cells: HTMLButtonElement[] = [];
  constructor(small = false) {
    this.el = el(`<div class="fp-ref${small ? ' is-sm' : ''}" role="radiogroup" aria-label="Reference point"></div>`);
    for (let i = 0; i < 9; i++) {
      const b = el<HTMLButtonElement>('<button type="button" role="radio"><i></i></button>');
      setTitle(b, `Reference point: ${REF_NAMES[i]}`);
      b.addEventListener('click', () => shared.setRef(i));
      this.cells.push(b);
      this.el.appendChild(b);
    }
    shared.subs.add(() => this.sync());
    this.sync();
  }
  private sync(): void { this.cells.forEach((b, i) => { setClass(b, 'is-on', i === shared.ref); b.setAttribute('aria-checked', String(i === shared.ref)); }); }
}

function refPoint(s: Snap): [number, number] | null {
  const b = s.bounds, pg = s.page;
  if (!b) return null;
  const fx = [0, 0.5, 1][shared.ref % 3], fy = [0, 0.5, 1][Math.floor(shared.ref / 3)];
  return [b[0] + fx * (b[2] - b[0]) - (pg?.bounds[0] ?? 0), b[1] + fy * (b[3] - b[1]) - (pg?.bounds[1] ?? 0)];
}

function xform(params: Record<string, unknown>): void { void ctx.run('transform.set', { ...params, ref: shared.ref }); }

function size(axis: 'w' | 'h', v: number): void {
  const b = ctx.snap.bounds;
  if (!b) return;
  const w = b[2] - b[0], h = b[3] - b[1];
  if (!shared.link) { xform(axis === 'w' ? { width: v } : { height: v }); return; }
  if (axis === 'w') xform({ width: v, height: w > 0 ? (h * v) / w : h });
  else xform({ width: h > 0 ? (w * v) / h : w, height: v });
}

export class ObjFields {
  constructor(private readonly small = false) {}

  private made = new Map<string, unknown>();
  private get<T>(k: string, make: () => T): T {
    if (!this.made.has(k)) this.made.set(k, make());
    return this.made.get(k) as T;
  }

  get x(): NumField { return this.get('x', () => new NumField({ label: 'X', title: 'X location of the reference point', kind: 'measure', small: this.small, onCommit: (v) => xform({ x: v }) })); }
  get y(): NumField { return this.get('y', () => new NumField({ label: 'Y', title: 'Y location of the reference point', kind: 'measureV', small: this.small, onCommit: (v) => xform({ y: v }) })); }
  get w(): NumField { return this.get('w', () => new NumField({ label: 'W', title: 'Width', kind: 'measure', min: 0.01, small: this.small, onCommit: (v) => size('w', Number(v)) })); }
  get h(): NumField { return this.get('h', () => new NumField({ label: 'H', title: 'Height', kind: 'measureV', min: 0.01, small: this.small, onCommit: (v) => size('h', Number(v)) })); }
  get rot(): NumField { return this.get('rot', () => new NumField({ icon: 'rotate', title: 'Rotation angle', kind: 'deg', min: -360, max: 360, step: 1, presets: [180, 135, 90, 45, 0, -45, -90, -135], small: this.small, onCommit: (v) => xform({ rotation: v }) })); }
  get shear(): NumField { return this.get('shear', () => new NumField({ icon: 'shear', title: 'Shear angle', kind: 'deg', min: -85, max: 85, step: 1, small: this.small, onCommit: (v) => xform({ shear: v }) })); }
  get ref(): RefGrid { return this.get('ref', () => new RefGrid(this.small)); }
  get link(): Toggle {
    return this.get('link', () => {
      const t = new Toggle('link', 'Constrain proportions for width and height', (on) => shared.setLink(on));
      t.el.classList.add('fp-link');
      shared.subs.add(() => t.set(shared.link));
      t.set(shared.link);
      return t;
    });
  }
  get flipH(): HTMLButtonElement { return this.get('flipH', () => iconBtn('flipH', 'Flip horizontal', () => void ctx.run('transform.flip', { axis: 'horizontal' }))); }
  get flipV(): HTMLButtonElement { return this.get('flipV', () => iconBtn('flipV', 'Flip vertical', () => void ctx.run('transform.flip', { axis: 'vertical' }))); }
  get fill(): SwatchPick { return this.get('fill', () => new SwatchPick('Fill', (n) => void ctx.run('object.fill', { swatch: n }), this.small)); }
  get stroke(): SwatchPick { return this.get('stroke', () => new SwatchPick('Stroke', (n) => void ctx.run('object.stroke', { swatch: n }), this.small)); }
  get weight(): NumField { return this.get('weight', () => new NumField({ icon: 'weight', title: 'Stroke weight', kind: 'pt', min: 0, max: 800, step: 0.25, presets: [0, 0.25, 0.5, 0.75, 1, 2, 3, 4, 5, 6, 8, 10], small: this.small, onCommit: (v) => void ctx.run('object.stroke', { weight: v }) })); }
  get opacity(): NumField {
    return this.get('opacity', () => new NumField({
      icon: 'opacity', title: 'Opacity', kind: 'pct', min: 0, max: 100, step: 1, decimals: 0, presets: [100, 90, 75, 50, 25, 10, 0], small: this.small,
      onCommit: (v) => { const ids = ctx.snap.ids; ctx.remember(ids, { opacity: Number(v) / 100 }); void ctx.run('object.opacity', { opacity: Number(v) / 100 }); },
    }));
  }

  private has(k: string): boolean { return this.made.has(k); }

  update(s: Snap): void {
    const on = s.mode === 'objects' && s.items.length > 0;
    const set = (k: string, v: NumVal) => { if (!this.has(k)) return; const f = this.made.get(k) as NumField; f.set(on ? v : null); f.setDisabled(!on); };
    const rp = refPoint(s);
    set('x', rp ? rp[0] : null);
    set('y', rp ? rp[1] : null);
    set('w', s.bounds ? s.bounds[2] - s.bounds[0] : null);
    set('h', s.bounds ? s.bounds[3] - s.bounds[1] : null);
    set('rot', s.xf ? Math.abs(s.xf.rotation) < 1e-9 ? 0 : s.xf.rotation : null);
    set('shear', s.xf ? Math.abs(s.xf.shear) < 1e-9 ? 0 : s.xf.shear : null);
    const items = s.items;
    if (this.has('fill')) { this.fill.set(on ? (same(items, (i) => i.fill) as string | null) : null); this.fill.setDisabled(!on); }
    if (this.has('stroke')) { this.stroke.set(on ? (same(items, (i) => i.stroke?.swatch) as string | null) : null); this.stroke.setDisabled(!on); }
    if (this.has('weight')) {
      const none = items.every((i) => i.stroke?.swatch === '[None]');
      const wv = same(items, (i) => i.stroke?.weight);
      this.weight.set(on ? (none ? 0 : (wv as number | null)) : null);
      this.weight.setDisabled(!on);
    }
    set('opacity', s.opacity === null ? null : Math.round(s.opacity * 1000) / 10);
    for (const k of ['flipH', 'flipV']) if (this.has(k)) (this.made.get(k) as HTMLButtonElement).disabled = !on;
    if (this.has('link')) this.link.setDisabled(!on);
  }
}
