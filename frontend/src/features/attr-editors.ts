import { el, q } from '../ui/dom';
import { icon } from '../ui/icons';

export interface Lists { charStyles: string[]; swatches: string[] }

type Field =
  | { key: string; label: string; type: 'text' | 'number'; width?: number; def?: unknown; scale?: number }
  | { key: string; label: string; type: 'select'; options: () => Array<[string, string]>; def?: string }
  | { key: string; label: string; type: 'check'; def?: boolean };

const THROUGH: Array<[string, string]> = [['sentence', 'Sentences'], ['word', 'Words'], ['character', 'Characters'], ['letter', 'Letters'], ['digit', 'Digits'], ['tab', 'Tab Characters'], ['forcedLineBreak', 'Forced Line Break'], ['emSpace', 'Em Spaces'], ['enSpace', 'En Spaces'], ['anchoredObject', 'Anchored Object Marker'], [':', 'Colon'], ['-', 'Hyphen']];

function fieldNode(f: Field, v: unknown): { node: HTMLElement; read: () => unknown } {
  if (f.type === 'check') {
    const i = el<HTMLInputElement>('<input type="checkbox">');
    i.checked = v === undefined ? !!f.def : v === true;
    const l = el('<label class="fae-check"></label>');
    l.append(i, document.createTextNode(f.label));
    return { node: l, read: () => i.checked };
  }
  if (f.type === 'select') {
    const s = el<HTMLSelectElement>('<select></select>');
    s.setAttribute('aria-label', f.label);
    const opts = f.options();
    for (const [val, lab] of opts) s.append(new Option(lab, val));
    const cur = typeof v === 'string' ? v : f.def ?? opts[0]?.[0] ?? '';
    if (cur && !opts.some(([o]) => o === cur)) s.append(new Option(cur, cur));
    s.value = cur;
    return { node: s, read: () => s.value };
  }
  const i = el<HTMLInputElement>(`<input type="${f.type}" spellcheck="false" ${f.type === 'number' ? 'step="any"' : ''}>`);
  i.setAttribute('aria-label', f.label);
  i.placeholder = f.label;
  if (f.width) i.style.width = `${f.width}px`;
  const sc = f.type === 'number' ? f.scale ?? 1 : 1;
  const raw = v ?? f.def;
  i.value = raw == null ? '' : f.type === 'number' ? String(Math.round(Number(raw) * sc * 1000) / 1000) : String(raw);
  return { node: i, read: () => (f.type === 'number' ? (i.value.trim() === '' ? (f.def ?? 0) : Number(i.value) / sc) : i.value) };
}

function listEditor(fields: Field[], value: unknown, addLabel: string): { node: HTMLElement; read: () => unknown } {
  const root = el('<div class="fae-list"><div class="fae-rows"></div></div>');
  const rows = q(root, '.fae-rows');
  const readers: Array<() => Record<string, unknown>> = [];
  const add = (item: Record<string, unknown>) => {
    const row = el('<div class="fae-row"></div>');
    const rs = fields.map((f) => { const n = fieldNode(f, item[f.key]); row.appendChild(n.node); return [f.key, n.read] as const; });
    const rm = el<HTMLButtonElement>(`<button class="cv-icon-btn fae-rm" type="button" title="Remove" aria-label="Remove">${icon('remove', 14)}</button>`);
    const read = () => Object.fromEntries(rs.map(([k, r]) => [k, r()]));
    readers.push(read);
    rm.addEventListener('click', () => { const i = readers.indexOf(read); if (i >= 0) readers.splice(i, 1); row.remove(); });
    row.appendChild(rm);
    rows.appendChild(row);
  };
  for (const it of Array.isArray(value) ? value : []) if (it && typeof it === 'object') add(it as Record<string, unknown>);
  const b = el<HTMLButtonElement>(`<button class="cv-btn fae-add" type="button">${icon('add', 14)}<span class="fae-add-label"></span></button>`);
  q(b, '.fae-add-label').textContent = addLabel;
  b.addEventListener('click', () => add({}));
  root.appendChild(b);
  return { node: root, read: () => readers.map((r) => r()) };
}

function objectEditor(fields: Field[], value: unknown): { node: HTMLElement; read: () => unknown } {
  const root = el('<div class="fae-obj"></div>');
  const v = (value && typeof value === 'object' ? value : {}) as Record<string, unknown>;
  const rs = fields.map((f) => {
    const n = fieldNode(f, f.key === 'on' && value != null && v.on === undefined ? true : v[f.key]);
    const wrap = f.type === 'check' ? n.node : el('<label class="fae-f"><span></span></label>');
    if (f.type !== 'check') { q(wrap, 'span').textContent = f.label; wrap.appendChild(n.node); }
    root.appendChild(wrap);
    return [f.key, n.read] as const;
  });
  return { node: root, read: () => { const o = Object.fromEntries(rs.map(([k, r]) => [k, r()])); return o.on === false ? { on: false } : o; } };
}

function triple(value: unknown, scale: number, unit: string, defs: [number, number, number]): { node: HTMLElement; read: () => unknown } {
  const v = Array.isArray(value) && value.length === 3 ? value.map(Number) : defs;
  const root = el('<div class="fae-row"></div>');
  const ins = ['Minimum', 'Desired', 'Maximum'].map((label, i) => {
    const w = el(`<label class="fae-f"><span></span><input type="number" step="any"></label>`);
    q(w, 'span').textContent = `${label} (${unit})`;
    const inp = q<HTMLInputElement>(w, 'input');
    inp.value = String(Math.round(v[i] * scale * 100) / 100);
    root.appendChild(w);
    return inp;
  });
  return { node: root, read: () => ins.map((inp, i) => (inp.value.trim() === '' ? defs[i] : Number(inp.value) / scale)) };
}

export function structuredEditor(name: string, value: unknown, lists: Lists): { node: HTMLElement; read: () => unknown } | null {
  if (name === 'wordSpace') return triple(value, 100, '%', [0.8, 1, 1.33]);
  if (name === 'letterSpace') return triple(value, 100, '% em', [0, 0, 0]);
  if (name === 'glyphScale') return triple(value, 100, '%', [1, 1, 1]);
  const chars = (): Array<[string, string]> => lists.charStyles.map((s) => [s, s.replace(/^\[(.*)\]$/, '$1')]);
  const sw = (): Array<[string, string]> => lists.swatches.map((s) => [s, s.replace(/^\[(.*)\]$/, '$1')]);
  if (name === 'grep') return listEditor([{ key: 'style', label: 'Apply Style', type: 'select', options: chars }, { key: 'pattern', label: 'To Text (GREP)', type: 'text', width: 220 }], value, 'New GREP Style');
  if (name === 'nested') return listEditor([
    { key: 'style', label: 'Character Style', type: 'select', options: chars },
    { key: 'inclusive', label: 'through', type: 'check', def: true },
    { key: 'count', label: 'Count', type: 'number', width: 56, def: 1 },
    { key: 'through', label: 'Delimiter', type: 'select', options: () => THROUGH, def: 'sentence' },
  ], value, 'New Nested Style');
  if (name === 'tabs') return listEditor([
    { key: 'position', label: 'Position (pt)', type: 'number', width: 80 },
    { key: 'align', label: 'Align', type: 'select', options: () => [['left', 'Left'], ['center', 'Center'], ['right', 'Right'], ['decimal', 'Decimal']] },
    { key: 'leader', label: 'Leader', type: 'text', width: 50 },
    { key: 'alignOn', label: 'Align On', type: 'text', width: 40, def: '.' },
  ], value, 'New Tab Stop');
  if (name === 'ruleAbove' || name === 'ruleBelow') return objectEditor([
    { key: 'on', label: name === 'ruleAbove' ? 'Rule Above On' : 'Rule Below On', type: 'check' },
    { key: 'weight', label: 'Weight (pt)', type: 'number', def: 1 },
    { key: 'color', label: 'Colour', type: 'select', options: sw, def: 'Black' },
    { key: 'tint', label: 'Tint (%)', type: 'number', def: 1, scale: 100 },
    { key: 'width', label: 'Width', type: 'select', options: () => [['column', 'Column'], ['text', 'Text']], def: 'column' },
    { key: 'offset', label: 'Offset (pt)', type: 'number', def: 0 },
    { key: 'left', label: 'Left Indent', type: 'number', def: 0 },
    { key: 'right', label: 'Right Indent', type: 'number', def: 0 },
  ], value);
  if (name === 'shading') return objectEditor([
    { key: 'on', label: 'Shading On', type: 'check' },
    { key: 'color', label: 'Colour', type: 'select', options: sw, def: 'Black' },
    { key: 'tint', label: 'Tint (%)', type: 'number', def: 0.2, scale: 100 },
    { key: 'top', label: 'Top Offset', type: 'number', def: 0 },
    { key: 'bottom', label: 'Bottom Offset', type: 'number', def: 0 },
    { key: 'left', label: 'Left Offset', type: 'number', def: 0 },
    { key: 'right', label: 'Right Offset', type: 'number', def: 0 },
  ], value);
  return null;
}

export function gradientStops(value: unknown, lists: Lists): { node: HTMLElement; read: () => unknown } {
  const sw = (): Array<[string, string]> => lists.swatches.filter((s) => !/^\[(None|Registration)\]$/.test(s)).map((s) => [s, s.replace(/^\[(.*)\]$/, '$1')]);
  const init = Array.isArray(value) && value.length ? value : [{ color: lists.swatches.includes('[Paper]') ? '[Paper]' : 'Paper', at: 0 }, { color: lists.swatches.includes('[Black]') ? '[Black]' : 'Black', at: 100 }];
  return listEditor([
    { key: 'color', label: 'Stop Colour', type: 'select', options: sw },
    { key: 'at', label: 'Location (%)', type: 'number', width: 70, def: 0 },
    { key: 'opacity', label: 'Opacity (%)', type: 'number', width: 70, def: 100 },
  ], init, 'Add Stop');
}
