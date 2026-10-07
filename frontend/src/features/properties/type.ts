import { ctx, type Snap } from './ctx';
import { Check, NumField, Pick, Seg, SwatchPick, Toggle, type NumVal, type Opt } from './fields';

export const STYLE_NAMES = ['Thin', 'Light', 'Regular', 'Italic', 'Medium', 'Medium Italic', 'SemiBold', 'SemiBold Italic', 'Bold', 'Bold Italic', 'ExtraBold', 'Black', 'Black Italic'];
export const LANGUAGES = ['[No Language]', 'English: USA', 'English: UK', 'English: Canadian', 'German: 2006 Reform', 'German: Swiss 2006 Reform', 'French', 'French: Canadian', 'Spanish', 'Italian', 'Portuguese', 'Portuguese: Brazilian', 'Dutch: 2005 Reform', 'Danish', 'Swedish', 'Norwegian: Bokmål', 'Finnish', 'Polish', 'Czech', 'Hungarian', 'Greek', 'Russian', 'Ukrainian', 'Turkish', 'Japanese', 'Chinese', 'Korean', 'Arabic', 'Hebrew'];
export const SIZES: NumVal[] = [6, 7, 8, 9, 10, 11, 12, 14, 18, 21, 24, 30, 36, 48, 60, 72];
export const TRACKS: NumVal[] = [-100, -75, -50, -25, -10, -5, 0, 5, 10, 25, 50, 75, 100, 200];

export const ALIGNS = [
  { id: 'left', icon: 'alL', title: 'Align left' },
  { id: 'center', icon: 'alC', title: 'Align centre' },
  { id: 'right', icon: 'alR', title: 'Align right' },
  { id: 'leftJustified', icon: 'alJL', title: 'Justify with last line aligned left' },
  { id: 'centerJustified', icon: 'alJC', title: 'Justify with last line aligned centre' },
  { id: 'rightJustified', icon: 'alJR', title: 'Justify with last line aligned right' },
  { id: 'fullyJustified', icon: 'alJF', title: 'Justify all lines' },
  { id: 'towardsSpine', icon: 'alTS', title: 'Align towards spine' },
  { id: 'awayFromSpine', icon: 'alAS', title: 'Align away from spine' },
];

export function textTarget(s: Snap): boolean { return s.mode === 'text' || (s.mode === 'objects' && s.kinds.has('text')); }

export function char(attrs: Record<string, unknown>): void { void ctx.run('type.char', { attrs }); }
export function para(attrs: Record<string, unknown>): void { void ctx.run('type.para', { attrs }); }

function lazy<T>(make: () => T): () => T {
  let v: T | undefined;
  const get = () => (v ??= make());
  (get as unknown as { peek: () => T | undefined }).peek = () => v;
  return get;
}

function peek<T>(g: () => T): T | undefined { return (g as unknown as { peek: () => T | undefined }).peek(); }

export class CharFields {
  constructor(private readonly small = false) {}

  readonly family = lazy(() => new Pick('Font family', (v) => char({ fontFamily: v }), { search: 'Search fonts', cls: `fp-dd fp-dd-family${this.small ? ' fp-dd-sm' : ''}` }));
  readonly style = lazy(() => new Pick('Font style', (v) => char({ fontStyle: v }), { cls: `fp-dd${this.small ? ' fp-dd-sm' : ''}` }));
  readonly size = lazy(() => new NumField({ icon: 'fontSize', title: 'Font size', kind: 'pt', min: 0.1, max: 1296, step: 1, presets: SIZES, small: this.small, onCommit: (v) => char({ size: v }) }));
  readonly leading = lazy(() => new NumField({
    icon: 'leading', title: 'Leading', kind: 'pt', min: 0, max: 5000, step: 1, small: this.small, keywords: { auto: 'Auto' }, presets: ['auto', ...SIZES],
    base: () => this.autoLeading(), onCommit: (v) => char({ leading: v === 'auto' ? { kind: 'auto' } : { kind: 'points', value: v } }),
  }));
  readonly kerning = lazy(() => new NumField({
    icon: 'kerning', title: 'Kerning between characters', kind: 'em', min: -1000, max: 10000, step: 5, small: this.small,
    keywords: { metrics: 'Metrics', optical: 'Optical' }, presets: ['metrics', 'optical', 0, -50, -25, -10, -5, 5, 10, 25, 50],
    onCommit: (v) => char({ kerning: typeof v === 'string' ? { kind: v } : v === 0 ? { kind: 'none' } : { kind: 'manual', value: v } }),
  }));
  readonly tracking = lazy(() => new NumField({ icon: 'tracking', title: 'Tracking', kind: 'em', min: -1000, max: 10000, step: 5, presets: TRACKS, small: this.small, decimals: 0, onCommit: (v) => char({ tracking: v }) }));
  readonly vScale = lazy(() => new NumField({ icon: 'vScale', title: 'Vertical scale', kind: 'pct', min: 1, max: 1000, step: 1, small: this.small, onCommit: (v) => char({ vScale: Number(v) / 100 }) }));
  readonly hScale = lazy(() => new NumField({ icon: 'hScale', title: 'Horizontal scale', kind: 'pct', min: 1, max: 1000, step: 1, small: this.small, onCommit: (v) => char({ hScale: Number(v) / 100 }) }));
  readonly baseline = lazy(() => new NumField({ icon: 'baseline', title: 'Baseline shift', kind: 'pt', min: -5000, max: 5000, step: 1, small: this.small, onCommit: (v) => char({ baselineShift: v }) }));
  readonly skew = lazy(() => new NumField({ icon: 'skew', title: 'Skew (false italic)', kind: 'deg', min: -85, max: 85, step: 1, small: this.small, onCommit: (v) => char({ skew: v }) }));
  readonly language = lazy(() => new Pick('Language', (v) => char({ language: v }), { search: 'Search languages', cls: `fp-dd${this.small ? ' fp-dd-sm' : ''}` }));
  readonly fill = lazy(() => new SwatchPick('Text colour', (n) => char({ fill: n }), this.small));
  readonly charStyle = lazy(() => new Pick('Character style', (v) => void ctx.run('style.character.apply', { name: v }), { cls: `fp-dd${this.small ? ' fp-dd-sm' : ''}` }));
  readonly allCaps = lazy(() => new Toggle('allCaps', 'All caps', (on) => char({ capitalization: on ? 'allCaps' : 'normal' })));
  readonly smallCaps = lazy(() => new Toggle('smallCaps', 'Small caps', (on) => char({ capitalization: on ? 'smallCaps' : 'normal' })));
  readonly superscript = lazy(() => new Toggle('superscript', 'Superscript', (on) => char({ position: on ? 'superscript' : 'normal' })));
  readonly subscript = lazy(() => new Toggle('subscript', 'Subscript', (on) => char({ position: on ? 'subscript' : 'normal' })));
  readonly underline = lazy(() => new Toggle('underline', 'Underline', (on) => char({ underline: on })));
  readonly strike = lazy(() => new Toggle('strike', 'Strikethrough', (on) => char({ strikethrough: on })));

  private autoLeading(): number {
    const a = ctx.snap.attrs;
    return a ? Number(a.chars.size ?? 12) * Number(a.para.autoLeading ?? 1.2) : 14.4;
  }

  update(s: Snap): void {
    const on = textTarget(s) && !!s.attrs;
    const c = on ? (s.attrs as NonNullable<Snap['attrs']>).chars : {};
    const fam = peek(this.family);
    if (fam) {
      fam.options(s.fonts.map((f): Opt => ({ value: f, label: f })));
      fam.set(on ? String(c.fontFamily ?? '') : null);
      fam.setDisabled(!on);
    }
    const sty = peek(this.style);
    if (sty) {
      const cur = String(c.fontStyle ?? 'Regular');
      sty.options([...new Set([cur, ...STYLE_NAMES])].map((v) => ({ value: v, label: v })));
      sty.set(on ? cur : null);
      sty.setDisabled(!on);
    }
    const num = (g: () => NumField, v: NumVal) => { const f = peek(g); if (f) { f.set(on ? v : null); f.setDisabled(!on); } };
    num(this.size, typeof c.size === 'number' ? c.size : null);
    const lead = c.leading as { kind?: string; value?: number } | undefined;
    num(this.leading, lead ? (lead.kind === 'points' ? Number(lead.value) : 'auto') : null);
    const kern = c.kerning as { kind?: string; value?: number } | undefined;
    num(this.kerning, kern ? (kern.kind === 'manual' ? Number(kern.value) : kern.kind === 'none' ? 0 : String(kern.kind)) : null);
    num(this.tracking, typeof c.tracking === 'number' ? c.tracking : null);
    num(this.vScale, typeof c.vScale === 'number' ? c.vScale * 100 : null);
    num(this.hScale, typeof c.hScale === 'number' ? c.hScale * 100 : null);
    num(this.baseline, typeof c.baselineShift === 'number' ? c.baselineShift : null);
    num(this.skew, typeof c.skew === 'number' ? c.skew : null);
    const lang = peek(this.language);
    if (lang) {
      const cur = String(c.language ?? 'English: USA');
      lang.options([...new Set([...LANGUAGES, cur])].map((v) => ({ value: v, label: v })));
      lang.set(on ? cur : null);
      lang.setDisabled(!on);
    }
    const fill = peek(this.fill);
    if (fill) { fill.set(on ? String(c.fill ?? '[Black]') : null, on ? Number(c.fillTint ?? 1) : 1); fill.setDisabled(!on); }
    const cs = peek(this.charStyle);
    if (cs) {
      cs.options(s.charStyles.map((n) => ({ value: n, label: n })));
      cs.set(on ? String(s.attrs?.characterStyle ?? '[None]') : null);
      cs.setDisabled(!on);
    }
    const tog = (g: () => Toggle, v: boolean) => { const t = peek(g); if (t) { t.set(on ? v : false); t.setDisabled(!on); } };
    tog(this.allCaps, c.capitalization === 'allCaps');
    tog(this.smallCaps, c.capitalization === 'smallCaps' || c.capitalization === 'openTypeAllSmallCaps');
    tog(this.superscript, c.position === 'superscript' || c.position === 'otSuperscript');
    tog(this.subscript, c.position === 'subscript' || c.position === 'otSubscript');
    tog(this.underline, !!c.underline);
    tog(this.strike, !!c.strikethrough);
  }
}

export class ParaFields {
  constructor(private readonly small = false, private readonly aligns = ALIGNS.map((a) => a.id)) {}

  readonly align = lazy(() => new Seg(ALIGNS.filter((a) => this.aligns.includes(a.id)), (id) => para({ align: id }), this.small ? 'is-sm' : ''));
  readonly left = lazy(() => new NumField({ icon: 'indentL', title: 'Left indent', kind: 'measure', min: -8640, max: 8640, small: this.small, onCommit: (v) => para({ leftIndent: v }) }));
  readonly right = lazy(() => new NumField({ icon: 'indentR', title: 'Right indent', kind: 'measure', min: -8640, max: 8640, small: this.small, onCommit: (v) => para({ rightIndent: v }) }));
  readonly first = lazy(() => new NumField({ icon: 'indentFirst', title: 'First line left indent', kind: 'measure', min: -8640, max: 8640, small: this.small, onCommit: (v) => para({ firstLineIndent: v }) }));
  readonly last = lazy(() => new NumField({ icon: 'indentLast', title: 'Last line right indent', kind: 'measure', min: -8640, max: 8640, small: this.small, onCommit: (v) => para({ lastLineIndent: v }) }));
  readonly before = lazy(() => new NumField({ icon: 'spaceBefore', title: 'Space before', kind: 'measureV', min: 0, max: 8640, small: this.small, onCommit: (v) => para({ spaceBefore: v }) }));
  readonly after = lazy(() => new NumField({ icon: 'spaceAfter', title: 'Space after', kind: 'measureV', min: 0, max: 8640, small: this.small, onCommit: (v) => para({ spaceAfter: v }) }));
  readonly dropLines = lazy(() => new NumField({ icon: 'dropLines', title: 'Drop cap number of lines', kind: 'int', min: 0, max: 25, small: this.small, onCommit: (v) => para({ dropCapLines: v }) }));
  readonly dropChars = lazy(() => new NumField({ icon: 'dropChars', title: 'Drop cap one or more characters', kind: 'int', min: 0, max: 150, small: this.small, onCommit: (v) => para({ dropCapChars: v }) }));
  readonly keepNext = lazy(() => new NumField({ icon: 'keep', title: 'Keep with next lines', kind: 'int', min: 0, max: 5, small: this.small, onCommit: (v) => para({ keepWithNext: v }) }));
  readonly hyphenate = lazy(() => new Check('Hyphenate', (on) => para({ hyphenate: on })));
  readonly keepTogether = lazy(() => new Check('Keep lines together', (on) => para({ keepLinesTogether: on })));
  readonly grid = lazy(() => new Seg([
    { id: 'none', text: 'Off', title: 'Do not align to baseline grid' },
    { id: 'allLines', text: 'All', title: 'Align all lines to baseline grid' },
    { id: 'firstLineOnly', text: 'First', title: 'Align only first line to baseline grid' },
  ], (id) => para({ gridAlign: id }), 'is-text'));
  readonly list = lazy(() => new Seg([
    { id: 'none', icon: 'listNone', title: 'No bullets or numbering' },
    { id: 'bullets', icon: 'bullets', title: 'Bulleted list' },
    { id: 'numbers', icon: 'numbers', title: 'Numbered list' },
  ], (id) => para({ listType: id })));
  readonly paraStyle = lazy(() => new Pick('Paragraph style', (v) => void ctx.run('style.paragraph.apply', { name: v }), { cls: `fp-dd${this.small ? ' fp-dd-sm' : ''}` }));

  update(s: Snap): void {
    const on = textTarget(s) && !!s.attrs;
    const p = on ? (s.attrs as NonNullable<Snap['attrs']>).para : {};
    const al = peek(this.align);
    if (al) { al.set(on ? String(p.align ?? 'left') : null); al.setDisabled(!on); }
    const num = (g: () => NumField, v: unknown) => { const f = peek(g); if (f) { f.set(on && typeof v === 'number' ? v : null); f.setDisabled(!on); } };
    num(this.left, p.leftIndent);
    num(this.right, p.rightIndent);
    num(this.first, p.firstLineIndent);
    num(this.last, p.lastLineIndent);
    num(this.before, p.spaceBefore);
    num(this.after, p.spaceAfter);
    num(this.dropLines, p.dropCapLines);
    num(this.dropChars, p.dropCapChars);
    num(this.keepNext, p.keepWithNext);
    const chk = (g: () => Check, v: unknown) => { const c = peek(g); if (c) { c.set(on ? !!v : false); c.setDisabled(!on); } };
    chk(this.hyphenate, p.hyphenate);
    chk(this.keepTogether, p.keepLinesTogether);
    const gr = peek(this.grid);
    if (gr) { gr.set(on ? String(p.gridAlign ?? 'none') : null); gr.setDisabled(!on); }
    const li = peek(this.list);
    if (li) { li.set(on ? String(p.listType ?? 'none') : null); li.setDisabled(!on); }
    const ps = peek(this.paraStyle);
    if (ps) {
      ps.options(s.paraStyles.map((n) => ({ value: n, label: n })));
      ps.set(on ? String(s.attrs?.paragraphStyle ?? '[Basic Paragraph]') : null);
      ps.setDisabled(!on);
    }
  }
}
