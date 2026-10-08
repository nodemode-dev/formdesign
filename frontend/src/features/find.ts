import '../styles/find.css';
import { editor } from '../editor/editor';
import { api, engine } from '../engine/client';
import { confirmAction, promptName } from '../ui/dialogs';
import { enhanceSelect } from '../ui/dropdown';
import { el, patchList, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import { icon } from '../ui/icons';
import type { Workspace } from '../ui/workspace';

type Mode = 'text' | 'grep' | 'glyph' | 'object';
type Rect4 = [number, number, number, number];
type Groups = Array<[string, Array<[string, string]>]>;
interface Cell { table: number; row: number; col: number }
interface Match { story?: number; note?: number; cell?: Cell; start?: number; end?: number; text?: string; before?: string; after?: string; page?: number | null; locked?: boolean; overset?: boolean; rect?: Rect4 | null; id?: number; kind?: string; name?: string; style?: string }
interface Query { name: string; mode: Mode; builtin?: boolean; [k: string]: unknown }
type Fmt = Record<string, string | number>;

const MODES: Array<[Mode, string]> = [['text', 'Text'], ['grep', 'GREP'], ['glyph', 'Glyph'], ['object', 'Object']];
const SCOPES: Array<[string, string]> = [['document', 'Document'], ['story', 'Story'], ['toEnd', 'To End of Story'], ['selection', 'Selection']];
const KINDS: Array<[string, string]> = [['', 'Any kind'], ['text', 'Text frame'], ['graphic', 'Graphic frame'], ['shape', 'Shape'], ['line', 'Line'], ['group', 'Group']];
const CHARS_TEXT: Array<[string, string]> = [['^t', 'Tab'], ['^p', 'End of Paragraph'], ['^n', 'Forced Line Break'], ['^_', 'Em Dash'], ['^=', 'En Dash'], ['^-', 'Discretionary Hyphen'], ['^~', 'Nonbreaking Hyphen'], ['^s', 'Nonbreaking Space'], ['^m', 'Em Space'], ['^>', 'En Space'], ['^<', 'Thin Space'], ['^e', 'Ellipsis'], ['^8', 'Bullet'], ['^2', 'Copyright Symbol'], ['^r', 'Registered Trademark'], ['^d', 'Trademark Symbol'], ['^{', 'Double Left Quote'], ['^}', 'Double Right Quote'], ['^[', 'Single Left Quote'], ['^]', 'Single Right Quote']];
const CHARS_GREP: Array<[string, string]> = [['\\t', 'Tab'], ['\\r', 'End of Paragraph'], ['\\n', 'Forced Line Break'], ['~_', 'Em Dash'], ['~=', 'En Dash'], ['~-', 'Discretionary Hyphen'], ['~~', 'Nonbreaking Hyphen'], ['~S', 'Nonbreaking Space'], ['~m', 'Em Space'], ['~<', 'Thin Space'], ['~e', 'Ellipsis'], ['~8', 'Bullet'], ['~{', 'Double Left Quote'], ['~}', 'Double Right Quote'], ['~[', 'Single Left Quote'], ['~]', 'Single Right Quote']];
const META: Record<string, Groups> = {
  textFind: [['Characters', CHARS_TEXT], ['Wildcards', [['^9', 'Any Digit'], ['^$', 'Any Letter'], ['^?', 'Any Character'], ['^w', 'Any White Space'], ['^"', 'Any Double Quote'], ["^'", 'Any Single Quote']]], ['Markers', [['^#', 'Any Page Number'], ['^x', 'Section Marker'], ['^F', 'Footnote Reference'], ['^a', 'Anchored Object']]]],
  textChange: [['Characters', CHARS_TEXT], ['Markers', [['^N', 'Current Page Number'], ['^x', 'Section Marker']]]],
  grepFind: [
    ['Characters', CHARS_GREP],
    ['Wildcards', [['\\d', 'Any Digit'], ['\\l', 'Any Lowercase Letter'], ['\\u', 'Any Uppercase Letter'], ['\\w', 'Any Word Character'], ['.', 'Any Character'], ['\\s', 'Any White Space'], ['\\h', 'Any Horizontal Space'], ['~"', 'Any Double Quote'], ["~'", 'Any Single Quote']]],
    ['Locations', [['\\<', 'Beginning of Word'], ['\\>', 'End of Word'], ['\\b', 'Word Boundary'], ['^', 'Beginning of Paragraph'], ['$', 'End of Paragraph']]],
    ['Repeat', [['?', 'Zero or One Time'], ['*', 'Zero or More Times'], ['+', 'One or More Times'], ['*?', 'Zero or More (Shortest Match)'], ['+?', 'One or More (Shortest Match)']]],
    ['Match', [['()', 'Marking Subexpression'], ['(?:)', 'Non-marking Subexpression'], ['[]', 'Character Set'], ['|', 'Or'], ['(?<=)', 'Positive Lookbehind'], ['(?<!)', 'Negative Lookbehind'], ['(?=)', 'Positive Lookahead'], ['(?!)', 'Negative Lookahead']]],
    ['Modifiers', [['(?i)', 'Case-insensitive On'], ['(?-i)', 'Case-insensitive Off']]],
  ],
  grepChange: [['Found', [['$0', 'Found Text'], ['$1', 'Found 1'], ['$2', 'Found 2'], ['$3', 'Found 3'], ['$4', 'Found 4']]], ['Characters', CHARS_GREP]],
};
const POS_KEY = 'formdes.find.pos';
const LINE_BREAK = new RegExp(String.fromCharCode(0x2028), 'g');
const MARKERS = new RegExp(`[${String.fromCharCode(0xe000)}-${String.fromCharCode(0xf8ff)}]`, 'g');

function visible(t: string): string {
  return t.replace(/\n/g, '¶').replace(/\t/g, '»').replace(LINE_BREAK, '¬').replace(MARKERS, '◆');
}

function fillSelect(sel: HTMLSelectElement, items: Array<[string, string]>): void {
  const key = JSON.stringify(items);
  if (sel.dataset.key === key) return;
  const cur = sel.value;
  sel.dataset.key = key;
  sel.replaceChildren(...items.map(([v, l]) => new Option(l, v)));
  sel.value = items.some(([v]) => v === cur) ? cur : (items[0]?.[0] ?? '');
}

function metaSelect(groups: Groups, input: HTMLInputElement): HTMLSelectElement {
  const sel = el<HTMLSelectElement>('<select class="cv-select ff-meta" title="Special characters" aria-label="Insert a special character"></select>');
  sel.append(new Option('@', ''));
  for (const [label, items] of groups) {
    const g = document.createElement('optgroup');
    g.label = label;
    for (const [code, name] of items) g.append(new Option(`${name} (${code})`, code));
    sel.append(g);
  }
  sel.addEventListener('change', () => {
    const code = sel.value;
    sel.value = '';
    if (!code) return;
    const a = input.selectionStart ?? input.value.length, b = input.selectionEnd ?? a;
    input.setRangeText(code, a, b, 'end');
    const inner = /^\(\?[^)]*\)$|^\(\)$|^\[\]$/.test(code) && !/^\(\?[i-]/.test(code) ? 1 : 0;
    const at = a + code.length - inner;
    input.focus();
    input.setSelectionRange(at, at);
    input.dispatchEvent(new Event('input'));
  });
  return sel;
}

interface Lists { paras: string[]; chars: string[]; swatches: string[]; layers: string[]; objStyles: string[]; fonts: string[] }

class FmtEditor {
  readonly root = el(`<div class="ff-fmt">
    <div class="ff-fmt-head"><span class="ff-fmt-lab"></span><span class="ff-fmt-sum"></span><button class="cv-btn ff-mini ff-fmt-edit" type="button">Edit</button><button class="cv-icon-btn ff-fmt-clear" type="button" title="Clear the format" aria-label="Clear the format">${icon('close', 14)}</button></div>
    <div class="ff-fmt-body" hidden>
      <label class="ff-f"><span>Paragraph style</span><select class="cv-select ff-ps"></select></label>
      <label class="ff-f"><span>Character style</span><select class="cv-select ff-cs"></select></label>
      <label class="ff-f"><span>Font</span><select class="cv-select ff-fam"></select></label>
      <label class="ff-f"><span>Font style</span><select class="cv-select ff-sty"></select></label>
      <label class="ff-f"><span>Size (pt)</span><input class="ff-in ff-size" type="number" min="1" step="0.5" placeholder="Any"></label>
      <label class="ff-f"><span>Colour</span><select class="cv-select ff-col"></select></label>
    </div></div>`);
  private readonly ps = q<HTMLSelectElement>(this.root, '.ff-ps');
  private readonly cs = q<HTMLSelectElement>(this.root, '.ff-cs');
  private readonly fam = q<HTMLSelectElement>(this.root, '.ff-fam');
  private readonly sty = q<HTMLSelectElement>(this.root, '.ff-sty');
  private readonly size = q<HTMLInputElement>(this.root, '.ff-size');
  private readonly col = q<HTMLSelectElement>(this.root, '.ff-col');
  onChange: (() => void) | null = null;

  constructor(label: string) {
    setText(q(this.root, '.ff-fmt-lab'), label);
    for (const s of [this.ps, this.cs, this.sty, this.col]) enhanceSelect(s);
    enhanceSelect(this.fam, { search: 'Search fonts' });
    fillSelect(this.sty, [['', 'Any']]);
    q(this.root, '.ff-fmt-edit').addEventListener('click', () => {
      const body = q(this.root, '.ff-fmt-body');
      setHidden(body, !body.hidden);
      setText(q(this.root, '.ff-fmt-edit'), body.hidden ? 'Edit' : 'Done');
    });
    q(this.root, '.ff-fmt-clear').addEventListener('click', () => this.set({}));
    this.fam.addEventListener('change', () => void this.styles(''));
    for (const n of [this.ps, this.cs, this.sty, this.col, this.size]) n.addEventListener('change', () => this.changed());
    this.fam.addEventListener('change', () => this.changed());
    this.summary();
  }

  lists(l: Lists): void {
    const any = (xs: string[]): Array<[string, string]> => [['', 'Any'], ...xs.map((x): [string, string] => [x, x])];
    fillSelect(this.ps, any(l.paras));
    fillSelect(this.cs, any(l.chars));
    fillSelect(this.fam, any(l.fonts));
    fillSelect(this.col, any(l.swatches));
  }

  private async styles(want: string): Promise<void> {
    const fam = this.fam.value;
    let st: string[] = [];
    if (fam) { try { st = ((await engine.call<string[]>('fonts.styles', { family: fam })).result ?? []) as string[]; } catch { st = []; } }
    fillSelect(this.sty, [['', 'Any'], ...st.map((x): [string, string] => [x, x])]);
    this.sty.value = want && st.includes(want) ? want : '';
    this.summary();
  }

  private changed(): void { this.summary(); this.onChange?.(); }

  get(): Fmt {
    const f: Fmt = {};
    if (this.ps.value) f.paragraphStyle = this.ps.value;
    if (this.cs.value) f.characterStyle = this.cs.value;
    if (this.fam.value) f.fontFamily = this.fam.value;
    if (this.sty.value) f.fontStyle = this.sty.value;
    const n = Number(this.size.value);
    if (this.size.value !== '' && Number.isFinite(n) && n > 0) f.size = n;
    if (this.col.value) f.fill = this.col.value;
    return f;
  }

  set(f: Record<string, unknown>): void {
    const str = (k: string) => (typeof f[k] === 'string' ? (f[k] as string) : '');
    const ensure = (s: HTMLSelectElement, v: string) => { if (v && ![...s.options].some((o) => o.value === v)) s.append(new Option(v, v)); s.value = v; };
    ensure(this.ps, str('paragraphStyle'));
    ensure(this.cs, str('characterStyle'));
    ensure(this.fam, str('fontFamily'));
    ensure(this.col, str('fill'));
    this.size.value = typeof f.size === 'number' ? String(f.size) : '';
    void this.styles(str('fontStyle'));
    this.changed();
  }

  private summary(): void {
    const f = this.get();
    const parts = [f.paragraphStyle && `Para: ${f.paragraphStyle}`, f.characterStyle && `Char: ${f.characterStyle}`, f.fontFamily && `${f.fontFamily}${f.fontStyle ? ` ${f.fontStyle}` : ''}`, !f.fontFamily && f.fontStyle && `${f.fontStyle}`, f.size && `${f.size} pt`, f.fill && `${f.fill}`].filter(Boolean);
    const sum = q(this.root, '.ff-fmt-sum');
    setText(sum, parts.length ? parts.join(' · ') : 'None');
    setClass(sum, 'is-none', !parts.length);
    setDisabled(q<HTMLButtonElement>(this.root, '.ff-fmt-clear'), !parts.length);
  }
}

class GlyphPick {
  readonly root = el(`<div class="ff-glyph">
    <label class="ff-f"><span>Font</span><select class="cv-select ff-gfam"></select></label>
    <label class="ff-f"><span>Style</span><select class="cv-select ff-gsty"></select></label>
    <label class="ff-f"><span>Glyph</span><input class="ff-in ff-gch" type="text" spellcheck="false" placeholder="A character or U+2026"></label>
    <span class="ff-gprev" aria-hidden="true"></span></div>`);
  private readonly fam = q<HTMLSelectElement>(this.root, '.ff-gfam');
  private readonly sty = q<HTMLSelectElement>(this.root, '.ff-gsty');
  private readonly ch = q<HTMLInputElement>(this.root, '.ff-gch');
  private readonly prev = q(this.root, '.ff-gprev');
  private static faces = new Map<string, string>();
  onChange: (() => void) | null = null;

  constructor(anyLabel: string) {
    enhanceSelect(this.fam, { search: 'Search fonts' });
    enhanceSelect(this.sty);
    fillSelect(this.fam, [['', anyLabel]]);
    fillSelect(this.sty, [['', 'Any']]);
    this.fam.addEventListener('change', () => { void this.styles(''); this.onChange?.(); });
    this.sty.addEventListener('change', () => { void this.face(); this.onChange?.(); });
    this.ch.addEventListener('input', () => { this.preview(); this.onChange?.(); });
  }

  fonts(list: string[], anyLabel: string): void { fillSelect(this.fam, [['', anyLabel], ...list.map((x): [string, string] => [x, x])]); }

  char(): string {
    const t = this.ch.value.trim();
    const m = /^u\+([0-9a-f]{2,6})$/i.exec(t);
    if (m) { const cp = parseInt(m[1], 16); return cp <= 0x10ffff ? String.fromCodePoint(cp) : ''; }
    return t ? String.fromCodePoint(t.codePointAt(0) as number) : '';
  }

  get(): Record<string, string> {
    const g: Record<string, string> = { char: this.ch.value.trim() };
    if (this.fam.value) g.fontFamily = this.fam.value;
    if (this.sty.value) g.fontStyle = this.sty.value;
    return g;
  }

  set(g: Record<string, unknown> | undefined): void {
    const s = (k: string) => (typeof g?.[k] === 'string' ? (g[k] as string) : '');
    if (s('fontFamily') && ![...this.fam.options].some((o) => o.value === s('fontFamily'))) this.fam.append(new Option(s('fontFamily'), s('fontFamily')));
    this.fam.value = s('fontFamily');
    this.ch.value = s('char');
    void this.styles(s('fontStyle'));
  }

  private async styles(want: string): Promise<void> {
    let st: string[] = [];
    if (this.fam.value) { try { st = ((await engine.call<string[]>('fonts.styles', { family: this.fam.value })).result ?? []) as string[]; } catch { st = []; } }
    fillSelect(this.sty, [['', 'Any'], ...st.map((x): [string, string] => [x, x])]);
    this.sty.value = want && st.includes(want) ? want : '';
    await this.face();
  }

  private async face(): Promise<void> {
    const fam = this.fam.value;
    const sty = this.sty.value || 'Regular';
    if (!fam) { this.prev.style.removeProperty('font-family'); this.preview(); return; }
    const key = `${fam}/${sty}`;
    let name = GlyphPick.faces.get(key);
    if (!name) {
      name = `fd-ff-${GlyphPick.faces.size + 1}`;
      try {
        const f = new FontFace(name, `url(/api/core/font?family=${encodeURIComponent(fam)}&style=${encodeURIComponent(sty)})`);
        document.fonts.add(await f.load());
        GlyphPick.faces.set(key, name);
      } catch { name = ''; }
    }
    if (name) this.prev.style.fontFamily = `"${name}"`; else this.prev.style.removeProperty('font-family');
    this.preview();
  }

  private preview(): void {
    const c = this.char();
    setText(this.prev, c);
    this.prev.title = c ? `U+${(c.codePointAt(0) as number).toString(16).toUpperCase().padStart(4, '0')}` : '';
  }
}

export class FindWindow {
  readonly root = el(`<section class="ff-win" role="dialog" aria-label="Find/Change" hidden>
    <header class="ff-head"><span class="ff-title">Find/Change</span><button class="cv-icon-btn ff-close" type="button" title="Close" aria-label="Close">${icon('close', 16)}</button></header>
    <div class="ff-body">
      <div class="ff-qrow"><span class="ff-lab">Query</span><select class="cv-select ff-query" aria-label="Saved query"></select>
        <button class="cv-icon-btn ff-qsave" type="button" title="Save these settings as a query" aria-label="Save query">${icon('save', 15)}</button>
        <button class="cv-icon-btn ff-qdel" type="button" title="Delete the saved query" aria-label="Delete query">${icon('delete', 15)}</button></div>
      <div class="cv-seg ff-modes" role="tablist" aria-label="Search for"></div>
      <div class="ff-page ff-page-text">
        <div class="ff-field"><span class="ff-lab">Find what</span><input class="ff-in ff-find" type="text" spellcheck="false" aria-label="Find what"></div>
        <div class="ff-field"><span class="ff-lab">Change to</span><input class="ff-in ff-change" type="text" spellcheck="false" aria-label="Change to"></div>
      </div>
      <div class="ff-page ff-page-glyph" hidden><div class="ff-sec">Find glyph</div><div class="ff-gfind"></div><div class="ff-sec">Change glyph</div><div class="ff-gchange"></div></div>
      <div class="ff-page ff-page-object" hidden>
        <div class="ff-sec">Find object format</div>
        <div class="ff-grid">
          <label class="ff-f"><span>Kind</span><select class="cv-select ff-okind"></select></label>
          <label class="ff-f"><span>Object style</span><select class="cv-select ff-ostyle"></select></label>
          <label class="ff-f"><span>Fill</span><select class="cv-select ff-ofill"></select></label>
          <label class="ff-f"><span>Stroke</span><select class="cv-select ff-ostroke"></select></label>
          <label class="ff-f"><span>Stroke weight (pt)</span><input class="ff-in ff-oweight" type="number" min="0" step="0.25" placeholder="Any"></label>
          <label class="ff-f"><span>Opacity (%)</span><input class="ff-in ff-oopacity" type="number" min="0" max="100" step="1" placeholder="Any"></label>
          <label class="ff-f"><span>Layer</span><select class="cv-select ff-olayer"></select></label>
        </div>
        <div class="ff-sec">Change object format</div>
        <div class="ff-grid">
          <label class="ff-f"><span>Object style</span><select class="cv-select ff-cstyle"></select></label>
          <label class="ff-f"><span>Fill</span><select class="cv-select ff-cfill"></select></label>
          <label class="ff-f"><span>Stroke</span><select class="cv-select ff-cstroke"></select></label>
          <label class="ff-f"><span>Stroke weight (pt)</span><input class="ff-in ff-cweight" type="number" min="0" step="0.25" placeholder="Unchanged"></label>
          <label class="ff-f"><span>Opacity (%)</span><input class="ff-in ff-copacity" type="number" min="0" max="100" step="1" placeholder="Unchanged"></label>
        </div>
      </div>
      <div class="ff-opts">
        <select class="cv-select ff-scope" aria-label="Search"></select>
        <button class="ff-tog ff-case" type="button" aria-pressed="false" title="Case sensitive">Aa</button>
        <button class="ff-tog ff-word" type="button" aria-pressed="false" title="Whole word">W</button>
        <button class="ff-tog ff-locked" type="button" aria-pressed="false" title="Include locked layers and locked objects (found, never changed)">${icon('lock', 14)}</button>
        <button class="ff-tog ff-hidden" type="button" aria-pressed="false" title="Include hidden layers and objects">${icon('eyeOff', 14)}</button>
        <button class="ff-tog ff-parents" type="button" aria-pressed="false" title="Include parent pages">P</button>
      </div>
      <div class="ff-fmts"></div>
      <div class="ff-acts">
        <button class="cv-btn ff-all" type="button" title="List every match">Find All</button>
        <button class="cv-btn cv-btn-primary ff-next" type="button" title="Find Next (Alt+Ctrl+F)">Find Next</button>
        <button class="cv-btn ff-chg" type="button">Change</button>
        <button class="cv-btn ff-chgall" type="button">Change All</button>
        <button class="cv-btn ff-chgfind" type="button">Change/Find</button>
      </div>
      <div class="ff-status" role="status"></div>
      <div class="ff-results" role="listbox" aria-label="Matches"></div>
    </div></section>`);
  private mode: Mode = 'text';
  private readonly find = q<HTMLInputElement>(this.root, '.ff-find');
  private readonly change = q<HTMLInputElement>(this.root, '.ff-change');
  private readonly query = q<HTMLSelectElement>(this.root, '.ff-query');
  private readonly scope = q<HTMLSelectElement>(this.root, '.ff-scope');
  private readonly findFmt = new FmtEditor('Find format');
  private readonly changeFmt = new FmtEditor('Change format');
  private readonly gFind = new GlyphPick('Any font');
  private readonly gChange = new GlyphPick('Keep the font');
  private readonly metas: Record<string, HTMLSelectElement> = {};
  private readonly results = q(this.root, '.ff-results');
  private matches: Match[] = [];
  private current = '';
  private queries: Query[] = [];
  private within: Record<string, unknown> | null = null;
  private withinKey = '';
  private busy = false;
  private fontsLoaded = false;
  private listTimer = 0;
  private pos: { x: number; y: number } | null = null;
  private docUid: number | null = null;
  private fontCache: string[] = [];

  constructor(private readonly ws: Workspace) {
    this.docUid = editor.doc?.uid ?? null;
    document.getElementById('overlay-root')?.appendChild(this.root);
    const modes = q(this.root, '.ff-modes');
    for (const [m, label] of MODES) {
      const b = el<HTMLButtonElement>(`<button type="button" role="tab" data-mode="${m}"></button>`);
      setText(b, label);
      b.addEventListener('click', () => this.setMode(m));
      modes.append(b);
    }
    const metaFind = { text: metaSelect(META.textFind, this.find), grep: metaSelect(META.grepFind, this.find) };
    const metaChange = { text: metaSelect(META.textChange, this.change), grep: metaSelect(META.grepChange, this.change) };
    this.metas.textFind = metaFind.text; this.metas.grepFind = metaFind.grep; this.metas.textChange = metaChange.text; this.metas.grepChange = metaChange.grep;
    const [ff, cf] = this.root.querySelectorAll<HTMLElement>('.ff-page-text .ff-field');
    for (const s of [metaFind.text, metaFind.grep]) { ff.append(s); enhanceSelect(s); }
    for (const s of [metaChange.text, metaChange.grep]) { cf.append(s); enhanceSelect(s); }
    q(this.root, '.ff-gfind').append(this.gFind.root);
    q(this.root, '.ff-gchange').append(this.gChange.root);
    q(this.root, '.ff-fmts').append(this.findFmt.root, this.changeFmt.root);
    fillSelect(this.scope, SCOPES);
    fillSelect(q<HTMLSelectElement>(this.root, '.ff-okind'), KINDS);
    for (const s of this.root.querySelectorAll<HTMLSelectElement>('.ff-query, .ff-scope, .ff-page-object select')) enhanceSelect(s);
    for (const t of this.root.querySelectorAll<HTMLButtonElement>('.ff-tog')) t.addEventListener('click', () => { this.toggle(t, t.getAttribute('aria-pressed') !== 'true'); this.edited(); });
    q(this.root, '.ff-close').addEventListener('click', () => this.close());
    q(this.root, '.ff-all').addEventListener('click', () => void this.run('all'));
    q(this.root, '.ff-next').addEventListener('click', () => void this.run('next'));
    q(this.root, '.ff-chg').addEventListener('click', () => void this.run('change'));
    q(this.root, '.ff-chgall').addEventListener('click', () => void this.run('changeAll'));
    q(this.root, '.ff-chgfind').addEventListener('click', () => void this.run('changeFind'));
    q(this.root, '.ff-qsave').addEventListener('click', () => void this.saveQuery());
    q(this.root, '.ff-qdel').addEventListener('click', () => void this.deleteQuery());
    this.query.addEventListener('change', () => this.loadQuery(this.query.value));
    for (const n of this.root.querySelectorAll<HTMLInputElement | HTMLSelectElement>('.ff-find, .ff-change, .ff-scope, .ff-page-object select, .ff-page-object input')) n.addEventListener(n.tagName === 'INPUT' && n.type === 'text' ? 'input' : 'change', () => this.edited());
    this.findFmt.onChange = () => this.edited();
    this.changeFmt.onChange = () => this.edited();
    this.gFind.onChange = () => this.edited();
    this.gChange.onChange = () => this.edited();
    this.root.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); this.close(); return; }
      if (e.key === 'Enter' && (e.target as HTMLElement).tagName === 'INPUT') { e.preventDefault(); e.stopPropagation(); void this.run('next'); }
    });
    this.drag();
    editor.on('docs', () => {
      const uid = editor.doc?.uid ?? null;
      if (uid !== this.docUid) { this.docUid = uid; this.within = null; this.clearResults(); }
      this.schedule();
    });
    editor.on('change', (e: { docChanged?: boolean } | undefined) => { if (e?.docChanged) this.schedule(); });
    this.setMode('text');
  }

  get isOpen(): boolean { return !this.root.hidden; }

  open(focusChange = false): void {
    if (this.root.hidden) {
      this.place();
      setHidden(this.root, false);
      void this.loadQueries();
      this.schedule(0);
    }
    const target = this.mode === 'object' ? q<HTMLElement>(this.root, '.ff-page-object select, .ff-page-object .cv-dd-btn') : this.mode === 'glyph' ? q<HTMLElement>(this.root, '.ff-gfind .ff-gch') : focusChange ? this.change : this.find;
    target.focus();
    if (target instanceof HTMLInputElement) target.select();
  }

  close(): void {
    if (this.root.hidden) return;
    setHidden(this.root, true);
    if (this.root.contains(document.activeElement)) (document.activeElement as HTMLElement).blur();
  }

  async findNext(): Promise<void> {
    if (!this.ready()) { this.open(); return; }
    await this.run('next');
  }

  private ready(): boolean {
    if (this.mode === 'object') return Object.keys(this.objectFind()).length > 0;
    if (this.mode === 'glyph') return !!this.gFind.char();
    return !!this.find.value || Object.keys(this.findFmt.get()).length > 0;
  }

  private place(): void {
    if (!this.pos) {
      try { const s = JSON.parse(localStorage.getItem(POS_KEY) ?? 'null'); if (s && typeof s.x === 'number' && typeof s.y === 'number') this.pos = s; } catch { this.pos = null; }
    }
    const w = 440;
    const p = this.pos ?? { x: Math.max(16, window.innerWidth - w - 640), y: 96 };
    this.moveTo(p.x, p.y);
  }

  private moveTo(x: number, y: number): void {
    const cx = Math.round(Math.max(8 - 440 + 80, Math.min(window.innerWidth - 80, x)));
    const cy = Math.round(Math.max(8, Math.min(window.innerHeight - 48, y)));
    this.pos = { x: cx, y: cy };
    this.root.style.left = `${cx}px`;
    this.root.style.top = `${cy}px`;
  }

  private drag(): void {
    const head = q(this.root, '.ff-head');
    let start: { px: number; py: number; x: number; y: number } | null = null;
    head.addEventListener('pointerdown', (e) => {
      if ((e.target as HTMLElement).closest('button') || e.button !== 0) return;
      head.setPointerCapture(e.pointerId);
      start = { px: e.clientX, py: e.clientY, x: this.pos?.x ?? 0, y: this.pos?.y ?? 0 };
      setClass(this.root, 'is-moving', true);
    });
    head.addEventListener('pointermove', (e) => { if (start) this.moveTo(start.x + e.clientX - start.px, start.y + e.clientY - start.py); });
    const end = () => {
      if (!start) return;
      start = null;
      setClass(this.root, 'is-moving', false);
      try { localStorage.setItem(POS_KEY, JSON.stringify(this.pos)); } catch { }
    };
    head.addEventListener('pointerup', end);
    head.addEventListener('pointercancel', end);
  }

  private toggle(t: HTMLButtonElement, on: boolean): void {
    t.setAttribute('aria-pressed', String(on));
    setClass(t, 'is-on', on);
  }

  private on(cls: string): boolean { return q(this.root, cls).getAttribute('aria-pressed') === 'true'; }

  private setMode(m: Mode): void {
    if (this.mode !== m) this.clearResults();
    this.mode = m;
    for (const b of this.root.querySelectorAll<HTMLButtonElement>('.ff-modes button')) {
      const on = b.dataset.mode === m;
      setClass(b, 'is-on', on);
      b.setAttribute('aria-selected', String(on));
    }
    const textual = m === 'text' || m === 'grep';
    setHidden(q(this.root, '.ff-page-text'), !textual);
    setHidden(q(this.root, '.ff-page-glyph'), m !== 'glyph');
    setHidden(q(this.root, '.ff-page-object'), m !== 'object');
    setHidden(q(this.root, '.ff-fmts'), !textual);
    setHidden(q(this.root, '.ff-word'), m !== 'text');
    setHidden(q(this.root, '.ff-case'), !textual);
    setHidden(q(this.root, '.ff-scope + .cv-dd-btn'), m === 'object');
    for (const [k, s] of Object.entries(this.metas)) setHidden(s.nextElementSibling as HTMLElement, !k.startsWith(m === 'grep' ? 'grep' : 'text'));
    setClass(this.find, 'ff-mono', m === 'grep');
    setClass(this.change, 'ff-mono', m === 'grep');
    setDisabled(q<HTMLButtonElement>(this.root, '.ff-chgfind'), false);
    this.edited();
  }

  private edited(): void {
    if (this.query.value) { this.query.value = ''; }
    this.within = null;
    this.withinKey = '';
    this.buttons();
  }

  private buttons(): void {
    const has = editor.hasDoc && this.ready();
    for (const c of ['.ff-all', '.ff-next', '.ff-chg', '.ff-chgall', '.ff-chgfind']) setDisabled(q<HTMLButtonElement>(this.root, c), !has || this.busy);
    const canChange = this.mode === 'glyph' ? !!this.gChange.char() : this.mode === 'object' ? Object.keys(this.objectChange()).length > 0 : true;
    for (const c of ['.ff-chg', '.ff-chgall', '.ff-chgfind']) if (!canChange) setDisabled(q<HTMLButtonElement>(this.root, c), true);
    const qd = this.queries.find((x) => x.name === this.query.value);
    setDisabled(q<HTMLButtonElement>(this.root, '.ff-qdel'), !qd || !!qd.builtin);
  }

  private schedule(ms = 250): void {
    if (this.root.hidden) return;
    window.clearTimeout(this.listTimer);
    this.listTimer = window.setTimeout(() => void this.loadLists(), ms);
  }

  private async loadLists(): Promise<void> {
    if (!editor.hasDoc) { this.buttons(); return; }
    const ins = await editor.inspect().catch(() => null) as { paragraphStyles?: string[]; characterStyles?: string[]; swatches?: string[]; layers?: Array<{ name: string }> } | null;
    const os = await engine.call<{ styles: Array<{ name: string }> }>('objectStyles', {}).catch(() => undefined);
    let fonts: string[] = [];
    if (!this.fontsLoaded) { try { fonts = ((await engine.call<string[]>('fonts.families')).result ?? []) as string[]; this.fontsLoaded = fonts.length > 0; } catch { fonts = []; } }
    const lists: Lists = {
      paras: (ins?.paragraphStyles ?? []).filter((x) => x !== '[No Paragraph Style]'),
      chars: ins?.characterStyles ?? [],
      swatches: ins?.swatches ?? [],
      layers: (ins?.layers ?? []).map((l) => l.name),
      objStyles: (os?.result?.styles ?? []).map((s) => s.name),
      fonts,
    };
    if (!fonts.length) lists.fonts = this.fontCache;
    else this.fontCache = fonts;
    this.findFmt.lists(lists);
    this.changeFmt.lists(lists);
    if (fonts.length) { this.gFind.fonts(fonts, 'Any font'); this.gChange.fonts(fonts, 'Keep the font'); }
    const any = (xs: string[], label = 'Any'): Array<[string, string]> => [['', label], ...xs.map((x): [string, string] => [x, x])];
    fillSelect(q<HTMLSelectElement>(this.root, '.ff-ostyle'), any(lists.objStyles));
    fillSelect(q<HTMLSelectElement>(this.root, '.ff-ofill'), any(lists.swatches));
    fillSelect(q<HTMLSelectElement>(this.root, '.ff-ostroke'), any(lists.swatches));
    fillSelect(q<HTMLSelectElement>(this.root, '.ff-olayer'), any(lists.layers));
    fillSelect(q<HTMLSelectElement>(this.root, '.ff-cstyle'), any(lists.objStyles, 'Unchanged'));
    fillSelect(q<HTMLSelectElement>(this.root, '.ff-cfill'), any(lists.swatches, 'Unchanged'));
    fillSelect(q<HTMLSelectElement>(this.root, '.ff-cstroke'), any(lists.swatches, 'Unchanged'));
    this.buttons();
  }

  private objectFind(): Record<string, unknown> {
    const v = (c: string) => q<HTMLInputElement | HTMLSelectElement>(this.root, c).value;
    const o: Record<string, unknown> = {};
    if (v('.ff-okind')) o.kind = v('.ff-okind');
    if (v('.ff-ostyle')) o.objectStyle = v('.ff-ostyle');
    if (v('.ff-ofill')) o.fill = v('.ff-ofill');
    if (v('.ff-ostroke')) o.stroke = v('.ff-ostroke');
    if (v('.ff-olayer')) o.layer = v('.ff-olayer');
    if (v('.ff-oweight') !== '') o.strokeWeight = Number(v('.ff-oweight'));
    if (v('.ff-oopacity') !== '') o.opacity = Number(v('.ff-oopacity')) / 100;
    return o;
  }

  private objectChange(): Record<string, unknown> {
    const v = (c: string) => q<HTMLInputElement | HTMLSelectElement>(this.root, c).value;
    const o: Record<string, unknown> = {};
    if (v('.ff-cstyle')) o.objectStyle = v('.ff-cstyle');
    if (v('.ff-cfill')) o.fill = v('.ff-cfill');
    if (v('.ff-cstroke')) o.stroke = v('.ff-cstroke');
    if (v('.ff-cweight') !== '') o.strokeWeight = Number(v('.ff-cweight'));
    if (v('.ff-copacity') !== '') o.opacity = Number(v('.ff-copacity')) / 100;
    return o;
  }

  private settings(): Record<string, unknown> {
    const p: Record<string, unknown> = { mode: this.mode };
    p.include = { locked: this.on('.ff-locked'), hidden: this.on('.ff-hidden'), parents: this.on('.ff-parents') };
    if (this.mode === 'object') {
      p.object = this.objectFind();
      p.objectChange = this.objectChange();
      return p;
    }
    p.scope = this.scope.value;
    if (this.mode === 'glyph') {
      p.glyph = this.gFind.get();
      if (this.gChange.char()) p.toGlyph = this.gChange.get();
      return p;
    }
    p.find = this.find.value;
    p.change = this.change.value;
    p.caseSensitive = this.on('.ff-case');
    if (this.mode === 'text') p.wholeWord = this.on('.ff-word');
    const ff = this.findFmt.get(), cf = this.changeFmt.get();
    if (Object.keys(ff).length) p.findFormat = ff;
    if (Object.keys(cf).length) p.changeFormat = cf;
    return p;
  }

  private withinFor(p: Record<string, unknown>): Record<string, unknown> | null | undefined {
    const scope = p.scope;
    if (scope !== 'toEnd' && scope !== 'selection') return undefined;
    const key = JSON.stringify(p);
    if (this.within && this.withinKey === key) return this.within;
    const t = editor.selection?.text as { story: number; anchor: number; focus: number; cell?: Cell } | undefined;
    if (!t) { this.status(scope === 'toEnd' ? 'Click in a story to search to its end.' : 'Select some text to search within it.', true); return null; }
    const a = Math.min(t.anchor, t.focus), b = Math.max(t.anchor, t.focus);
    if (scope === 'selection' && a === b) { this.status('Select some text to search within it.', true); return null; }
    const w: Record<string, unknown> = { story: t.story, start: a };
    if (t.cell) { if (t.cell.table > Number.MAX_SAFE_INTEGER) w.note = t.cell.row; else w.cell = t.cell; }
    if (scope === 'selection') w.end = b;
    this.within = w;
    this.withinKey = key;
    return w;
  }

  private async run(op: string): Promise<void> {
    if (this.busy || !editor.hasDoc) return;
    const p = this.settings();
    if (this.mode !== 'object') {
      const w = this.withinFor(p);
      if (w === null) return;
      if (w) p.within = w;
    }
    p.op = op;
    this.busy = true;
    this.buttons();
    try {
      const r = await engine.call<Record<string, unknown>>('findChange', p);
      editor.schedule();
      this.ws.viewport.refreshOverlays();
      await this.show(op, (r.result ?? {}) as Record<string, unknown>);
    } catch (err) {
      this.status((err as Error).message, true);
    } finally {
      this.busy = false;
      this.buttons();
    }
  }

  private pageName(abs: number | null | undefined): string {
    if (abs === null || abs === undefined) return '';
    return editor.layout?.pages.find((x) => x.abs === abs)?.name ?? String(abs + 1);
  }

  private where(m: Match): string {
    if (m.overset) return 'in overset text, beyond the last frame';
    const p = this.pageName(m.page);
    return p ? `on page ${p}` : 'on the pasteboard';
  }

  private async show(op: string, r: Record<string, unknown>): Promise<void> {
    const m = r.match as Match | null | undefined;
    if (op === 'all') {
      this.matches = (r.matches as Match[]) ?? [];
      this.current = '';
      this.renderResults();
      const n = this.matches.length;
      this.status(n ? `${n}${r.truncated ? '+' : ''} found${r.truncated ? '; showing the first ' + n : ''}` : 'Nothing found.');
      return;
    }
    if (op === 'changeAll') {
      const n = Number(r.changed ?? 0), sk = Number(r.skipped ?? 0);
      this.status(n || sk ? `Changed ${n}${sk ? `; left ${sk} alone (they hold a footnote, table, anchored object or marker)` : ''}.` : 'Nothing found to change.');
      this.within = null;
      if (this.matches.length) await this.refreshAll();
      return;
    }
    let line = '';
    if (op === 'change' || op === 'changeFind') line = Number(r.changed ?? 0) ? 'Changed 1. ' : op === 'change' ? 'The selection is not a match: use Find Next first. ' : '';
    if (op === 'next' || op === 'changeFind') {
      if (m) {
        if (m.rect) this.ws.viewport.reveal(m.rect);
        line += `Found ${this.where(m)}${(r as { wrapped?: boolean }).wrapped ? ', after going back to the start' : ''}${m.locked ? ' (locked, cannot be changed)' : ''}.`;
        this.current = this.key(m);
        this.renderResults();
      } else line += 'Nothing found.';
    } else if (op === 'change' && m?.rect) this.ws.viewport.reveal(m.rect);
    this.status(line.trim());
    if ((op === 'change' || op === 'changeFind') && this.matches.length) await this.refreshAll();
  }

  private async refreshAll(): Promise<void> {
    const p = this.settings();
    const w = this.mode !== 'object' ? this.withinFor(p) : undefined;
    if (w === null) return;
    if (w) p.within = w;
    p.op = 'all';
    try {
      const r = await engine.call<{ matches: Match[] }>('findChange', p);
      this.matches = r.result?.matches ?? [];
      this.renderResults();
    } catch { }
  }

  private key(m: Match): string {
    if (m.id !== undefined) return `o${m.id}`;
    return `${m.story}:${m.note ?? ''}:${m.cell ? `${m.cell.table}.${m.cell.row}.${m.cell.col}` : ''}:${m.start}`;
  }

  private clearResults(): void {
    this.matches = [];
    this.current = '';
    this.renderResults();
    this.status('');
  }

  private renderResults(): void {
    patchList<Match>(this.results, this.matches, {
      key: (m) => this.key(m),
      create: (m) => {
        const r = el<HTMLButtonElement>('<button class="ff-row" type="button" role="option"><span class="ff-rpage"></span><span class="ff-rctx"><span class="ff-rb"></span><span class="ff-rhit"></span><span class="ff-ra"></span></span></button>');
        r.addEventListener('click', () => void this.pick(r.dataset.key as string));
        this.row(r, m);
        return r;
      },
      update: (r, m) => this.row(r, m),
    });
    setHidden(this.results, this.matches.length === 0);
  }

  private row(r: HTMLElement, m: Match): void {
    const page = m.overset ? 'overset' : this.pageName(m.page);
    setText(q(r, '.ff-rpage'), page ? (m.overset ? page : `p. ${page}`) : '');
    if (m.id !== undefined) {
      setText(q(r, '.ff-rb'), '');
      setText(q(r, '.ff-rhit'), KINDS.find((k) => k[0] === m.kind)?.[1] ?? m.kind ?? '');
      setText(q(r, '.ff-ra'), ` ${m.name || m.style || ''}`);
    } else {
      setText(q(r, '.ff-rb'), visible(m.before ?? ''));
      setText(q(r, '.ff-rhit'), visible(m.text ?? ''));
      setText(q(r, '.ff-ra'), visible(m.after ?? ''));
    }
    setClass(r, 'is-on', this.key(m) === this.current);
    setClass(r, 'is-locked', !!m.locked);
  }

  private async pick(key: string): Promise<void> {
    const m = this.matches.find((x) => this.key(x) === key);
    if (!m) return;
    this.current = key;
    this.renderResults();
    try {
      if (m.id !== undefined) {
        await editor.exec('selection.set', { ids: [m.id] }, true);
        if (m.rect) this.ws.viewport.reveal(m.rect);
      } else {
        const target: Record<string, unknown> = { story: m.story, start: m.start, end: m.end };
        if (m.note !== undefined) target.note = m.note;
        if (m.cell) target.cell = m.cell;
        const r = await engine.call<{ match: Match }>('findChange', { op: 'select', target });
        editor.schedule();
        this.ws.viewport.refreshOverlays();
        const got = r.result?.match;
        if (got?.rect) this.ws.viewport.reveal(got.rect);
        this.status(got ? `Selected ${this.where(got)}.` : '');
      }
    } catch (err) { this.status((err as Error).message, true); }
  }

  private status(text: string, error = false): void {
    const s = q(this.root, '.ff-status');
    setText(s, text);
    setClass(s, 'is-error', error);
  }

  private async loadQueries(): Promise<void> {
    try { this.queries = (await api<{ queries: Query[] }>('/find/queries')).queries ?? []; } catch { this.queries = []; }
    const cur = this.query.value;
    this.query.replaceChildren(new Option('Custom', ''));
    const add = (label: string, list: Query[]) => {
      if (!list.length) return;
      const g = document.createElement('optgroup');
      g.label = label;
      for (const x of list) g.append(new Option(`${x.name} (${MODES.find((m) => m[0] === x.mode)?.[1] ?? x.mode})`, x.name));
      this.query.append(g);
    };
    add('Saved', this.queries.filter((x) => !x.builtin));
    add('Built in', this.queries.filter((x) => x.builtin));
    this.query.value = this.queries.some((x) => x.name === cur) ? cur : '';
    this.buttons();
  }

  private loadQuery(name: string): void {
    const qy = this.queries.find((x) => x.name === name);
    if (!qy) { this.buttons(); return; }
    this.setMode(qy.mode);
    this.find.value = typeof qy.find === 'string' ? qy.find : '';
    this.change.value = typeof qy.change === 'string' ? qy.change : '';
    this.toggle(q(this.root, '.ff-case'), !!qy.caseSensitive);
    this.toggle(q(this.root, '.ff-word'), !!qy.wholeWord);
    const inc = (qy.include ?? {}) as Record<string, boolean>;
    this.toggle(q(this.root, '.ff-locked'), !!inc.locked);
    this.toggle(q(this.root, '.ff-hidden'), !!inc.hidden);
    this.toggle(q(this.root, '.ff-parents'), !!inc.parents);
    if (typeof qy.scope === 'string') this.scope.value = qy.scope;
    this.findFmt.set((qy.findFormat ?? {}) as Record<string, unknown>);
    this.changeFmt.set((qy.changeFormat ?? {}) as Record<string, unknown>);
    this.gFind.set(qy.glyph as Record<string, unknown> | undefined);
    this.gChange.set(qy.toGlyph as Record<string, unknown> | undefined);
    const o = (qy.object ?? {}) as Record<string, unknown>, c = (qy.objectChange ?? {}) as Record<string, unknown>;
    const set = (cls: string, v: unknown) => { const n = q<HTMLInputElement | HTMLSelectElement>(this.root, cls); const s = v === undefined || v === null ? '' : String(v); if (n instanceof HTMLSelectElement && s && ![...n.options].some((x) => x.value === s)) n.append(new Option(s, s)); n.value = s; };
    set('.ff-okind', o.kind); set('.ff-ostyle', o.objectStyle); set('.ff-ofill', o.fill); set('.ff-ostroke', o.stroke); set('.ff-olayer', o.layer); set('.ff-oweight', o.strokeWeight);
    set('.ff-oopacity', typeof o.opacity === 'number' ? Math.round(o.opacity * 100) : undefined);
    set('.ff-cstyle', c.objectStyle); set('.ff-cfill', c.fill); set('.ff-cstroke', c.stroke); set('.ff-cweight', c.strokeWeight);
    set('.ff-copacity', typeof c.opacity === 'number' ? Math.round(c.opacity * 100) : undefined);
    this.query.value = name;
    this.within = null;
    this.buttons();
    this.status(qy.builtin ? 'Built-in query loaded. Find All to preview, then Change All.' : 'Saved query loaded.');
  }

  private async saveQuery(): Promise<void> {
    const name = await promptName('Save Query', this.queries.find((x) => x.name === this.query.value && !x.builtin)?.name ?? '', 'Save');
    if (!name) return;
    const body = { ...this.settings(), name };
    try {
      const saved = await api<Query>('/find/queries', { json: body });
      await this.loadQueries();
      this.query.value = saved.name;
      this.buttons();
      this.status(`Saved "${saved.name}".`);
    } catch (err) { this.status((err as Error).message, true); }
  }

  private async deleteQuery(): Promise<void> {
    const name = this.query.value;
    if (!name) return;
    if (!(await confirmAction('Delete Query', `Delete the saved query "${name}"?`, 'Delete', true))) return;
    try {
      await api(`/find/queries?name=${encodeURIComponent(name)}`, { method: 'DELETE' });
      this.query.value = '';
      await this.loadQueries();
      this.status(`Deleted "${name}".`);
    } catch (err) { this.status((err as Error).message, true); }
  }
}

export function installFind(ws: Workspace): void {
  let win: FindWindow | null = null;
  const make = () => (win ??= new FindWindow(ws));
  const has = () => editor.hasDoc;
  ws.addCommands(
    { id: 'ui.find', label: 'Find/Change…', keys: ['Mod+F'], enabled: has, run: () => make().open() },
    { id: 'ui.findNext', label: 'Find Next', keys: ['Alt+Mod+F'], enabled: has, run: () => void make().findNext() },
  );
  ws.override('find.find', () => make().open());
  ws.override('find.change', () => make().open(true));
  ws.override('find.next', () => void make().findNext());
  ws.addMenuItems({ menu: 'Edit', items: ['-', 'ui.find', 'ui.findNext'], at: 'end' });
}
