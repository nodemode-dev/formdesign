import { confirmAction, Dialog, promptName } from '../../ui/dialogs';
import { el, patchList, q, setClass, setDisabled, setHidden, setText, setTitle } from '../../ui/dom';
import { ctx, type Attrs, type Snap } from './ctx';
import { grid, iconBtn, labelled, NumField, Pick, Seg, SwatchPick, type NumVal } from './fields';
import { ALIGNS, SIZES, STYLE_NAMES, TRACKS, textTarget } from './type';

type Kind = 'paragraph' | 'character';

const BUILTIN = new Set(['[No Paragraph Style]', '[Basic Paragraph]', '[None]']);
const VOLATILE = new Set(['change', 'xmlTag', 'conditions', 'ruby', 'kenten', 'tateChuYoko', 'otfFeatures']);
const CHAR_MAIN = ['fontFamily', 'fontStyle', 'size', 'leading', 'kerning', 'tracking', 'hScale', 'vScale', 'baselineShift', 'skew', 'fill', 'fillTint', 'stroke', 'strokeTint', 'capitalization', 'position', 'underline', 'strikethrough', 'language'];

function pickAttrs(src: Record<string, any>, keys?: string[]): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(src)) {
    if (VOLATILE.has(k) || v === null || v === undefined) continue;
    if (keys && !keys.includes(k)) continue;
    out[k] = v;
  }
  return out;
}

function uniqueName(base: string, taken: string[]): string {
  let i = 1;
  while (taken.includes(`${base} ${i}`)) i++;
  return `${base} ${i}`;
}

export class StylesPanel {
  readonly root = el(`<div class="fp-panel fp-stp"><div class="fp-stcur"><span class="fp-stcur-lab"></span><span class="fp-stcur-name"></span></div><div class="fp-listwrap"><div class="fp-list fp-stlist" role="listbox"></div><p class="fp-list-empty" hidden></p></div><div class="fp-foot"><span class="fp-foot-meta"></span><span class="fp-grow"></span></div></div>`);
  private picked = '';
  private readonly list: HTMLElement;
  private readonly btn: Record<string, HTMLButtonElement> = {};

  constructor(private readonly kind: Kind) {
    this.list = q(this.root, '.fp-stlist');
    this.list.setAttribute('aria-label', kind === 'paragraph' ? 'Paragraph styles' : 'Character styles');
    this.list.tabIndex = 0;
    const foot = q(this.root, '.fp-foot');
    this.btn.create = iconBtn('fromSel', kind === 'paragraph' ? 'New paragraph style from the selection' : 'New character style from the selection', () => void this.create());
    this.btn.redefine = iconBtn('redefine', 'Redefine the style to match the selection', () => void this.redefine());
    this.btn.edit = iconBtn('pencil', 'Style options', () => void this.edit());
    foot.append(this.btn.create, this.btn.redefine, this.btn.edit);
    if (kind === 'paragraph') { this.btn.del = iconBtn('trash', 'Delete style', () => void this.remove()); foot.append(this.btn.del); }
    setText(q(this.root, '.fp-stcur-lab'), 'Applied');
    this.list.addEventListener('keydown', (e) => this.key(e));
    ctx.on((s) => this.update(s));
  }

  private names(s: Snap): string[] { return this.kind === 'paragraph' ? s.paraStyles : s.charStyles; }
  private applied(s: Snap): string | null {
    if (!textTarget(s) || !s.attrs) return null;
    return this.kind === 'paragraph' ? s.attrs.paragraphStyle : s.attrs.characterStyle;
  }
  private overrides(s: Snap): number { return s.attrs ? (this.kind === 'paragraph' ? s.attrs.paraOverrides : s.attrs.charOverrides) : 0; }

  private update(s: Snap): void {
    const names = this.names(s), applied = this.applied(s), over = this.overrides(s);
    patchList<string>(this.list, names, {
      key: (n) => n,
      create: (n) => {
        const r = el('<div class="fp-strow" role="option" tabindex="-1"><span class="fp-stname"></span><span class="fp-stplus"></span></div>');
        r.addEventListener('click', () => this.apply(r.dataset.key as string));
        r.addEventListener('dblclick', () => { this.picked = r.dataset.key as string; void this.edit(); });
        this.row(r, n, applied, over);
        return r;
      },
      update: (r, n) => this.row(r, n, applied, over),
    });
    const empty = q(this.root, '.fp-list-empty');
    setHidden(empty, names.length > 0);
    setText(empty, s.mode === 'nodoc' ? 'Open a document to see its styles.' : 'No styles yet.');
    setText(q(this.root, '.fp-stcur-name'), applied ? `${applied}${over ? ' +' : ''}` : s.mode === 'nodoc' ? '' : 'No text selected');
    setClass(q(this.root, '.fp-stcur'), 'is-none', !applied);
    setText(q(this.root, '.fp-foot-meta'), s.mode === 'nodoc' ? '' : `${names.length} styles`);
    const tt = textTarget(s) && !!s.attrs;
    const target = this.target(s);
    setDisabled(this.btn.create, !tt);
    const fixed = (n: string | null) => !n || n === '[No Paragraph Style]' || n === '[None]';
    setDisabled(this.btn.redefine, !tt || fixed(applied));
    setDisabled(this.btn.edit, fixed(target));
    if (this.btn.del) setDisabled(this.btn.del, !target || BUILTIN.has(target));
    setTitle(this.btn.edit, target ? `Style options for ${target}` : 'Style options');
  }

  private target(s: Snap): string | null {
    const names = this.names(s);
    if (this.picked && names.includes(this.picked)) return this.picked;
    const a = this.applied(s);
    return a && names.includes(a) ? a : null;
  }

  private row(r: HTMLElement, name: string, applied: string | null, over: number): void {
    setText(q(r, '.fp-stname'), name);
    setText(q(r, '.fp-stplus'), applied === name && over ? '+' : '');
    setTitle(q(r, '.fp-stplus'), applied === name && over ? `${over} local ${over === 1 ? 'override' : 'overrides'}` : '');
    setClass(r, 'is-applied', applied === name);
    setClass(r, 'is-picked', this.picked === name && applied !== name);
    setClass(r, 'is-builtin', BUILTIN.has(name));
    r.setAttribute('aria-selected', String(applied === name));
  }

  private apply(name: string): void {
    this.picked = name;
    const s = ctx.snap;
    if (textTarget(s)) void ctx.run(this.kind === 'paragraph' ? 'style.paragraph.apply' : 'style.character.apply', { name });
    else this.update(s);
  }

  private key(e: KeyboardEvent): void {
    if (!['ArrowDown', 'ArrowUp', 'Enter'].includes(e.key)) return;
    e.preventDefault();
    e.stopPropagation();
    const names = this.names(ctx.snap);
    let i = names.indexOf(this.target(ctx.snap) ?? '');
    if (e.key === 'Enter') { if (i >= 0) this.apply(names[i]); return; }
    i = e.key === 'ArrowDown' ? Math.min(names.length - 1, i + 1) : Math.max(0, i - 1);
    this.picked = names[i] ?? '';
    this.update(ctx.snap);
    this.list.querySelector<HTMLElement>('.is-picked, .is-applied')?.scrollIntoView({ block: 'nearest' });
  }

  private async create(): Promise<void> {
    const s = ctx.snap;
    if (!s.attrs) return;
    const names = this.names(s);
    const name = await promptName(this.kind === 'paragraph' ? 'New Paragraph Style' : 'New Character Style', uniqueName(this.kind === 'paragraph' ? 'Paragraph Style' : 'Character Style', names), 'Create');
    if (!name) return;
    if (names.includes(name)) { ctx.fail(new Error(`A style named “${name}” already exists`)); return; }
    const ok = this.kind === 'paragraph'
      ? await ctx.batch([{ command: 'style.paragraph.create', params: { name, fromSelection: true } }, { command: 'style.paragraph.apply', params: { name } }])
      : await ctx.batch([{ command: 'style.character.create', params: { name, chars: pickAttrs(s.attrs.chars, CHAR_MAIN) } }, { command: 'style.character.apply', params: { name } }]);
    if (ok) this.picked = name;
  }

  private async redefine(): Promise<void> {
    const s = ctx.snap, name = this.applied(s);
    if (!s.attrs || !name) return;
    if (this.kind === 'paragraph') {
      await ctx.batch([
        { command: 'style.paragraph.edit', params: { name, para: pickAttrs(s.attrs.para), chars: pickAttrs(s.attrs.chars) } },
        { command: 'style.paragraph.apply', params: { name, clearOverrides: true } },
      ]);
    } else {
      await ctx.batch([
        { command: 'style.character.edit', params: { name, chars: pickAttrs(s.attrs.chars, CHAR_MAIN) } },
        { command: 'style.character.apply', params: { name } },
      ]);
    }
  }

  private async remove(): Promise<void> {
    const name = this.target(ctx.snap);
    if (!name || BUILTIN.has(name)) return;
    const ok = await confirmAction('Delete Paragraph Style', `Delete “${name}”? Paragraphs that use it change to [Basic Paragraph] and keep their look.`, 'Delete', true);
    if (!ok) return;
    if (await ctx.run('style.paragraph.delete', { name, replaceWith: '[Basic Paragraph]' })) this.picked = '';
  }

  private async edit(): Promise<void> {
    const name = this.target(ctx.snap);
    if (name) await styleDialog(this.kind, name);
  }
}

export function styleDialog(kind: Kind, name: string): Promise<void> {
  return new Promise((resolve) => {
    const s = ctx.snap;
    const a: Attrs | null = s.attrs;
    const exact = !!a && (kind === 'paragraph' ? a.paragraphStyle === name && !a.paraOverrides : a.characterStyle === name && !a.charOverrides);
    const c = exact && a ? a.chars : {}, p = exact && a ? a.para : {};
    const chars: Record<string, unknown> = {}, paras: Record<string, unknown> = {}, meta: Record<string, unknown> = {};
    const d = new Dialog(kind === 'paragraph' ? 'Paragraph Style Options' : 'Character Style Options', 480);
    const nameIn = el<HTMLInputElement>('<input class="cv-input fp-input" type="text" spellcheck="false">');
    nameIn.value = name;
    nameIn.addEventListener('keydown', (e) => e.stopPropagation());
    const others = (kind === 'paragraph' ? s.paraStyles : s.charStyles).filter((n) => n !== name);
    const based = new Pick('Based on', (v) => { meta.basedOn = v === '[No Paragraph Style]' || v === '[None]' ? null : v; }, { cls: 'fp-dd' });
    based.options(others.map((n) => ({ value: n, label: n })));
    based.set(null);
    const family = new Pick('Font family', (v) => { chars.fontFamily = v; }, { search: 'Search fonts', cls: 'fp-dd' });
    family.options(s.fonts.map((f) => ({ value: f, label: f })));
    family.set(typeof c.fontFamily === 'string' ? c.fontFamily : null);
    const style = new Pick('Font style', (v) => { chars.fontStyle = v; }, { cls: 'fp-dd' });
    style.options([...new Set([...(typeof c.fontStyle === 'string' ? [c.fontStyle] : []), ...STYLE_NAMES])].map((v) => ({ value: v, label: v })));
    style.set(typeof c.fontStyle === 'string' ? c.fontStyle : null);
    const num = (spec: ConstructorParameters<typeof NumField>[0], v: NumVal) => { const f = new NumField(spec); f.set(v); return f; };
    const size = num({ icon: 'fontSize', title: 'Font size', kind: 'pt', min: 0.1, max: 1296, presets: SIZES, onCommit: (v) => { chars.size = v; } }, typeof c.size === 'number' ? c.size : null);
    const lead = c.leading as { kind?: string; value?: number } | undefined;
    const leading = num({ icon: 'leading', title: 'Leading', kind: 'pt', min: 0, max: 5000, keywords: { auto: 'Auto' }, presets: ['auto', ...SIZES], onCommit: (v) => { chars.leading = v === 'auto' ? { kind: 'auto' } : { kind: 'points', value: v }; } }, lead ? (lead.kind === 'points' ? Number(lead.value) : 'auto') : null);
    const tracking = num({ icon: 'tracking', title: 'Tracking', kind: 'em', min: -1000, max: 10000, step: 5, decimals: 0, presets: TRACKS, onCommit: (v) => { chars.tracking = v; } }, typeof c.tracking === 'number' ? c.tracking : null);
    const colour = new SwatchPick('Colour', (n) => { chars.fill = n; colour.set(n); });
    colour.set(typeof c.fill === 'string' ? c.fill : null);
    const form = el('<div class="fp-form fp-stform"></div>');
    const sub = (t: string) => { const n = el('<div class="fp-form-sub"></div>'); setText(n, t); return n; };
    form.append(labelled('Name', nameIn), labelled('Based on', based.el));
    if (kind === 'paragraph') {
      const next = new Pick('Next style', (v) => { meta.nextStyle = v; }, { cls: 'fp-dd' });
      next.options([name, ...others].map((n) => ({ value: n, label: n === name ? `[Same style]` : n })));
      next.set(null);
      form.append(labelled('Next style', next.el));
    }
    form.append(sub('Basic Character Formats'), labelled('Font', family.el), grid(style.el, size.el), grid(leading.el, tracking.el), labelled('Colour', colour.el));
    if (kind === 'paragraph') {
      const align = new Seg(ALIGNS.slice(0, 7), (id) => { paras.align = id; align.set(id); });
      align.set(typeof p.align === 'string' ? p.align : null);
      const pnum = (key: string, icon: string, title: string, k: 'measure' | 'measureV') => num({ icon, title, kind: k, min: key === 'firstLineIndent' || key === 'leftIndent' ? -8640 : 0, max: 8640, onCommit: (v) => { paras[key] = v; } }, typeof p[key] === 'number' ? p[key] : null).el;
      form.append(sub('Indents and Spacing'), align.el, grid(
        pnum('leftIndent', 'indentL', 'Left indent', 'measure'), pnum('firstLineIndent', 'indentFirst', 'First line indent', 'measure'),
        pnum('spaceBefore', 'spaceBefore', 'Space before', 'measureV'), pnum('spaceAfter', 'spaceAfter', 'Space after', 'measureV'),
      ));
    }
    const note = el('<p class="fp-note"></p>');
    setText(note, exact ? 'Values shown are the style’s own, read from the selected text.' : 'Empty fields keep the style’s current values; only what you change is written.');
    form.append(note);
    d.body.append(form);
    d.button('Cancel', 'ghost', () => d.close());
    d.button('OK', 'primary', async () => {
      (document.activeElement as HTMLElement | null)?.blur?.();
      const params: Record<string, unknown> = { name };
      const rename = nameIn.value.trim();
      if (rename && rename !== name) params.rename = rename;
      if ('basedOn' in meta) params.basedOn = meta.basedOn;
      if (kind === 'paragraph' && 'nextStyle' in meta) params.nextStyle = meta.nextStyle;
      if (Object.keys(chars).length) params.chars = chars;
      if (kind === 'paragraph' && Object.keys(paras).length) params.para = paras;
      if (Object.keys(params).length === 1) { d.close(); return; }
      if (await ctx.run(kind === 'paragraph' ? 'style.paragraph.edit' : 'style.character.edit', params)) d.close();
    });
    d.onClose = () => { d.scrim.remove(); resolve(); };
    d.open(nameIn);
  });
}
