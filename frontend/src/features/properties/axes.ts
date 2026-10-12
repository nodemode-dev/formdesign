import { el, q, setText } from '../../ui/dom';
import { engine } from '../../engine/client';
import { type Snap } from './ctx';
import { char, textTarget } from './type';

interface Axis { tag: string; name: string; min: number; default: number; max: number }
interface Instance { name: string; coords: Record<string, number> }
interface AxisInfo { variable: boolean; axes: Axis[]; instances: Instance[] }
interface Row { axis: Axis; range: HTMLInputElement; out: HTMLElement }

const cache = new Map<string, Promise<AxisInfo | null>>();

function fetchAxes(font: string, style: string): Promise<AxisInfo | null> {
  const key = `${font}\n${style}`;
  let p = cache.get(key);
  if (!p) {
    p = engine.call<AxisInfo>('exec', { command: 'type.fontAxes', params: { font, style } }).then((r) => r.result ?? null).catch(() => null);
    cache.set(key, p);
  }
  return p;
}

function shown(v: number): string { return Number.isInteger(v) ? String(v) : v.toFixed(2).replace(/0+$/, '').replace(/\.$/, ''); }

export class VariationAxes {
  readonly el = el('<div class="fp-axes" hidden><div class="fp-axes-head"><span class="fp-axes-title">Variable font axes</span><button class="fp-axes-reset" type="button" hidden>Reset</button></div><div class="fp-axes-rows"></div></div>');
  private readonly rows = q(this.el, '.fp-axes-rows');
  private readonly reset = q<HTMLButtonElement>(this.el, '.fp-axes-reset');
  private readonly byTag = new Map<string, Row>();
  private sig = '';
  private key = '';
  private info: AxisInfo | null = null;
  private style = '';
  private vars: Record<string, number> = {};
  private seq = 0;

  constructor() {
    this.reset.addEventListener('click', () => char({ fontVariations: null }));
  }

  update(s: Snap): void {
    const on = textTarget(s) && !!s.attrs;
    const c = on ? (s.attrs as NonNullable<Snap['attrs']>).chars : {};
    const font = on ? String(c.fontFamily ?? '') : '';
    const style = on ? String(c.fontStyle ?? 'Regular') : '';
    const raw = on && c.fontVariations && typeof c.fontVariations === 'object' ? (c.fontVariations as Record<string, number>) : {};
    this.vars = raw;
    this.style = style;
    const key = font ? `${font}\n${style}` : '';
    if (key === this.key) { this.sync(); return; }
    this.key = key;
    const mine = ++this.seq;
    if (!font) { this.info = null; this.build(); return; }
    void fetchAxes(font, style).then((info) => {
      if (mine !== this.seq) return;
      this.info = info;
      this.build();
    });
  }

  private build(): void {
    const axes = this.info?.variable ? this.info.axes : [];
    const sig = axes.map((a) => `${a.tag}:${a.min}:${a.max}`).join('|');
    if (sig !== this.sig) {
      this.sig = sig;
      this.byTag.clear();
      const frag = document.createDocumentFragment();
      for (const axis of axes) {
        const row = el(`<label class="fp-ax"><span class="fp-ax-name"></span><input class="fp-ax-range" type="range"><span class="fp-ax-val"></span></label>`);
        setText(q(row, '.fp-ax-name'), axis.name);
        row.title = `${axis.name} (${axis.tag}), ${shown(axis.min)} to ${shown(axis.max)}`;
        const range = q<HTMLInputElement>(row, 'input');
        range.min = String(axis.min);
        range.max = String(axis.max);
        range.step = axis.max - axis.min > 20 ? '1' : '0.01';
        range.setAttribute('aria-label', axis.name);
        const out = q(row, '.fp-ax-val');
        range.addEventListener('input', () => setText(out, shown(Number(range.value))));
        range.addEventListener('change', () => this.commit());
        this.byTag.set(axis.tag, { axis, range, out });
        frag.appendChild(row);
      }
      this.rows.replaceChildren(frag);
    }
    this.el.hidden = axes.length === 0;
    this.sync();
  }

  private base(tag: string, dflt: number): number {
    const inst = this.info?.instances.find((i) => i.name.toLowerCase() === this.style.toLowerCase());
    return inst?.coords[tag] ?? dflt;
  }

  private sync(): void {
    for (const [tag, r] of this.byTag) {
      const v = this.vars[tag] ?? this.base(tag, r.axis.default);
      if (r.range.value !== String(v) && document.activeElement !== r.range) r.range.value = String(v);
      setText(r.out, shown(v));
    }
    this.reset.hidden = Object.keys(this.vars).length === 0;
  }

  private commit(): void {
    const next: Record<string, number> = {};
    for (const [tag, r] of this.byTag) {
      const v = Number(r.range.value);
      if (Math.abs(v - this.base(tag, r.axis.default)) > 1e-6 || tag in this.vars) next[tag] = v;
    }
    char({ fontVariations: Object.keys(next).length ? next : null });
  }
}
