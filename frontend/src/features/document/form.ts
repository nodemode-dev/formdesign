import { enhanceSelect } from '../../ui/dropdown';
import { el, escapeHtml, q, setChecked, setClass, setText, setTitle, setValue } from '../../ui/dom';
import { fmt, parseLength, toPt, type Unit } from './units';

const STEP: Record<Unit, number> = { pt: 1, pc: 1, in: 0.0625, mm: 1, px: 1 };

/** A labelled control: the label above, the control under it. */
export function field(label: string, control: HTMLElement, extra = ''): HTMLElement {
  const f = el(`<label class="fd-f-field${extra ? ` ${extra}` : ''}"><span class="fd-f-label"></span></label>`);
  setText(q(f, '.fd-f-label'), label);
  f.appendChild(control);
  return f;
}

export function setLabel(f: HTMLElement, text: string): void { setText(q(f, '.fd-f-label'), text); }

export function row(...items: HTMLElement[]): HTMLElement {
  const r = el('<div class="fd-f-row"></div>');
  r.append(...items);
  return r;
}

/** A length in points shown in the chosen unit; typing another unit (12mm, 0.5in, 3p6) converts it. */
export class LengthField {
  readonly root = el('<div class="fd-f-num"><input class="fd-f-input" type="text" inputmode="decimal" spellcheck="false" autocomplete="off"><span class="fd-f-unit"></span></div>');
  readonly input = q<HTMLInputElement>(this.root, 'input');
  onChange: ((pt: number) => void) | null = null;
  private pt = 0;

  constructor(private unit: Unit, private readonly min = 0, private readonly max = 100000) {
    setText(q(this.root, '.fd-f-unit'), unit);
    this.input.addEventListener('change', () => this.commit());
    this.input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') this.commit();
      if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
      e.preventDefault();
      const step = toPt(STEP[this.unit] * (e.shiftKey ? 10 : 1), this.unit);
      this.setValue(this.pt + (e.key === 'ArrowUp' ? step : -step), true);
      this.input.value = fmt(this.pt, this.unit);
      this.onChange?.(this.pt);
    });
  }

  get value(): number { return this.pt; }

  set(pt: number): void { this.setValue(pt, false); }

  setUnit(u: Unit): void {
    this.unit = u;
    setText(q(this.root, '.fd-f-unit'), u);
    this.input.value = fmt(this.pt, u);
  }

  setDisabled(off: boolean): void { if (this.input.disabled !== off) this.input.disabled = off; setClass(this.root, 'is-off', off); }

  private setValue(pt: number, force: boolean): void {
    this.pt = Math.min(this.max, Math.max(this.min, pt));
    if (force) this.input.value = fmt(this.pt, this.unit);
    else setValue(this.input, fmt(this.pt, this.unit));
  }

  private commit(): void {
    const v = parseLength(this.input.value, this.unit);
    if (v === null) { this.input.value = fmt(this.pt, this.unit); return; }
    const before = this.pt;
    this.setValue(v, true);
    if (this.pt !== before) this.onChange?.(this.pt);
  }
}

/** A whole number with arrow-key steps. */
export class IntField {
  readonly root = el('<div class="fd-f-num"><input class="fd-f-input" type="text" inputmode="numeric" spellcheck="false" autocomplete="off"></div>');
  readonly input = q<HTMLInputElement>(this.root, 'input');
  onChange: ((n: number) => void) | null = null;
  private n = 0;

  constructor(private readonly min = 1, private readonly max = 9999) {
    this.input.addEventListener('change', () => this.commit());
    this.input.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') this.commit();
      if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return;
      e.preventDefault();
      this.n = this.clamp(this.n + (e.key === 'ArrowUp' ? 1 : -1) * (e.shiftKey ? 10 : 1));
      this.input.value = String(this.n);
      this.onChange?.(this.n);
    });
  }

  get value(): number { return this.n; }
  set(n: number): void { this.n = this.clamp(n); setValue(this.input, String(this.n)); }
  setDisabled(off: boolean): void { if (this.input.disabled !== off) this.input.disabled = off; setClass(this.root, 'is-off', off); }

  private clamp(n: number): number { return Math.min(this.max, Math.max(this.min, Math.round(n))); }

  private commit(): void {
    const v = Number(this.input.value.trim());
    if (!Number.isFinite(v)) { this.input.value = String(this.n); return; }
    const before = this.n;
    this.n = this.clamp(v);
    this.input.value = String(this.n);
    if (this.n !== before) this.onChange?.(this.n);
  }
}

export interface SegItem<T extends string> { value: T; label?: string; icon?: string; title?: string }

/** A segmented choice, the design system's .cv-seg. */
export class Seg<T extends string> {
  readonly root = el('<div class="cv-seg fd-f-seg" role="radiogroup"></div>');
  onChange: ((v: T) => void) | null = null;
  private v: T;

  constructor(items: Array<SegItem<T>>, value: T) {
    this.v = value;
    for (const it of items) {
      const b = el<HTMLButtonElement>(`<button type="button" role="radio">${it.icon ?? ''}${it.label ? `<span>${escapeHtml(it.label)}</span>` : ''}</button>`);
      b.dataset.value = it.value;
      if (it.title) setTitle(b, it.title);
      b.addEventListener('click', () => { if (this.v === it.value) return; this.set(it.value); this.onChange?.(it.value); });
      this.root.appendChild(b);
    }
    this.set(value);
  }

  get value(): T { return this.v; }

  set(v: T): void {
    this.v = v;
    for (const b of Array.from(this.root.children) as HTMLButtonElement[]) {
      setClass(b, 'is-on', b.dataset.value === v);
      b.setAttribute('aria-checked', String(b.dataset.value === v));
    }
  }
}

export class Check {
  readonly root = el('<label class="fd-f-check"><input type="checkbox"><span></span></label>');
  readonly input = q<HTMLInputElement>(this.root, 'input');
  onChange: ((on: boolean) => void) | null = null;
  constructor(label: string, on = false) {
    setText(q(this.root, 'span'), label);
    this.input.checked = on;
    this.input.addEventListener('change', () => this.onChange?.(this.input.checked));
  }
  get value(): boolean { return this.input.checked; }
  set(on: boolean): void { setChecked(this.input, on); }
  setDisabled(off: boolean): void { if (this.input.disabled !== off) this.input.disabled = off; setClass(this.root, 'is-off', off); }
}

/** A native select shown as the design system's glass list (dropdown.ts); it must sit in `host` first. */
export class Pick<T extends string> {
  readonly root = el('<div class="fd-f-pick"></div>');
  readonly select = el<HTMLSelectElement>('<select class="fd-f-select"></select>');
  onChange: ((v: T) => void) | null = null;
  private key = '';

  constructor(options: Array<{ value: T; label: string }>, value: T, label = '') {
    this.root.appendChild(this.select);
    if (label) this.select.setAttribute('aria-label', label);
    this.setOptions(options);
    this.select.value = value;
    enhanceSelect(this.select);
    this.select.addEventListener('change', () => this.onChange?.(this.select.value as T));
  }

  get value(): T { return this.select.value as T; }
  set(v: T): void { if (this.select.value !== v) this.select.value = v; }

  setOptions(options: Array<{ value: T; label: string }>): void {
    const key = JSON.stringify(options);
    if (key === this.key) return;
    this.key = key;
    const keep = this.select.value;
    this.select.replaceChildren(...options.map((o) => { const n = document.createElement('option'); n.value = o.value; n.textContent = o.label; return n; }));
    if (options.some((o) => o.value === keep)) this.select.value = keep;
  }

  setDisabled(off: boolean): void { if (this.select.disabled !== off) this.select.disabled = off; }
}

export async function copyText(text: string): Promise<boolean> {
  try { await navigator.clipboard.writeText(text); return true; } catch { }
  const ta = el<HTMLTextAreaElement>('<textarea class="fd-f-copybuf" readonly></textarea>');
  ta.value = text;
  document.body.appendChild(ta);
  ta.select();
  let ok = false;
  try { ok = document.execCommand('copy'); } catch { ok = false; }
  ta.remove();
  return ok;
}
