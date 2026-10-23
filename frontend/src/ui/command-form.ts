import { editor, type CoreCommand } from '../editor/editor';
import { Dialog } from './dialogs';
import { el, q, setText } from './dom';
import { toast } from './toast';

export type FieldKind = 'choice' | 'number' | 'bool' | 'json' | 'text' | 'path' | 'paths' | 'entries' | 'features';

const FEATURES: Array<[string, string]> = [
  ['liga', 'Standard ligatures'], ['dlig', 'Discretionary ligatures'], ['swsh', 'Swash'], ['salt', 'Stylistic alternates'], ['titl', 'Titling alternates'],
  ['smcp', 'Small caps'], ['c2sc', 'Capitals to small caps'], ['frac', 'Fractions'], ['ordn', 'Ordinals'], ['sups', 'Superscript'], ['subs', 'Subscript'],
  ['onum', 'Oldstyle figures'], ['lnum', 'Lining figures'], ['tnum', 'Tabular figures'], ['pnum', 'Proportional figures'], ['zero', 'Slashed zero'],
  ['ss01', 'Stylistic set 1'], ['ss02', 'Stylistic set 2'], ['ss03', 'Stylistic set 3'], ['ss04', 'Stylistic set 4'], ['ss05', 'Stylistic set 5'], ['ss06', 'Stylistic set 6'], ['ss07', 'Stylistic set 7'],
];

const EXT: Array<[RegExp, string, string]> = [[/Svg$/, '.svg', 'image/svg+xml'], [/Idml$/, '.idml', 'application/vnd.adobe.indesign-idml-package'], [/(Pdf|Booklet|print)$/, '.pdf', 'application/pdf']];

function download(cmdId: string, b64: string): void {
  const [, ext, mime] = EXT.find(([re]) => re.test(cmdId)) ?? [/./, '.bin', 'application/octet-stream'];
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const url = URL.createObjectURL(new Blob([bytes], { type: mime }));
  const a = document.createElement('a');
  a.href = url;
  a.download = `${(editor.doc?.title ?? 'FormDesign').replace(/[\\/:*?"<>|]+/g, '-')}${ext}`;
  document.body.appendChild(a);
  a.click();
  a.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 4000);
}

export interface Field { name: string; optional: boolean; desc: string; kind: FieldKind; choices: string[]; group: number }

export interface FormHost {
  pickFiles(accept: string, multiple?: boolean): Promise<File[]>;
  upload(files: File[]): Promise<string[]>;
  runCore(id: string, params?: Record<string, unknown>): Promise<unknown>;
}

const NUMERIC = new Set(['count', 'dx', 'dy', 'rows', 'columns', 'cols', 'width', 'height', 'size', 'angle', 'weight', 'radius', 'sides', 'tint', 'offset', 'page', 'level', 'copies', 'gutter', 'rowGutter', 'columnGutter', 'quietZone', 'starInset', 'start', 'at', 'x', 'y', 'xOffset', 'yOffset', 'position', 'index', 'id', 'frame', 'headerRows', 'footerRows', 'sx', 'sy', 'ref', 'opacity', 'blur', 'distance', 'spread', 'number']);
const BOOLS = new Set(['grid', 'all', 'overwrite', 'removeExisting', 'relative', 'headings', 'swatches', 'styles', 'join', 'allow', 'on', 'locked', 'flip', 'toProcess', 'bleed', 'delete', 'pinTop', 'pinBottom', 'pinLeft', 'pinRight', 'resizeWidth', 'resizeHeight', 'simulatePaper', 'acrossStories', 'continueFromPrevious', 'spot', 'content', 'clearOverrides', 'add', 'raw', 'tagged', 'spreads', 'marks', 'cover', 'blackPointCompensation', 'playOnPageLoad', 'loop', 'controls', 'multiline', 'required', 'nonprinting', 'overprintFill', 'overprintStroke', 'fromSelection', 'ignoreWrap', 'adjustLayout', 'facingPages', 'spreadGuide', 'word', 'forward', 'case', 'wholeWord', 'balanceColumns', 'spanFootnotes', 'invert', 'ignoreWrap', 'pinTop', 'headings', 'join']);
const SKIP = new Set(['ids']);

function matching(s: string, open: number): number {
  let depth = 0;
  for (let i = open; i < s.length; i++) {
    if ('{[('.includes(s[i])) depth++;
    else if ('}])'.includes(s[i])) { depth--; if (depth === 0) return i; }
  }
  return s.length - 1;
}

function splitTop(s: string, sep: string): string[] {
  const out: string[] = [];
  let depth = 0, cur = '';
  for (const ch of s) {
    if ('{[('.includes(ch)) depth++;
    else if ('}])'.includes(ch)) depth--;
    if (ch === sep && depth === 0) { out.push(cur); cur = ''; } else cur += ch;
  }
  if (cur.trim()) out.push(cur);
  return out.map((x) => x.trim()).filter(Boolean);
}

const HEAD = /^([A-Za-z][\w]*)(\?)?\s*(?::\s*([\s\S]*))?$/;

function kindOf(cmd: string, name: string, desc: string): [FieldKind, string[]] {
  if (cmd === 'toc.generate' && name === 'entries') return ['entries', []];
  if (cmd === 'type.openType' && name === 'features') return ['features', []];
  if (name === 'path') return [/^(file\.(export|print)|snippet\.export|book\.export)/.test(cmd) ? 'text' : 'path', []];
  if (name === 'paths') return ['paths', []];
  const d = desc.trim();
  if (/^[\w$-]+(\s*\|\s*[\w$-]+)+$/.test(d)) return ['choice', d.split('|').map((x) => x.trim())];
  if (/^[[{]/.test(d)) return ['json', []];
  if (BOOLS.has(name) || /^(true|false)$/.test(d)) return ['bool', []];
  if (/[[{]/.test(d)) return ['text', []];
  if (NUMERIC.has(name) || /\b(pt|points|deg|degrees|percent|1-based|0-based|modules|0\.\.\d|0 to)\b/.test(d)) return ['number', []];
  return ['text', []];
}

export function parseSig(cmd: string, sig: string): Field[] {
  const s = sig.trim();
  const open = s.indexOf('{');
  if (open < 0) return [];
  const body = s.slice(open + 1, matching(s, open));
  const fields: Field[] = [];
  let group = 0;
  for (const part of splitTop(body, ',')) {
    const segs = splitTop(part, '|');
    const items: string[] = [];
    for (const seg of segs) {
      const isField = /^[A-Za-z]\w*\?(\s*:|$)/.test(seg) || (items.length > 0 && !items[items.length - 1].includes(':') && /^[A-Za-z]\w*(\s*:|$)/.test(seg));
      if (items.length === 0 || isField) items.push(seg); else items[items.length - 1] += `|${seg}`;
    }
    const alt = items.length > 1;
    if (alt) group++;
    for (const it of items) {
      const m = HEAD.exec(it.replace(/\s*\+\s*\w+.*$/, ''));
      if (!m) continue;
      const [, name, q1, desc = ''] = m;
      if (SKIP.has(name)) continue;
      const [kind, choices] = kindOf(cmd, name, desc);
      fields.push({ name, optional: alt || q1 === '?', desc: desc.trim(), kind, choices, group: alt ? group : 0 });
    }
  }
  return fields;
}

type Options = Array<[string, string]>;
type Loader = () => Promise<Options>;

async function query<T = any>(id: string, params: Record<string, unknown> = {}): Promise<T | undefined> {
  return editor.exec<T>(id, params, true).catch(() => undefined);
}

const names = (xs: unknown[] | undefined): Options => (xs ?? []).map((x) => { const s = String(x); return [s, s.replace(/^\[(.*)\]$/, '$1')]; });

const styles = (kind: string): Loader => async () => names((await query<Record<string, string[]>>('style.list'))?.[kind]);
const swatches: Loader = async () => names((await query<Array<{ name: string }>>('swatch.list'))?.map((s) => s.name).filter((n) => n !== '[None]' && n !== '[Registration]'));

const DYNAMIC: Record<string, Loader> = {
  'xref.insert.anchor': async () => ((await query<{ anchors: Array<{ name: string; page?: string }> }>('anchor.list'))?.anchors ?? []).map((a) => [a.name, a.page ? `${a.name} (page ${a.page})` : a.name]),
  'variables.insert.name': async () => names((await query<{ variables: Array<{ name: string }> }>('variables.list'))?.variables.map((v) => v.name)),
  'variables.delete.name': async () => names((await query<{ variables: Array<{ name: string }> }>('variables.list'))?.variables.map((v) => v.name)),
  'data.insertField.name': async () => names((await query<{ fields: string[] }>('data.fields'))?.fields),
  'hyperlink.edit.id': async () => ((await query<{ hyperlinks: Array<{ id: number; name: string }> }>('hyperlink.list'))?.hyperlinks ?? []).map((h) => [String(h.id), h.name]),
  'hyperlink.delete.id': async () => ((await query<{ hyperlinks: Array<{ id: number; name: string }> }>('hyperlink.list'))?.hyperlinks ?? []).map((h) => [String(h.id), h.name]),
  'hyperlink.goToSource.id': async () => ((await query<{ hyperlinks: Array<{ id: number; name: string }> }>('hyperlink.list'))?.hyperlinks ?? []).map((h) => [String(h.id), h.name]),
  'bookmark.delete.index': async () => ((await query<{ bookmarks: Array<{ index: number; name: string }> }>('bookmark.list'))?.bookmarks ?? []).map((b) => [String(b.index), b.name]),
  'font.replace.from': async () => names((await query<{ fonts: Array<{ family: string }> }>('font.list'))?.fonts.map((f) => f.family)),
  'font.replace.to': async () => names((await query<{ families: string[] }>('font.list'))?.families),
  'ink.options.ink': async () => names((await query<{ inks: Array<{ name: string }> }>('ink.list'))?.inks.map((i) => i.name)),
  'ink.options.alias': async () => [['none', 'None'], ...names((await query<{ inks: Array<{ name: string }> }>('ink.list'))?.inks.map((i) => i.name))],
  'list.delete.name': async () => names((await query<{ lists: Array<{ name: string }> }>('list.list'))?.lists.map((l) => l.name)),
  'file.print.printer': async () => names((await query<{ printers: string[] }>('file.printers'))?.printers),
  'color.settings.cmykProfile': async () => names((await query<{ profiles: Array<{ name: string; kind: string }> }>('color.settings'))?.profiles.filter((p) => p.kind === 'cmyk').map((p) => p.name)),
  'color.settings.rgbProfile': async () => names((await query<{ profiles: Array<{ name: string; kind: string }> }>('color.settings'))?.profiles.filter((p) => p.kind === 'rgb').map((p) => p.name)),
  'style.table.edit.name': styles('table'),
  'style.cell.edit.name': styles('cell'),
  'style.paragraph.apply.name': styles('paragraph'),
  'style.character.apply.name': styles('character'),
  'style.object.apply.name': styles('object'),
  'swatch.createTint.base': swatches,
  'object.qrCode.swatch': swatches,
  'object.fill.swatch': swatches,
  'object.stroke.swatch': swatches,
  'footnote.insert.style': styles('paragraph'),
  'object.caption.style': styles('paragraph'),
};

function loaderFor(cmd: string, f: Field): Loader | undefined {
  return DYNAMIC[`${cmd}.${f.name}`];
}

function label(name: string): string {
  const s = name.replace(/([a-z0-9])([A-Z])/g, '$1 $2').replace(/^./, (c) => c.toUpperCase());
  return s.replace(/\bIds?\b/, 'ID').replace(/\bPdf\b/, 'PDF').replace(/\bUrl\b/, 'URL');
}

const RULE_LABELS: Array<[string, string]> = [['guide', 'Stretch with page']];

function choiceLabel(f: Field, c: string): string {
  const own = f.name === 'rule' ? RULE_LABELS.find(([id]) => id === c) : undefined;
  return own ? own[1] : label(c);
}

function coerce(text: string): unknown {
  const t = text.trim();
  if (t === '') return undefined;
  if (/^-?\d+(\.\d+)?$/.test(t)) return Number(t);
  if (t === 'true' || t === 'false') return t === 'true';
  if (/^[[{]/.test(t)) { try { return JSON.parse(t); } catch { return t; } }
  return t;
}

interface Bound { field: Field; read: () => unknown }

export function needsForm(cmd: CoreCommand): boolean {
  const last = cmd.menu[cmd.menu.length - 1] ?? '';
  const fields = parseSig(cmd.id, cmd.params ?? '{}');
  if (!fields.length) return false;
  if (/(\.\.\.|…)$/.test(last) || /(\.\.\.|…)$/.test(cmd.label)) return true;
  return fields.some((f) => !f.optional);
}

export function openCommandForm(host: FormHost, cmd: CoreCommand, why: string, params: Record<string, unknown> = {}): void {
  const fields = parseSig(cmd.id, cmd.params ?? '{}');
  const d = new Dialog(cmd.label.replace(/(\.\.\.|…)$/, ''), 520);
  const form = el('<div class="fo-form fd-cmd-form"></div>');
  if (why) { const p = el('<p class="fo-note fd-cmd-why"></p>'); setText(p, why); form.appendChild(p); }
  const bound: Bound[] = [];
  const groups = new Map<number, HTMLElement>();
  for (const f of fields) {
    let host2: HTMLElement = form;
    if (f.group) {
      let g = groups.get(f.group);
      if (!g) { g = el('<div class="fd-cmd-alt"><span class="fo-note">Fill one of these:</span></div>'); groups.set(f.group, g); form.appendChild(g); }
      host2 = g;
    }
    const row = el('<label class="fo-field"><span></span></label>');
    setText(q(row, 'span'), `${label(f.name)}${f.optional ? '' : ' *'}`);
    if (f.desc && f.kind !== 'choice') row.title = f.desc;
    const initial = params[f.name];
    const loader = loaderFor(cmd.id, f);
    if (f.kind === 'bool') {
      const inp = el<HTMLInputElement>('<input type="checkbox">');
      inp.checked = initial === true;
      let touched = initial !== undefined;
      inp.addEventListener('change', () => { touched = true; });
      row.appendChild(inp);
      bound.push({ field: f, read: () => (touched ? inp.checked : undefined) });
    } else if (f.kind === 'choice' || loader) {
      const sel = el<HTMLSelectElement>('<select></select>');
      if (f.optional) sel.append(new Option('', ''));
      for (const c of f.choices) sel.append(new Option(choiceLabel(f, c), c));
      if (loader) {
        void loader().then((opts) => {
          for (const [v, l] of opts) sel.append(new Option(l, v));
          if (initial !== undefined) sel.value = String(initial);
          else if (!f.optional && sel.options.length) sel.selectedIndex = 0;
        });
      }
      if (initial !== undefined) sel.value = String(initial);
      row.appendChild(sel);
      bound.push({ field: f, read: () => (sel.value === '' ? undefined : f.kind === 'number' ? Number(sel.value) : coerce(sel.value)) });
    } else if (f.kind === 'entries') {
      const box = el('<div class="fd-cmd-entries"></div>');
      row.appendChild(box);
      const picks: Array<{ style: string; level: HTMLSelectElement; on: HTMLInputElement }> = [];
      void query<Record<string, string[]>>('style.list').then((s) => {
        for (const name of s?.paragraph ?? []) {
          if (/No Paragraph Style/.test(name)) continue;
          const r = el('<div class="fd-cmd-entry"><input type="checkbox"><span></span><select></select></div>');
          setText(q(r, 'span'), name.replace(/^\[(.*)\]$/, '$1'));
          const lv = q<HTMLSelectElement>(r, 'select');
          for (let i = 1; i <= 4; i++) lv.append(new Option(`Level ${i}`, String(i)));
          const m = /(\d)\s*$/.exec(name.replace(/\]$/, ''));
          if (m) lv.value = String(Math.min(4, Number(m[1])));
          const on = q<HTMLInputElement>(r, 'input');
          on.checked = /^\[?(heading|head|h)\s*\d/i.test(name) || /title/i.test(name);
          picks.push({ style: name, level: lv, on });
          box.appendChild(r);
        }
      });
      bound.push({ field: f, read: () => { const v = picks.filter((p) => p.on.checked).map((p) => ({ style: p.style, level: Number(p.level.value) })); return v.length ? v : undefined; } });
    } else if (f.kind === 'features') {
      const box = el('<div class="fd-cmd-entries"></div>');
      row.appendChild(box);
      const checks = new Map<string, HTMLInputElement>();
      const start = new Set<string>();
      for (const [tag, name] of FEATURES) {
        const r = el('<label class="fd-cmd-feature"><input type="checkbox"><span></span></label>');
        setText(q(r, 'span'), `${name} (${tag})`);
        checks.set(tag, q<HTMLInputElement>(r, 'input'));
        box.appendChild(r);
      }
      void query<{ chars?: { otfFeatures?: string[] } }>('type.selectionAttrs').then((a) => {
        for (const t of a?.chars?.otfFeatures ?? []) { start.add(t); const c = checks.get(t); if (c) c.checked = true; }
      });
      bound.push({ field: f, read: () => {
        const out: Record<string, boolean> = {};
        for (const [tag, c] of checks) if (c.checked !== start.has(tag)) out[tag] = c.checked;
        return Object.keys(out).length ? out : undefined;
      } });
    } else if (f.kind === 'path' || f.kind === 'paths') {
      const wrap = el('<span class="fd-cmd-path"><input type="text" spellcheck="false"><button class="cv-btn" type="button">Choose…</button></span>');
      const inp = q<HTMLInputElement>(wrap, 'input');
      if (initial !== undefined) inp.value = Array.isArray(initial) ? initial.join('\n') : String(initial);
      q(wrap, 'button').addEventListener('click', () => void (async () => {
        const files = await host.pickFiles('', f.kind === 'paths');
        if (!files.length) return;
        try { const paths = await host.upload(files); inp.value = paths.join('\n'); } catch (err) { toast((err as Error).message, 'error'); }
      })());
      row.appendChild(wrap);
      bound.push({ field: f, read: () => { const v = inp.value.split('\n').map((x) => x.trim()).filter(Boolean); return v.length ? (f.kind === 'paths' ? v : v[0]) : undefined; } });
    } else {
      const multi = f.kind === 'json';
      const inp = el<HTMLInputElement | HTMLTextAreaElement>(multi ? '<textarea rows="3" spellcheck="false"></textarea>' : `<input type="${f.kind === 'number' ? 'number' : 'text'}" step="any" spellcheck="false">`);
      if (f.desc) inp.placeholder = f.desc;
      if (initial !== undefined) inp.value = typeof initial === 'object' ? JSON.stringify(initial) : String(initial);
      row.appendChild(inp);
      bound.push({ field: f, read: () => (f.kind === 'number' ? (inp.value.trim() === '' ? undefined : Number(inp.value)) : coerce(inp.value)) });
    }
    host2.appendChild(row);
  }
  const adv = el('<details class="fd-cmd-adv"><summary>Parameters as JSON</summary><textarea rows="4" spellcheck="false"></textarea><p class="fo-note"></p></details>');
  setText(q(adv, '.fo-note'), `${cmd.id}  ${cmd.params}`);
  const ta = q<HTMLTextAreaElement>(adv, 'textarea');
  ta.value = '';
  form.appendChild(adv);
  d.body.appendChild(form);
  const collect = (): Record<string, unknown> | null => {
    const out: Record<string, unknown> = { ...params };
    for (const b of bound) {
      const v = b.read();
      if (v === undefined) { delete out[b.field.name]; continue; }
      if (b.field.kind === 'number' && Number.isNaN(v)) { toast(`${label(b.field.name)} is not a number`, 'error'); return null; }
      out[b.field.name] = v;
    }
    if (ta.value.trim()) {
      try { Object.assign(out, JSON.parse(ta.value)); } catch { toast('The JSON parameters are not valid JSON', 'error'); return null; }
    }
    for (const f of fields) {
      if (!f.optional && out[f.name] === undefined) { toast(`${label(f.name)} is needed`, 'error'); return null; }
    }
    return out;
  };
  const cancel = el<HTMLButtonElement>('<button class="cv-btn" type="button">Cancel</button>');
  const ok = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">OK</button>');
  cancel.addEventListener('click', () => d.close());
  ok.addEventListener('click', () => {
    const p = collect();
    if (!p) return;
    d.close();
    void host.runCore(cmd.id, p).then((r) => {
      const res = r as { message?: unknown; base64?: unknown } | undefined;
      if (typeof res?.message === 'string' && res.message) toast(res.message, 'info');
      if (typeof res?.base64 === 'string' && res.base64) download(cmd.id, res.base64);
    });
  });
  form.addEventListener('keydown', (e) => { if (e.key === 'Enter' && !(e.target instanceof HTMLTextAreaElement)) { e.preventDefault(); ok.click(); } });
  d.foot.append(cancel, ok);
  d.onClose = () => d.scrim.remove();
  d.open();
}
