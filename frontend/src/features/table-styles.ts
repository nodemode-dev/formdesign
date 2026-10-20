import { editor } from '../editor/editor';
import { Dialog } from '../ui/dialogs';
import { el, q, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';
import { swatchNames } from './colour-tools';

// Table > Table Style Options and Cell Style Options as structured forms, in place of the generic command form with a JSON
// box. The attributes are the ones the layout core reads (table styles: border and altRows; cell styles: fill, fillTint,
// insets, vj and per-side strokes; see formdesign-render scene.rs and formdesign-engine cmd/table.rs). Each form is built once; picking
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

const colourOpts = (swatches: () => string[], none: boolean) => (): Array<[string, string]> => [...(none ? [['[None]', 'None'] as [string, string]] : []), ...swatches().map((s): [string, string] => [s, s.replace(/^\[(.*)\]$/, '$1')])];

interface Alt { node: HTMLElement; refill: () => void; load: (a: Obj) => void; put: (before: Obj, out: Obj) => void }

/** One alternating pattern of a table style: key is altRows, altCols, altRowStrokes or altColStrokes. */
function altBlock(title: string, key: string, stroke: boolean, skips: boolean, swatches: () => string[]): Alt {
  const on = chk(`Use ${title.toLowerCase()}`);
  const sides = ([['first', 'First'], ['next', 'Next']] as const).map(([k, l]) => ({
    k,
    n: num(`${l} count`, { min: 1, ph: '1' }),
    c: sel(`${l} colour`, colourOpts(swatches, true)),
    t: num(`${l} tint (%)`, { scale: 100, min: 0, ph: '100' }),
    w: num(`${l} weight (pt)`, { min: 0, ph: '1' }),
    ty: sel(`${l} line type`, () => TYPES),
  }));
  const skipF = num('Skip first rows', { min: 0, ph: '0' }), skipL = num('Skip last rows', { min: 0, ph: '0' });
  const ctls: Ctl[] = [...sides.flatMap((s) => (stroke ? [s.n, s.w, s.c, s.t, s.ty] : [s.n, s.c, s.t])), ...(skips ? [skipF, skipL] : [])];
  const g = group(title, on.node, ...ctls.map((c) => c.node));
  const sync = (): void => { for (const c of ctls) for (const x of c.node.querySelectorAll<HTMLInputElement | HTMLSelectElement>('input, select')) x.disabled = !on.input.checked; };
  on.input.addEventListener('change', sync);
  return {
    node: g.node,
    refill: () => { for (const s of sides) s.c.refill(); },
    load: (a) => {
      const r = a[key] && typeof a[key] === 'object' ? obj(a[key]) : null;
      on.set(!!r);
      const x = r ?? {};
      for (const s of sides) {
        s.n.set(x[s.k] ?? 1); s.c.set(typeof x[`${s.k}Color`] === 'string' ? x[`${s.k}Color`] : '[None]'); s.t.set(x[`${s.k}Tint`] ?? 1);
        s.w.set(x[`${s.k}Weight`]); s.ty.set(typeof x[`${s.k}Type`] === 'string' ? x[`${s.k}Type`] : 'solid');
      }
      skipF.set(x.skipFirst ?? 0); skipL.set(x.skipLast ?? 0);
      sync();
    },
    put: (before, out) => {
      if (!on.input.checked) {
        if (before[key] != null) out[key] = null;
        return;
      }
      const a = obj(before[key]);
      for (const s of sides) {
        a[s.k] = Math.max(1, Math.round((s.n.get() as number | null) ?? 1));
        const col = s.c.get() as string;
        if (col && col !== '[None]') a[`${s.k}Color`] = col; else delete a[`${s.k}Color`];
        const t = s.t.get() as number | null;
        if (t === null || Math.abs(t - 1) < 1e-9) delete a[`${s.k}Tint`]; else a[`${s.k}Tint`] = Math.min(1, Math.max(0, t));
        if (stroke) {
          const w = s.w.get() as number | null;
          if (w === null) delete a[`${s.k}Weight`]; else a[`${s.k}Weight`] = Math.max(0, w);
          const ty = s.ty.get() as string;
          if (ty && ty !== 'solid') a[`${s.k}Type`] = ty; else delete a[`${s.k}Type`];
        }
      }
      if (skips) {
        for (const [k, c] of [['skipFirst', skipF], ['skipLast', skipL]] as const) {
          const v = c.get() as number | null;
          if (v === null || v < 1) delete a[k]; else a[k] = Math.round(v);
        }
      }
      if (!same(a, before[key])) out[key] = a;
    },
  };
}

const REGIONS: Array<[string, string]> = [
  ['headerCellStyle', 'Header rows'], ['footerCellStyle', 'Footer rows'], ['leftColumnCellStyle', 'Left column'],
  ['rightColumnCellStyle', 'Right column'], ['bodyCellStyle', 'Body cells'],
];
const ROTATE: Array<[string, string]> = [['0', 'None'], ['90', '90 degrees'], ['180', '180 degrees'], ['270', '270 degrees']];

function tableForm(swatches: () => string[], cellStyles: () => string[]): Form {
  const bw = num('Weight (pt)', { min: 0, ph: '1' });
  const bc = sel('Colour', colourOpts(swatches, false));
  const border = group('Table border', bw.node, bc.node);
  const alts = [
    altBlock('Alternating row fills', 'altRows', false, true, swatches),
    altBlock('Alternating column fills', 'altCols', false, false, swatches),
    altBlock('Alternating row strokes', 'altRowStrokes', true, false, swatches),
    altBlock('Alternating column strokes', 'altColStrokes', true, false, swatches),
  ];
  const regions = REGIONS.map(([key, label]) => ({ key, ctl: sel(`${label} cell style`, () => [['', 'None'], ...cellStyles().map((n): [string, string] => [n, n.replace(/^\[(.*)\]$/, '$1')])]) }));
  const reg = group('Cell styles by region', ...regions.map((r) => r.ctl.node));
  return {
    nodes: [border.node, ...alts.map((a) => a.node), reg.node],
    refill: () => { bc.refill(); for (const a of alts) a.refill(); for (const r of regions) r.ctl.refill(); },
    load: (a) => {
      const b = obj(a.border);
      bw.set(b.weight);
      bc.set(typeof b.color === 'string' ? b.color : '[Black]');
      for (const x of alts) x.load(a);
      for (const r of regions) r.ctl.set(typeof a[r.key] === 'string' ? a[r.key] : '');
    },
    changes: (before) => {
      const out: Obj = {};
      const b = obj(before.border);
      const w = bw.get() as number | null;
      if (w !== null) b.weight = Math.max(0, w);
      const c = bc.get() as string;
      if (c && (typeof before.border === 'object' || c !== '[Black]')) b.color = c;
      if (Object.keys(b).length && !same(b, before.border)) out.border = b;
      for (const x of alts) x.put(before, out);
      for (const r of regions) {
        const v = r.ctl.get() as string;
        const was = typeof before[r.key] === 'string' ? (before[r.key] as string) : '';
        if (v !== was) out[r.key] = v || null;
      }
      return out;
    },
  };
}

function cellForm(swatches: () => string[], paraStyles: () => string[]): Form {
  const colours = (none: boolean) => colourOpts(swatches, none);
  const fill = sel('Fill colour', colours(true)), tint = num('Fill tint (%)', { scale: 100, min: 0, ph: '100' });
  const para = sel('Paragraph style', () => [['', 'None'], ...paraStyles().map((n): [string, string] => [n, n.replace(/^\[(.*)\]$/, '$1')])]);
  const rot = sel('Text rotation', () => ROTATE);
  const diag = (['down', 'up'] as const).map((key) => {
    const label = key === 'down' ? 'Top left to bottom right' : 'Top right to bottom left';
    const on = chk(`${label} diagonal line`);
    const w = num(`${label} weight (pt)`, { min: 0, ph: '1' }), c = sel(`${label} colour`, colours(true));
    const sync = (): void => { for (const p of [w, c]) for (const x of p.node.querySelectorAll<HTMLInputElement | HTMLSelectElement>('input, select')) x.disabled = !on.input.checked; };
    on.input.addEventListener('change', sync);
    return { key, on, w, c, sync, node: group(label, on.node, w.node, c.node).node };
  });
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
    nodes: [group('Fill', fill.node, tint.node).node, group('Cell insets', ...ins.map((i) => i.node)).node, group('Text', para.node, vj.node, rot.node).node, ...sides.map((s) => s.node), ...diag.map((d) => d.node)],
    refill: () => { fill.refill(); para.refill(); for (const s of sides) s.c.refill(); for (const d of diag) d.c.refill(); },
    load: (a) => {
      para.set(typeof a.paraStyle === 'string' ? a.paraStyle : '');
      rot.set(String(typeof a.rotate === 'number' ? ((a.rotate % 360) + 360) % 360 : 0));
      const dg = obj(a.diagonal);
      for (const d of diag) {
        const o = dg[d.key] && typeof dg[d.key] === 'object' ? obj(dg[d.key]) : null;
        d.on.set(!!o);
        d.w.set(o?.weight); d.c.set(typeof o?.color === 'string' ? o.color : '[None]');
        d.sync();
      }
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
      const ps = para.get() as string;
      if (ps !== (typeof before.paraStyle === 'string' ? before.paraStyle : '')) out.paraStyle = ps || null;
      const rv = Number(rot.get());
      const rwas = typeof before.rotate === 'number' ? ((before.rotate % 360) + 360) % 360 : 0;
      if (rv !== rwas) out.rotate = rv || null;
      const dold = obj(before.diagonal);
      const dnext: Obj = { ...dold };
      for (const d of diag) {
        if (!d.on.input.checked) { delete dnext[d.key]; continue; }
        const o = obj(dold[d.key]);
        const w = d.w.get() as number | null;
        if (w !== null) o.weight = Math.max(0, w);
        const c = d.c.get() as string;
        if (c && c !== '[None]') o.color = c; else delete o.color;
        dnext[d.key] = o;
      }
      const hasDiag = diag.some((d) => dnext[d.key]);
      const dn = hasDiag ? dnext : null;
      if (!same(dn, Object.keys(dold).length ? dold : null)) out.diagonal = dn;
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
  const cellNames = (list?.cell ?? []).filter((n) => !/^\[?None\]?$/i.test(n));
  const paraNames = (list?.paragraph ?? []).filter((n) => !/^\[?(None|No Paragraph Style)\]?$/i.test(n));
  const form: Form = kind === 'table' ? tableForm(() => swatchList, () => cellNames) : cellForm(() => swatchList, () => paraNames);
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
