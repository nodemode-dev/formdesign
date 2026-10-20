import { enhanceSelect } from '../../ui/dropdown';
import { el, patchList, q, setClass, setDisabled, setHidden, setStyleVar, setText, setTitle } from '../../ui/dom';
import { ctx, type SwatchInfo } from './ctx';
import { glyph } from './glyphs';
import { fmtNum, formatMeasure, parseMeasure, parseNumber, unitStep, type Unit } from './units';

export type NumKind = 'measure' | 'measureV' | 'pt' | 'pct' | 'deg' | 'int' | 'em' | 'plain';
export type NumVal = number | string | null;

export interface NumSpec {
  label?: string; icon?: string; title: string; kind: NumKind;
  min?: number; max?: number; step?: number; decimals?: number;
  keywords?: Record<string, string>;
  presets?: NumVal[];
  base?: () => number | null;
  onCommit: (v: number | string) => void;
  cls?: string; small?: boolean;
}

function unitFor(kind: NumKind): Unit { return kind === 'measureV' ? ctx.snap.vunits : ctx.snap.units; }

export class NumField {
  readonly el: HTMLElement;
  readonly input: HTMLInputElement;
  value: NumVal = null;
  private dirty = false;
  private shown = '';

  constructor(readonly spec: NumSpec) {
    const lab = spec.icon ? glyph(spec.icon, 14) : `<span class="fp-num-txt"></span>`;
    this.el = el(`<div class="fp-num${spec.small ? ' is-sm' : ''}${spec.cls ? ` ${spec.cls}` : ''}"><span class="fp-num-lab">${lab}</span><input class="fp-num-in" type="text" spellcheck="false" autocomplete="off"></div>`);
    if (!spec.icon && spec.label) setText(q(this.el, '.fp-num-txt'), spec.label);
    if (!spec.label && !spec.icon) this.el.classList.add('no-label');
    this.input = q<HTMLInputElement>(this.el, 'input');
    this.input.setAttribute('aria-label', spec.title);
    setTitle(this.el, spec.title);
    if (spec.presets?.length) {
      const b = el<HTMLButtonElement>(`<button class="fp-num-pre" type="button" tabindex="-1" aria-label="Presets">${glyph('caret', 12)}</button>`);
      b.addEventListener('pointerdown', (e) => e.preventDefault());
      b.addEventListener('click', () => {
        if (this.input.disabled) return;
        menu.open(b, (spec.presets ?? []).map((v) => ({ id: String(v), label: this.format(v) })), String(this.value ?? ''), (id) => {
          const v = spec.presets?.find((x) => String(x) === id);
          if (v !== undefined && v !== null) this.commit(v);
        }, this.el);
      });
      this.el.appendChild(b);
      this.el.classList.add('has-pre');
    }
    this.wire();
  }

  private format(v: NumVal): string {
    if (v === null) return '';
    if (typeof v === 'string') return this.spec.keywords?.[v] ?? v;
    const d = this.spec.decimals ?? 2;
    switch (this.spec.kind) {
      case 'measure': case 'measureV': return formatMeasure(v, unitFor(this.spec.kind));
      case 'pt': return `${fmtNum(v, d)} pt`;
      case 'pct': return `${fmtNum(v, d)}%`;
      case 'deg': return `${fmtNum(v, d)}°`;
      case 'int': return String(Math.round(v));
      default: return fmtNum(v, d);
    }
  }

  private parse(s: string): NumVal {
    const t = s.trim().toLowerCase();
    if (!t) return null;
    for (const [k, label] of Object.entries(this.spec.keywords ?? {})) if (label.toLowerCase().startsWith(t) || k.toLowerCase() === t) return k;
    let v: number | null;
    switch (this.spec.kind) {
      case 'measure': case 'measureV': {
        const cur = typeof this.value === 'number' ? this.value : null;
        v = parseMeasure(s, unitFor(this.spec.kind), cur);
        break;
      }
      case 'pt': v = parseMeasure(s, 'points'); break;
      default: v = parseNumber(s);
    }
    if (v === null) return null;
    if (this.spec.kind === 'int') v = Math.round(v);
    return this.clamp(v);
  }

  private clamp(v: number): number {
    if (this.spec.min !== undefined) v = Math.max(this.spec.min, v);
    if (this.spec.max !== undefined) v = Math.min(this.spec.max, v);
    return v;
  }

  private step(): number {
    if (this.spec.step !== undefined) return this.spec.step;
    return this.spec.kind === 'measure' || this.spec.kind === 'measureV' ? unitStep(unitFor(this.spec.kind)) : 1;
  }

  private numeric(): number {
    if (typeof this.value === 'number') return this.value;
    return this.spec.base?.() ?? 0;
  }

  private show(text: string): void {
    this.shown = text;
    if (this.input.value !== text) this.input.value = text;
  }

  set(v: NumVal): void {
    this.value = v;
    if (this.dirty && document.activeElement === this.input) return;
    this.dirty = false;
    this.show(this.format(v));
    this.input.placeholder = v === null ? '–' : '';
  }

  setDisabled(off: boolean): void {
    setDisabled(this.input, off);
    setClass(this.el, 'is-off', off);
  }

  refresh(): void { if (!(this.dirty && document.activeElement === this.input)) this.show(this.format(this.value)); }

  commit(v: number | string): void {
    this.value = v;
    this.dirty = false;
    this.show(this.format(v));
    this.input.placeholder = '';
    this.spec.onCommit(v);
  }

  private revert(flash = false): void {
    this.dirty = false;
    this.show(this.format(this.value));
    if (flash) { this.el.classList.add('is-invalid'); window.setTimeout(() => this.el.classList.remove('is-invalid'), 420); }
  }

  private accept(): void {
    if (!this.dirty) return;
    const v = this.parse(this.input.value);
    if (v === null) { this.revert(this.input.value.trim() !== ''); return; }
    if (v === this.value) { this.revert(); return; }
    this.commit(v);
  }

  private wire(): void {
    const inp = this.input;
    inp.addEventListener('input', () => { this.dirty = inp.value !== this.shown; });
    inp.addEventListener('focus', () => { window.requestAnimationFrame(() => { if (document.activeElement === inp) inp.select(); }); });
    inp.addEventListener('blur', () => this.accept());
    inp.addEventListener('keydown', (e) => {
      e.stopPropagation();
      if (e.key === 'Enter') { e.preventDefault(); this.accept(); inp.select(); return; }
      if (e.key === 'Escape') { e.preventDefault(); this.revert(); inp.blur(); return; }
      if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
        e.preventDefault();
        const typed = this.dirty ? this.parse(inp.value) : null;
        const cur = typeof typed === 'number' ? typed : this.numeric();
        const k = e.shiftKey ? 10 : e.altKey ? 0.1 : 1;
        const next = this.clamp(cur + (e.key === 'ArrowUp' ? 1 : -1) * this.step() * k);
        this.commit(Math.round(next * 1e4) / 1e4);
        inp.select();
      }
    });
    const lab = q(this.el, '.fp-num-lab');
    lab.addEventListener('pointerdown', (e) => {
      if (e.button !== 0 || inp.disabled) return;
      e.preventDefault();
      const x0 = e.clientX, start = this.numeric();
      let scrub = false, last = start;
      lab.setPointerCapture(e.pointerId);
      const move = (ev: PointerEvent) => {
        const dx = ev.clientX - x0;
        if (!scrub && Math.abs(dx) < 3) return;
        if (!scrub) { scrub = true; this.el.classList.add('is-scrub'); document.body.classList.add('fp-scrubbing'); }
        const k = ev.shiftKey ? 10 : ev.altKey ? 0.1 : 1;
        last = Math.round(this.clamp(start + Math.round(dx / 2) * this.step() * k) * 1e4) / 1e4;
        this.dirty = false;
        this.show(this.format(last));
      };
      const up = () => {
        lab.removeEventListener('pointermove', move);
        lab.removeEventListener('pointerup', up);
        lab.removeEventListener('pointercancel', up);
        this.el.classList.remove('is-scrub');
        document.body.classList.remove('fp-scrubbing');
        if (scrub) { if (last !== this.value) this.commit(last); }
        else inp.focus();
      };
      lab.addEventListener('pointermove', move);
      lab.addEventListener('pointerup', up);
      lab.addEventListener('pointercancel', up);
    });
  }
}

export interface MenuItemSpec { id: string; label: string; chip?: SwatchInfo | null; meta?: string }

class PopMenu {
  private readonly pop = el('<div class="fp-pop" role="listbox" hidden><div class="fp-pop-list"></div></div>');
  private readonly list = q(this.pop, '.fp-pop-list');
  private anchor: HTMLElement | null = null;
  private pick: ((id: string) => void) | null = null;
  private cur = '';
  private hot = -1;

  constructor() {
    (document.getElementById('overlay-root') ?? document.body).appendChild(this.pop);
    document.addEventListener('pointerdown', (e) => {
      if (this.pop.hidden) return;
      const t = e.target as Node;
      if (!this.pop.contains(t) && !this.anchor?.contains(t)) this.close();
    }, true);
    window.addEventListener('resize', () => this.close());
    this.pop.addEventListener('keydown', (e) => {
      const rows = Array.from(this.list.children) as HTMLElement[];
      if (e.key === 'Escape') { e.preventDefault(); this.close(); this.anchor?.focus(); return; }
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        this.hot = (this.hot + (e.key === 'ArrowDown' ? 1 : rows.length - 1) + rows.length) % Math.max(1, rows.length);
        this.mark();
        return;
      }
      if (e.key === 'Enter') { e.preventDefault(); rows[this.hot]?.click(); }
    });
  }

  get isOpen(): boolean { return !this.pop.hidden; }

  open(anchor: HTMLElement, items: MenuItemSpec[], current: string, pick: (id: string) => void, alignTo?: HTMLElement): void {
    if (!this.pop.hidden && this.anchor === anchor) { this.close(); return; }
    this.anchor = anchor;
    this.pick = pick;
    this.cur = current;
    patchList<MenuItemSpec>(this.list, items, {
      key: (it) => it.id,
      create: (it) => {
        const n = el(`<button class="fp-pop-item" type="button" role="option"><span class="fp-chip"></span><span class="fp-pop-label"></span><span class="fp-pop-meta"></span></button>`);
        n.addEventListener('click', () => { const id = n.dataset.key as string; this.close(); this.pick?.(id); });
        this.fillRow(n, it);
        return n;
      },
      update: (n, it) => this.fillRow(n, it),
    });
    setClass(this.pop, 'has-chips', items.some((i) => i.chip !== undefined));
    setHidden(this.pop, false);
    const r = (alignTo ?? anchor).getBoundingClientRect();
    this.pop.style.minWidth = `${Math.max(150, Math.round(r.width))}px`;
    const h = this.pop.offsetHeight, w = this.pop.offsetWidth;
    this.pop.style.left = `${Math.round(Math.max(4, Math.min(r.left, window.innerWidth - w - 4)))}px`;
    this.pop.style.top = `${Math.round(r.bottom + 4 + h <= window.innerHeight - 4 ? r.bottom + 4 : Math.max(4, r.top - 4 - h))}px`;
    const rows = Array.from(this.list.children) as HTMLElement[];
    this.hot = Math.max(0, rows.findIndex((n) => n.dataset.key === current));
    this.mark();
    rows[this.hot]?.scrollIntoView({ block: 'nearest' });
    this.pop.focus();
    (rows[this.hot] ?? this.pop).focus();
  }

  private fillRow(n: HTMLElement, it: MenuItemSpec): void {
    setText(q(n, '.fp-pop-label'), it.label);
    setText(q(n, '.fp-pop-meta'), it.meta ?? '');
    const chip = q(n, '.fp-chip');
    setHidden(chip, it.chip === undefined);
    if (it.chip !== undefined) paintChip(chip, it.chip);
    setClass(n, 'is-current', it.id === this.cur);
  }

  private mark(): void { Array.from(this.list.children).forEach((n, i) => { setClass(n, 'is-hot', i === this.hot); if (i === this.hot) (n as HTMLElement).focus(); }); }

  close(): void { setHidden(this.pop, true); }
}

export const menu = new PopMenu();

export function paintChip(chip: HTMLElement, sw: SwatchInfo | null | undefined, tint = 1): void {
  const none = !sw || sw.css === 'none';
  setClass(chip, 'is-none', !!sw && sw.css === 'none');
  setClass(chip, 'is-mixed', !sw);
  setStyleVar(chip, '--chip', none ? 'transparent' : sw.css.startsWith('linear') ? 'transparent' : sw.css);
  setStyleVar(chip, '--chip-img', !none && sw.css.startsWith('linear') ? sw.css : 'none');
  setStyleVar(chip, '--chip-tint', String(Math.max(0.08, Math.min(1, tint))));
}

export function swatchItems(): MenuItemSpec[] {
  return ctx.snap.swatches.map((s) => ({ id: s.name, label: s.name, chip: s, meta: s.spot ? 'Spot' : '' }));
}

export class SwatchPick {
  readonly el: HTMLButtonElement;
  private name: string | null = null;
  constructor(title: string, onPick: (name: string) => void, small = false) {
    this.el = el<HTMLButtonElement>(`<button class="fp-swbtn${small ? ' is-sm' : ''}" type="button"><span class="fp-chip"></span><span class="fp-swbtn-name"></span>${glyph('caret', 12)}</button>`);
    setTitle(this.el, title);
    this.el.setAttribute('aria-label', title);
    this.el.addEventListener('click', () => menu.open(this.el, swatchItems(), this.name ?? '', onPick));
  }
  set(name: string | null, tint = 1): void {
    this.name = name;
    const sw = name === null ? null : ctx.snap.swatch.get(name) ?? null;
    paintChip(q(this.el, '.fp-chip'), name === null ? null : sw ?? { name, css: '#888', model: 'other', spot: false, values: [], locked: false }, tint);
    setText(q(this.el, '.fp-swbtn-name'), name === null ? 'Mixed' : name.replace(/^\[|\]$/g, ''));
    setClass(this.el, 'is-mixed', name === null);
  }
  setDisabled(off: boolean): void { setDisabled(this.el, off); }
}

export interface SegItem { id: string; icon?: string; text?: string; title: string }

export class Seg {
  readonly el: HTMLElement;
  private readonly btns = new Map<string, HTMLButtonElement>();
  constructor(items: SegItem[], onPick: (id: string) => void, cls = '') {
    this.el = el(`<div class="fp-seg${cls ? ` ${cls}` : ''}" role="group"></div>`);
    for (const it of items) {
      const b = el<HTMLButtonElement>(`<button type="button">${it.icon ? glyph(it.icon, 16) : ''}</button>`);
      if (it.text) b.appendChild(document.createTextNode(it.text));
      setTitle(b, it.title);
      b.setAttribute('aria-label', it.title);
      b.addEventListener('click', () => onPick(it.id));
      this.btns.set(it.id, b);
      this.el.appendChild(b);
    }
  }
  set(id: string | null): void { for (const [k, b] of this.btns) { setClass(b, 'is-on', k === id); b.setAttribute('aria-pressed', String(k === id)); } }
  setDisabled(off: boolean): void { for (const b of this.btns.values()) setDisabled(b, off); }
  button(id: string): HTMLButtonElement | undefined { return this.btns.get(id); }
}

export class Toggle {
  readonly el: HTMLButtonElement;
  on = false;
  constructor(icon: string, title: string, onToggle: (on: boolean) => void, text = '') {
    this.el = el<HTMLButtonElement>(`<button class="fp-tog" type="button" aria-pressed="false">${icon ? glyph(icon, 16) : ''}</button>`);
    if (text) this.el.appendChild(el(`<span class="fp-tog-text"></span>`));
    if (text) setText(q(this.el, '.fp-tog-text'), text);
    setTitle(this.el, title);
    this.el.setAttribute('aria-label', title);
    this.el.addEventListener('click', () => onToggle(!this.on));
  }
  set(on: boolean | null): void {
    this.on = !!on;
    setClass(this.el, 'is-on', !!on);
    setClass(this.el, 'is-mixed', on === null);
    this.el.setAttribute('aria-pressed', String(!!on));
  }
  setDisabled(off: boolean): void { setDisabled(this.el, off); }
}

export class Check {
  readonly el: HTMLLabelElement;
  readonly box: HTMLInputElement;
  constructor(label: string, onChange: (on: boolean) => void, title = '') {
    this.el = el<HTMLLabelElement>('<label class="fp-check"><input type="checkbox"><span></span></label>');
    setText(q(this.el, 'span'), label);
    if (title) setTitle(this.el, title);
    this.box = q<HTMLInputElement>(this.el, 'input');
    this.box.addEventListener('change', () => onChange(this.box.checked));
  }
  set(on: boolean | null): void {
    if (this.box.checked !== !!on) this.box.checked = !!on;
    if (this.box.indeterminate !== (on === null)) this.box.indeterminate = on === null;
  }
  setDisabled(off: boolean): void { setDisabled(this.box, off); setClass(this.el, 'is-off', off); }
}

export interface Opt { value: string; label: string }

export class Pick {
  readonly select: HTMLSelectElement;
  readonly el: HTMLElement;
  private keys = '';
  constructor(title: string, onChange: (v: string) => void, opts: { search?: string; cls?: string } = {}) {
    this.el = el(`<div class="fp-pick${opts.cls ? ` ${opts.cls}` : ''}"><select class="${opts.cls ?? ''}"></select></div>`);
    this.select = q<HTMLSelectElement>(this.el, 'select');
    this.select.setAttribute('aria-label', title);
    this.select.title = title;
    this.select.addEventListener('change', () => onChange(this.select.value));
    enhanceSelect(this.select, opts.search ? { search: opts.search } : {});
  }
  options(list: Opt[]): void {
    const key = JSON.stringify(list);
    if (key === this.keys) return;
    this.keys = key;
    const cur = this.select.value;
    patchList<Opt>(this.select, list, {
      key: (o) => o.value,
      create: (o) => { const n = document.createElement('option'); n.value = o.value; n.textContent = o.label; return n; },
      update: (n, o) => setText(n, o.label),
    });
    if (cur && this.select.value !== cur && list.some((o) => o.value === cur)) this.select.value = cur;
  }
  set(v: string | null): void {
    if (v !== null && !Array.from(this.select.options).some((o) => o.value === v)) {
      const n = document.createElement('option');
      n.value = v; n.textContent = v; n.dataset.key = v;
      this.select.appendChild(n);
      this.keys = '';
    }
    const want = v ?? '';
    if (this.select.value !== want) this.select.value = want;
    setClass(this.btn(), 'is-mixed', v === null);
  }
  private btn(): HTMLElement | null { return this.el.querySelector('.cv-dd-btn'); }
  setDisabled(off: boolean): void { this.select.disabled = off; }
}

const SEC_KEY = 'formdesign.props.sections';
function secState(): Record<string, boolean> { try { return JSON.parse(localStorage.getItem(SEC_KEY) ?? '{}') as Record<string, boolean>; } catch { return {}; } }

export class Section {
  readonly root: HTMLElement;
  readonly body: HTMLElement;
  readonly head: HTMLButtonElement;
  readonly tools: HTMLElement;
  constructor(readonly id: string, title: string) {
    this.root = el(`<section class="fp-sec"><div class="fp-sec-top"><button class="fp-sec-head" type="button" aria-expanded="true">${glyph('chevron', 14)}<span></span></button><span class="fp-sec-tools"></span></div><div class="fp-sec-body"></div></section>`);
    this.head = q<HTMLButtonElement>(this.root, '.fp-sec-head');
    this.body = q(this.root, '.fp-sec-body');
    this.tools = q(this.root, '.fp-sec-tools');
    setText(q(this.head, 'span:last-child'), title);
    const shut = secState()[id] === false;
    this.setOpen(!shut);
    this.head.addEventListener('click', () => {
      const open = this.head.getAttribute('aria-expanded') !== 'true';
      this.setOpen(open);
      try { const s = secState(); s[id] = open; localStorage.setItem(SEC_KEY, JSON.stringify(s)); } catch { }
    });
  }
  setOpen(open: boolean): void { this.head.setAttribute('aria-expanded', String(open)); setHidden(this.body, !open); setClass(this.root, 'is-shut', !open); }
  show(on: boolean): void { setHidden(this.root, !on); }
}

export function row(...children: HTMLElement[]): HTMLElement {
  const r = el('<div class="fp-row"></div>');
  r.append(...children);
  return r;
}

export function grid(...children: HTMLElement[]): HTMLElement {
  const r = el('<div class="fp-grid"></div>');
  r.append(...children);
  return r;
}

export function labelled(text: string, control: HTMLElement): HTMLElement {
  const r = el('<div class="fp-lrow"><span class="fp-lab"></span></div>');
  setText(q(r, '.fp-lab'), text);
  r.appendChild(control);
  return r;
}

export function iconBtn(icon: string, title: string, run: () => void, cls = ''): HTMLButtonElement {
  const b = el<HTMLButtonElement>(`<button class="fp-ibtn${cls ? ` ${cls}` : ''}" type="button">${glyph(icon, 16)}</button>`);
  setTitle(b, title);
  b.setAttribute('aria-label', title);
  b.addEventListener('click', run);
  return b;
}

export function same<T>(list: T[], f: (x: T) => unknown): unknown {
  if (!list.length) return null;
  const v = f(list[0]);
  return list.every((x) => f(x) === v) ? v : null;
}
