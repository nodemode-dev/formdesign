import { editor } from '../editor/editor';
import { Dialog } from '../ui/dialogs';
import { el, q, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';
import { swatchNames } from './colour-tools';

// Table > Table Style Options and Cell Style Options as structured forms, in place of the generic command form with a JSON
// box. The attributes are the ones the layout core reads (table styles: border and altRows; cell styles: fill, fillTint,
// insets, vj and per-side strokes; see formdes-render scene.rs and formdes-engine cmd/table.rs). Each form is built once; picking
// another style patches the controls. Only attributes that changed are sent, and an attribute the form does not know about
// is never touched, so a style made by a script or an agent keeps everything else it has.

type Obj = Record<string, unknown>;
interface Ctl { node: HTMLElement; get: () => unknown; set: (v: unknown) => void }

const TYPES: Array<[string, string]> = [['solid', 'Solid'], ['dashed', 'Dashed'], ['dotted', 'Dotted'], ['none', 'None']];
const VJ: Array<[string, string]> = [['top', 'Top'], ['center', 'Centre'], ['bottom', 'Bottom'], ['justify', 'Justify']];
const SIDES: Array<[string, string]> = [['top', 'Top'], ['bottom', 'Bottom'], ['left', 'Left'], ['right', 'Right']];

const obj = (v: unknown): Obj => (v && typeof v === 'object' && !Array.isArray(v) ? { ...(v as Obj) } : {});
const same = (a: unknown, b: unknown): boolean => JSON.stringify(a ?? null) === JSON.stringify(b ?? null);

function field(label: string, control: HTMLElement): HTMLElement {
  const l = el('<label class="fae-f"><span></span></label>');
  setText(q(l, 'span'), label);
  l.appendChild(control);
  return l;
}

function num(label: string, o: { scale?: number; min?: number; ph?: string } = {}): Ctl {
  const i = el<HTMLInputElement>('<input type="number" step="any" spellcheck="false">');
  i.setAttribute('aria-label', label);
  if (o.min !== undefined) i.min = String(o.min);
  if (o.ph) i.placeholder = o.ph;
  const sc = o.scale ?? 1;
  return {
    node: field(label, i),
    get: () => (i.value.trim() === '' || !Number.isFinite(Number(i.value)) ? null : Number(i.value) / sc),
    set: (v) => { i.value = typeof v === 'number' && Number.isFinite(v) ? String(Math.round(v * sc * 1000) / 1000) : ''; },
  };
}

function sel(label: string, options: () => Array<[string, string]>): Ctl & { refill: () => void } {
  const s = el<HTMLSelectElement>('<select class="cv-select"></select>');
  s.setAttribute('aria-label', label);
  const refill = (): void => {
    const cur = s.value;
    s.replaceChildren(...options().map(([v, l]) => new Option(l, v)));
    if (cur && [...s.options].some((o) => o.value === cur)) s.value = cur;
  };
  refill();
  return {
    node: field(label, s), refill,
    get: () => s.value,
    set: (v) => {
      let want = typeof v === 'string' ? v : '';
      const key = (x: string): string => x.replace(/^\[(.*)\]$/, '$1').toLowerCase();
      // A style may name a swatch with or without the brackets the list shows; either picks the same entry. The stored
      // spelling is kept when the person leaves the control alone (changes() compares against it).
      const hit = [...s.options].find((o) => o.value === want) ?? [...s.options].find((o) => want && key(o.value) === key(want));
      if (hit) want = hit.value; else if (want) s.append(new Option(want.replace(/^\[(.*)\]$/, '$1'), want));
      s.value = want || s.options[0]?.value || '';
    },
  };
}

function chk(label: string): Ctl & { input: HTMLInputElement } {
  const i = el<HTMLInputElement>('<input type="checkbox">');
  const l = el('<label class="fae-check"></label>');
  l.append(i, document.createTextNode(label));
  return { node: l, input: i, get: () => i.checked, set: (v) => { i.checked = v === true; } };
}

function group(title: string, ...nodes: HTMLElement[]): { node: HTMLElement; box: HTMLElement } {
  const d = el('<details class="fto-sec" open><summary></summary><div class="fae-obj"></div></details>');
  setText(q(d, 'summary'), title);
  const box = q(d, '.fae-obj');
  box.append(...nodes);
  return { node: d, box };
}

interface Form {
  nodes: HTMLElement[];
  load: (attrs: Obj) => void;
  /** The attributes whose value now differs from `before`, null meaning remove. */
  changes: (before: Obj) => Obj;
  refill: () => void;
}

function tableForm(swatches: () => string[]): Form {
  const colours = (none: boolean) => (): Array<[string, string]> => [...(none ? [['[None]', 'None'] as [string, string]] : []), ...swatches().map((s): [string, string] => [s, s.replace(/^\[(.*)\]$/, '$1')])];
  const bw = num('Weight (pt)', { min: 0, ph: '1' });
  const bc = sel('Colour', colours(false));
  const alt = chk('Alternating row fills');
  const f1 = num('First rows', { min: 1, ph: '1' }), f1c = sel('First colour', colours(true)), f1t = num('First tint (%)', { scale: 100, min: 0, ph: '100' });
  const f2 = num('Next rows', { min: 1, ph: '1' }), f2c = sel('Next colour', colours(true)), f2t = num('Next tint (%)', { scale: 100, min: 0, ph: '100' });
  const border = group('Table border', bw.node, bc.node);
  const rows = group('Alternating rows', alt.node, f1.node, f1c.node, f1t.node, f2.node, f2c.node, f2t.node);
  const ctls = [f1, f1c, f1t, f2, f2c, f2t];
  const sync = (): void => { for (const c of ctls) for (const x of c.node.querySelectorAll<HTMLInputElement | HTMLSelectElement>('input, select')) x.disabled = !alt.input.checked; };
  alt.input.addEventListener('change', sync);
  return {
    nodes: [border.node, rows.node],
    refill: () => { bc.refill(); f1c.refill(); f2c.refill(); },
    load: (a) => {
      const b = obj(a.border);
      bw.set(b.weight);
      bc.set(typeof b.color === 'string' ? b.color : '[Black]');
      const r = a.altRows && typeof a.altRows === 'object' ? obj(a.altRows) : null;
      alt.set(!!r);
      const x = r ?? {};
      f1.set(x.first ?? 1); f1c.set(typeof x.firstColor === 'string' ? x.firstColor : '[None]'); f1t.set(x.firstTint ?? 1);
      f2.set(x.next ?? 1); f2c.set(typeof x.nextColor === 'string' ? x.nextColor : '[None]'); f2t.set(x.nextTint ?? 1);
      sync();
    },
    changes: (before) => {
      const out: Obj = {};
      const b = obj(before.border);
      const w = bw.get() as number | null;
      if (w !== null) b.weight = Math.max(0, w);
      const c = bc.get() as string;
      if (c && (typeof before.border === 'object' || c !== '[Black]')) b.color = c;
      if (Object.keys(b).length && !same(b, before.border)) out.border = b;
      if (!alt.input.checked) {
        if (before.altRows != null) out.altRows = null;
      } else {
        const a = obj(before.altRows);
        a.first = Math.max(1, Math.round((f1.get() as number | null) ?? 1));
        a.next = Math.max(1, Math.round((f2.get() as number | null) ?? 1));
        for (const [key, ctl, tint] of [['first', f1c, f1t], ['next', f2c, f2t]] as const) {
          const col = ctl.get() as string;
          if (col && col !== '[None]') a[`${key}Color`] = col; else delete a[`${key}Color`];
          const t = tint.get() as number | null;
          if (t === null || Math.abs(t - 1) < 1e-9) delete a[`${key}Tint`]; else a[`${key}Tint`] = Math.min(1, Math.max(0, t));
        }
        if (!same(a, before.altRows)) out.altRows = a;
      }
      return out;
    },
  };
}

function cellForm(swatches: () => string[]): Form {
  const colours = (none: boolean) => (): Array<[string, string]> => [...(none ? [['[None]', 'None'] as [string, string]] : []), ...swatches().map((s): [string, string] => [s, s.replace(/^\[(.*)\]$/, '$1')])];
  const fill = sel('Fill colour', colours(true)), tint = num('Fill tint (%)', { scale: 100, min: 0, ph: '100' });
  const ins = [['Top inset (pt)', 'top'], ['Left inset (pt)', 'left'], ['Bottom inset (pt)', 'bottom'], ['Right inset (pt)', 'right']].map(([l]) => num(l, { min: 0, ph: '4' }));
  const vj = sel('Vertical justification', () => VJ);
  const sides = SIDES.map(([key, label]) => {
    const on = chk(`${label} stroke overrides the table border`);
    const w = num(`${label} weight (pt)`, { min: 0 }), c = sel(`${label} colour`, () => [['', 'Table border colour'], ...swatches().filter((s) => s !== '[None]').map((s): [string, string] => [s, s.replace(/^\[(.*)\]$/, '$1')])]);
    const t = num(`${label} tint (%)`, { scale: 100, min: 0, ph: '100' }), ty = sel(`${label} line type`, () => TYPES);
    const parts = [w, c, t, ty];
    const sync = (): void => { for (const p of parts) for (const x of p.node.querySelectorAll<HTMLInputElement | HTMLSelectElement>('input, select')) x.disabled = !on.input.checked; };
    on.input.addEventListener('change', sync);
    const g = group(`${label} edge`, on.node, ...parts.map((p) => p.node));
    return { key, on, w, c, t, ty, sync, node: g.node };
  });
  return {
    nodes: [group('Fill', fill.node, tint.node).node, group('Cell insets', ...ins.map((i) => i.node)).node, group('Alignment', vj.node).node, ...sides.map((s) => s.node)],
    refill: () => { fill.refill(); for (const s of sides) s.c.refill(); },
    load: (a) => {
      fill.set(typeof a.fill === 'string' ? a.fill : '[None]');
      tint.set(a.fillTint ?? 1);
      const iv = a.insets;
      const four = typeof iv === 'number' ? [iv, iv, iv, iv] : Array.isArray(iv) && iv.length === 4 ? iv : [4, 4, 4, 4];
      ins.forEach((c, i) => c.set(four[i]));
      vj.set(typeof a.vj === 'string' ? a.vj : 'top');
      const st = obj(a.strokes);
      for (const s of sides) {
        const o = st[s.key] && typeof st[s.key] === 'object' ? obj(st[s.key]) : null;
        s.on.set(!!o);
        const x = o ?? {};
        s.w.set(x.weight); s.c.set(typeof x.color === 'string' ? x.color : ''); s.t.set(x.tint ?? 1); s.ty.set(typeof x.type === 'string' ? x.type : 'solid');
        s.sync();
      }
    },
    changes: (before) => {
      const out: Obj = {};
      const f = fill.get() as string;
      const nf = f === '[None]' ? null : f;
      const bare = (x: unknown): unknown => (typeof x === 'string' ? x.replace(/^\[(.*)\]$/, '$1').toLowerCase() : x ?? null);
      if (!same(bare(nf), bare(before.fill))) out.fill = nf;
      const t = tint.get() as number | null;
      const nt = nf === null || t === null || Math.abs(t - 1) < 1e-9 ? null : Math.min(1, Math.max(0, t));
      if (!same(nt, before.fillTint)) out.fillTint = nt;
      const iv = ins.map((c) => c.get() as number | null);
      if (iv.some((v) => v !== null)) {
        const arr = iv.map((v) => Math.max(0, v ?? 4));
        const cur = before.insets;
        const was = typeof cur === 'number' ? [cur, cur, cur, cur] : Array.isArray(cur) && cur.length === 4 ? cur : null;
        if (!same(arr, was) && !(was === null && arr.every((v) => v === 4))) out.insets = arr;
      }
      const v = vj.get() as string;
      if (v !== (typeof before.vj === 'string' ? before.vj : 'top') || (before.vj === undefined && v !== 'top')) out.vj = v;
      const old = obj(before.strokes);
      const next: Obj = {};
      for (const s of sides) {
        if (!s.on.input.checked) continue;
        const o = obj(old[s.key]);
        const w = s.w.get() as number | null;
        if (w !== null) o.weight = Math.max(0, w);
        const c = s.c.get() as string;
        if (c) o.color = c; else delete o.color;
        const tt = s.t.get() as number | null;
        if (tt === null || Math.abs(tt - 1) < 1e-9) delete o.tint; else o.tint = Math.min(1, Math.max(0, tt));
        o.type = s.ty.get();
        next[s.key] = o;
      }
      // Sides this form does not offer (a style may carry others) stay as they were.
      for (const k of Object.keys(old)) if (!SIDES.some(([s]) => s === k)) next[k] = old[k];
      const ns = Object.keys(next).length ? next : null;
      if (!same(ns, before.strokes)) out.strokes = ns;
      return out;
    },
  };
}

async function open(kind: 'table' | 'cell', ws: Workspace, params: Record<string, unknown> = {}): Promise<void> {
  const list = await editor.exec<Record<string, string[]>>('style.list', {}, true).catch(() => undefined);
  const names = (list?.[kind] ?? []).filter((n) => !/^\[?(None|No Table Style)\]?$/i.test(n));
  if (!names.length) { toast(`This document has no ${kind} styles yet. Create one first.`, 'error'); return; }
  let swatchList: string[] = [];
  try { swatchList = swatchNames(await editor.exec<unknown>('swatch.list', {}, true)).filter((s) => s !== '[None]'); } catch { swatchList = []; }
  const d = new Dialog(kind === 'table' ? 'Table Style Options' : 'Cell Style Options', 520);
  const form: Form = kind === 'table' ? tableForm(() => swatchList) : cellForm(() => swatchList);
  const top = el('<div class="fo-form"><label class="fo-field"><span>Style</span><select class="fts-style cv-select"></select></label><label class="fo-field"><span>Style Name</span><input class="fts-name" type="text" spellcheck="false"></label></div>');
  const pick = q<HTMLSelectElement>(top, '.fts-style');
  const name = q<HTMLInputElement>(top, '.fts-name');
  pick.replaceChildren(...names.map((n) => new Option(n.replace(/^\[(.*)\]$/, '$1'), n)));
  const want = typeof params.name === 'string' ? params.name : '';
  pick.value = names.includes(want) ? want : names.find((n) => !n.startsWith('[')) ?? names[0];
  const body = el('<div class="fto-body"></div>');
  body.append(...form.nodes);
  d.body.append(top, body);
  let before: Obj = {};
  let seq = 0;
  const load = async (): Promise<void> => {
    const mine = ++seq;
    const r = await editor.exec<{ attrs?: Obj }>('style.get', { kind, name: pick.value }, true).catch(() => undefined);
    if (mine !== seq) return;
    before = obj(r?.attrs);
    name.value = pick.value.replace(/^\[(.*)\]$/, '$1');
    form.refill();
    form.load(before);
  };
  pick.addEventListener('change', () => void load());
  d.button('Cancel', '', () => d.close());
  d.button('OK', 'primary', () => {
    const attrs = form.changes(before);
    const current = pick.value;
    const rename = name.value.trim();
    const p: Record<string, unknown> = { name: current, ...(kind === 'cell' ? { kind: 'cell' } : {}) };
    if (Object.keys(attrs).length) p.attrs = attrs;
    if (rename && rename !== current.replace(/^\[(.*)\]$/, '$1')) p.rename = rename;
    d.close();
    if (!p.attrs && !p.rename) return;
    void ws.runCore(kind === 'table' ? 'style.table.edit' : 'style.cell.edit', p);
  });
  d.onClose = () => d.scrim.remove();
  await load();
  d.open(pick);
}

export function installTableStyles(ws: Workspace): void {
  ws.override('style.table.edit', (p) => void open('table', ws, p));
  ws.override('style.cell.edit', (p) => void open('cell', ws, p));
}
