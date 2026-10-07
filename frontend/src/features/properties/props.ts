import { Dialog } from '../../ui/dialogs';
import { el, q, setHidden, setHtml, setText } from '../../ui/dom';
import type { Workspace } from '../../ui/workspace';
import { ctx, kindOf, type DocSetup, type ItemInfo, type Snap } from './ctx';
import { Check, grid, iconBtn, labelled, NumField, Pick, row, same, Section, Seg, Toggle, type NumVal } from './fields';
import { glyph } from './glyphs';
import { ObjFields } from './objects';
import { CharFields, ParaFields } from './type';
import { LABELS, type Unit } from './units';

const ICON: Record<string, string> = { text: 'textFrame', graphic: 'graphicFrame', group: 'group', line: 'line' };

function kindName(it: ItemInfo): string {
  const k = kindOf(it);
  if (k === 'text') return 'Text Frame';
  if (k === 'graphic') return 'Graphic Frame';
  if (k === 'group') return 'Group';
  const raw = it.kind.replace(/[<>]/g, '').trim();
  return raw ? raw.replace(/\b\w/g, (c) => c.toUpperCase()) : 'Object';
}

function shapeIcon(it: ItemInfo): string {
  const k = kindOf(it);
  if (ICON[k]) return ICON[k];
  if (/ellipse|oval/i.test(it.kind)) return 'ellipse';
  if (/polygon|star/i.test(it.kind)) return 'polygon';
  if (/path|curve|spline/i.test(it.kind)) return 'path';
  return 'rectangle';
}

const WRAPS = [
  { id: 'none', icon: 'wrapNone', title: 'No text wrap' },
  { id: 'boundingBox', icon: 'wrapBox', title: 'Wrap around bounding box' },
  { id: 'contour', icon: 'wrapContour', title: 'Wrap around object shape' },
  { id: 'jumpObject', icon: 'wrapJump', title: 'Jump object' },
  { id: 'jumpToNextColumn', icon: 'wrapColumn', title: 'Jump to next column' },
];
const FITS = [
  { id: 'fillProportionally', icon: 'fitFill', title: 'Fill frame proportionally' },
  { id: 'fitProportionally', icon: 'fitProp', title: 'Fit content proportionally' },
  { id: 'fitContentToFrame', icon: 'fitContent', title: 'Fit content to frame' },
  { id: 'centerContent', icon: 'fitCenter', title: 'Centre content' },
  { id: 'fitFrameToContent', icon: 'fitFrame', title: 'Fit frame to content' },
];
const VJ = [
  { id: 'top', icon: 'vjTop', title: 'Align top' },
  { id: 'center', icon: 'vjCenter', title: 'Align centre' },
  { id: 'bottom', icon: 'vjBottom', title: 'Align bottom' },
  { id: 'justify', icon: 'vjJustify', title: 'Justify vertically' },
];
const AUTOSIZE = [
  { value: 'off', label: 'Off' }, { value: 'heightOnly', label: 'Height Only' }, { value: 'widthOnly', label: 'Width Only' },
  { value: 'heightAndWidth', label: 'Height and Width' }, { value: 'heightAndWidthProportionally', label: 'Height and Width (Keep Proportions)' },
];
const ALIGN_TO = [
  { value: 'selection', label: 'Align to Selection' }, { value: 'keyObject', label: 'Align to Key Object' },
  { value: 'margins', label: 'Align to Margins' }, { value: 'page', label: 'Align to Page' }, { value: 'spread', label: 'Align to Spread' },
];
const INTENTS = [{ value: 'print', label: 'Print' }, { value: 'web', label: 'Web' }, { value: 'mobile', label: 'Mobile' }];
const UNITS = (Object.keys(LABELS) as Unit[]).map((u) => ({ value: u, label: LABELS[u] }));

function frameOpts(params: Record<string, unknown>, memo: Parameters<typeof ctx.remember>[1]): void {
  ctx.remember(ctx.snap.ids, memo);
  void ctx.run('object.textFrameOptions', params);
}

function setup(params: Record<string, unknown>): void { void ctx.run('layout.documentSetup', params); }

export class PropertiesPanel {
  readonly root = el(`<div class="fp-panel fp-props"><div class="fp-head"><span class="fp-tile"></span><div class="fp-head-text"><div class="fp-head-title"></div><div class="fp-head-sub"></div></div></div><div class="fp-view" data-view="doc"></div><div class="fp-view" data-view="obj"></div><div class="fp-view" data-view="text"></div><div class="fp-empty" data-view="nodoc">Open or make a document to see its properties.</div></div>`);
  private readonly obj = new ObjFields();
  private readonly ch = new CharFields();
  private readonly pa = new ParaFields(false, ['left', 'center', 'right', 'leftJustified', 'fullyJustified', 'towardsSpine', 'awayFromSpine']);
  private readonly tch = new CharFields();
  private readonly tpa = new ParaFields(false, ['left', 'center', 'right', 'leftJustified', 'fullyJustified', 'towardsSpine', 'awayFromSpine']);
  private readonly secs = new Map<string, Section>();
  private readonly doc: Record<string, NumField> = {};
  private facing!: Check;
  private orient!: Seg;
  private intent!: Pick;
  private hunits!: Pick;
  private marginLink!: Toggle;
  private bleedLink!: Toggle;
  private frame: Record<string, NumField> = {};
  private vj!: Seg;
  private autoSize!: Pick;
  private fit!: Seg;
  private autoFit!: Check;
  private alignTo!: Pick;
  private alignTarget = 'selection';
  private wrap!: Seg;
  private wrapOffset!: NumField;

  constructor(private readonly ws: Workspace) {
    this.buildDoc();
    this.buildObj();
    this.buildText();
    ctx.on((s) => this.update(s));
  }

  private sec(view: string, id: string, title: string): Section {
    const s = new Section(id, title);
    this.secs.set(id, s);
    q(this.root, `[data-view="${view}"]`).appendChild(s.root);
    return s;
  }

  private buildDoc(): void {
    const d = this.doc;
    d.w = new NumField({ label: 'W', title: 'Page width', kind: 'measure', min: 1, onCommit: (v) => setup({ width: v }) });
    d.h = new NumField({ label: 'H', title: 'Page height', kind: 'measureV', min: 1, onCommit: (v) => setup({ height: v }) });
    d.pages = new NumField({ icon: 'pages', title: 'Number of pages', kind: 'int', min: 1, max: 9999, onCommit: (v) => setup({ pages: v }) });
    this.orient = new Seg([{ id: 'portrait', icon: 'portrait', title: 'Portrait' }, { id: 'landscape', icon: 'landscape', title: 'Landscape' }], (id) => {
      const st = ctx.snap.setup;
      if (!st) return;
      const land = st.width > st.height;
      if ((id === 'landscape') !== land) setup({ width: st.height, height: st.width });
    }, 'fp-orient');
    this.facing = new Check('Facing pages', (on) => setup({ facingPages: on }));
    this.intent = new Pick('Intent', (v) => setup({ intent: v }), { cls: 'fp-dd' });
    this.intent.options(INTENTS);
    this.hunits = new Pick('Ruler units', (v) => void ctx.run('document.preferences', { horizontalUnits: v, verticalUnits: v }), { cls: 'fp-dd' });
    this.hunits.options(UNITS);
    const s1 = this.sec('doc', 'document', 'Document');
    s1.body.append(
      grid(d.w.el, d.h.el),
      grid(this.orient.el, d.pages.el),
      this.facing.el,
      grid(labelled('Intent', this.intent.el), labelled('Units', this.hunits.el)),
    );
    const m = (name: string, k: 'top' | 'bottom' | 'inside' | 'outside') => new NumField({ label: name, title: `${name} margin`, kind: k === 'top' || k === 'bottom' ? 'measureV' : 'measure', min: 0, onCommit: (v) => this.margins(k, Number(v)) });
    d.mt = m('T', 'top'); d.mb = m('B', 'bottom'); d.mi = m('I', 'inside'); d.mo = m('O', 'outside');
    this.marginLink = new Toggle('link', 'Make all settings the same', () => this.marginLink.set(!this.marginLink.on));
    this.marginLink.el.classList.add('fp-link', 'is-tall');
    d.cols = new NumField({ icon: 'columns', title: 'Number of columns', kind: 'int', min: 1, max: 216, onCommit: (v) => void ctx.run('layout.marginsAndColumns', { columns: v }) });
    d.gutter = new NumField({ icon: 'gutter', title: 'Column gutter', kind: 'measure', min: 0, onCommit: (v) => { ctx.remember([-1], { colGutter: Number(v) }); void ctx.run('layout.marginsAndColumns', { gutter: v }); } });
    const s2 = this.sec('doc', 'margins', 'Margins and Columns');
    s2.body.append(el('<div class="fp-linked"></div>'));
    const lk = q(s2.body, '.fp-linked');
    lk.append(grid(d.mt.el, d.mb.el, d.mi.el, d.mo.el), this.marginLink.el);
    s2.body.append(grid(d.cols.el, d.gutter.el), el('<p class="fp-note">Margins and columns apply to every page.</p>'));
    const b = (name: string, i: number) => new NumField({ label: name, title: `Bleed ${['top', 'bottom', 'inside', 'outside'][i]}`, kind: i < 2 ? 'measureV' : 'measure', min: 0, onCommit: (v) => this.bleed(i, Number(v)) });
    d.bt = b('T', 0); d.bb = b('B', 1); d.bi = b('I', 2); d.bo = b('O', 3);
    this.bleedLink = new Toggle('link', 'Make all settings the same', () => this.bleedLink.set(!this.bleedLink.on));
    this.bleedLink.el.classList.add('fp-link', 'is-tall');
    this.bleedLink.set(true);
    const s3 = this.sec('doc', 'bleed', 'Bleed');
    s3.body.append(el('<div class="fp-linked"></div>'));
    q(s3.body, '.fp-linked').append(grid(d.bt.el, d.bb.el, d.bi.el, d.bo.el), this.bleedLink.el);
    const btn = el<HTMLButtonElement>(`<button class="cv-btn fp-wide" type="button">${glyph('setup', 14)}<span>Document Setup…</span></button>`);
    btn.addEventListener('click', () => void this.setupDialog());
    q(this.root, '[data-view="doc"]').appendChild(el('<div class="fp-actions"></div>')).appendChild(btn);
  }

  private margins(k: 'top' | 'bottom' | 'inside' | 'outside', v: number): void {
    const pg = this.curPage();
    if (this.marginLink.on) { void ctx.run('layout.marginsAndColumns', { margins: v }); return; }
    const m = { ...(pg?.margins ?? { top: 0, bottom: 0, inside: 0, outside: 0 }), [k]: v };
    void ctx.run('layout.marginsAndColumns', { margins: m });
  }

  private bleed(i: number, v: number): void {
    const st = ctx.snap.setup;
    if (this.bleedLink.on || !st) { setup({ bleed: v }); return; }
    const b = [...(st.bleed ?? [0, 0, 0, 0])];
    b[i] = v;
    setup({ bleed: b });
  }

  private curPage() {
    const s = ctx.snap;
    const abs = this.ws.viewport.currentPage();
    return s.allPages.find((p) => p.index === abs) ?? s.page;
  }

  private buildObj(): void {
    const o = this.obj;
    const t = this.sec('obj', 'transform', 'Transform');
    const wh = el('<div class="fp-linked"></div>');
    wh.append(grid(o.x.el, o.w.el, o.y.el, o.h.el), o.link.el);
    o.link.el.classList.add('is-tall');
    t.body.append(row(o.ref.el, wh), row(o.rot.el, o.shear.el, o.flipH, o.flipV));
    const a = this.sec('obj', 'appearance', 'Appearance');
    a.body.append(labelled('Fill', o.fill.el), labelled('Stroke', row(o.stroke.el, o.weight.el)), labelled('Opacity', o.opacity.el));
    const f = this.frame;
    f.cols = new NumField({ icon: 'columns', title: 'Number of columns', kind: 'int', min: 1, max: 40, onCommit: (v) => frameOpts({ columns: v }, {}) });
    f.gutter = new NumField({ icon: 'gutter', title: 'Column gutter', kind: 'measure', min: 0, onCommit: (v) => frameOpts({ gutter: v }, { gutter: Number(v) }) });
    f.inset = new NumField({ icon: 'inset', title: 'Inset spacing (all sides)', kind: 'measure', min: 0, onCommit: (v) => frameOpts({ inset: v }, { inset: Number(v) }) });
    this.vj = new Seg(VJ, (id) => frameOpts({ verticalJustification: id }, { vj: id }));
    this.autoSize = new Pick('Auto-size', (v) => frameOpts({ autoSize: v }, { autoSize: v }), { cls: 'fp-dd' });
    this.autoSize.options(AUTOSIZE);
    const tf = this.sec('obj', 'frame', 'Text Frame');
    tf.body.append(grid(f.cols.el, f.gutter.el), grid(f.inset.el, this.vj.el), labelled('Auto-size', this.autoSize.el));
    this.fit = new Seg(FITS, (id) => { ctx.remember(ctx.snap.ids, { fitting: id }); void ctx.run('object.fit', { mode: id }); });
    this.autoFit = new Check('Auto-Fit', (on) => { ctx.remember(ctx.snap.ids, { autoFit: on }); void ctx.run('object.fittingOptions', { autoFit: on }); }, 'Refit the content whenever the frame is resized');
    const fs = this.sec('obj', 'fitting', 'Frame Fitting');
    fs.body.append(this.fit.el, this.autoFit.el);
    const ch = this.sec('obj', 'otype', 'Character');
    ch.body.append(this.ch.family().el, grid(this.ch.style().el, this.ch.size().el), grid(this.ch.leading().el, this.ch.tracking().el), labelled('Colour', this.ch.fill().el));
    ch.tools.append(iconBtn('pencil', 'Character panel', () => this.ws.showPanel('character'), 'is-sm'));
    const pa = this.sec('obj', 'opara', 'Paragraph');
    pa.body.append(this.pa.align().el, labelled('Style', this.pa.paraStyle().el));
    pa.tools.append(iconBtn('pencil', 'Paragraph panel', () => this.ws.showPanel('paragraph'), 'is-sm'));
    this.alignTo = new Pick('Align to', (v) => { this.alignTarget = v; }, { cls: 'fp-dd' });
    this.alignTo.options(ALIGN_TO);
    this.alignTo.set('selection');
    const al = this.sec('obj', 'align', 'Align');
    const edge = (id: string, icon: string, title: string) => iconBtn(icon, title, () => void ctx.run('object.align', { edge: id, to: this.alignTarget }));
    const dist = (axis: string, by: string, icon: string, title: string) => iconBtn(icon, title, () => void ctx.run('object.distribute', { axis, by }));
    al.body.append(
      this.alignTo.el,
      el('<div class="fp-btnrow"></div>'),
    );
    const br = q(al.body, '.fp-btnrow');
    br.append(edge('left', 'aLeft', 'Align left edges'), edge('hcenter', 'aHC', 'Align horizontal centres'), edge('right', 'aRight', 'Align right edges'), el('<span class="fp-vsep"></span>'),
      edge('top', 'aTop', 'Align top edges'), edge('vcenter', 'aVC', 'Align vertical centres'), edge('bottom', 'aBottom', 'Align bottom edges'));
    const dr = el('<div class="fp-btnrow"></div>');
    dr.append(dist('horizontal', 'centers', 'dH', 'Distribute horizontal centres'), dist('vertical', 'centers', 'dV', 'Distribute vertical centres'), el('<span class="fp-vsep"></span>'),
      dist('horizontal', 'spacing', 'dHS', 'Distribute horizontal space'), dist('vertical', 'spacing', 'dVS', 'Distribute vertical space'));
    al.body.append(dr);
    this.wrap = new Seg(WRAPS, (id) => { ctx.remember(ctx.snap.ids, { wrap: id }); void ctx.run('object.textWrap', { mode: id }); });
    this.wrapOffset = new NumField({ icon: 'inset', title: 'Wrap offset (all sides)', kind: 'measure', min: 0, onCommit: (v) => {
      const mode = ctx.recall(ctx.snap.ids, 'wrap') ?? 'boundingBox';
      ctx.remember(ctx.snap.ids, { wrap: mode === 'none' ? 'boundingBox' : mode, wrapOffset: Number(v) });
      void ctx.run('object.textWrap', { mode: mode === 'none' ? 'boundingBox' : mode, offset: v });
    } });
    const w = this.sec('obj', 'wrap', 'Text Wrap');
    w.body.append(this.wrap.el, this.wrapOffset.el);
  }

  private buildText(): void {
    const c = this.tch, p = this.tpa;
    const s1 = this.sec('text', 'tchar', 'Character');
    s1.body.append(c.family().el, grid(c.style().el, c.size().el), grid(c.leading().el, c.tracking().el), grid(c.kerning().el, c.baseline().el), labelled('Colour', c.fill().el));
    s1.tools.append(iconBtn('pencil', 'Character panel', () => this.ws.showPanel('character'), 'is-sm'));
    const s2 = this.sec('text', 'tpara', 'Paragraph');
    s2.body.append(p.align().el, grid(p.left().el, p.first().el, p.before().el, p.after().el));
    s2.tools.append(iconBtn('pencil', 'Paragraph panel', () => this.ws.showPanel('paragraph'), 'is-sm'));
    const s3 = this.sec('text', 'tstyles', 'Styles');
    s3.body.append(labelled('Paragraph', p.paraStyle().el), labelled('Character', c.charStyle().el));
  }

  private head(s: Snap): void {
    let icon = 'doc', title = s.title || 'Document', sub = '';
    if (s.mode === 'text') {
      icon = 'textFrame';
      title = 'Text';
      const t = s.text;
      const n = t ? Math.abs(t.focus - t.anchor) : 0;
      sub = n ? `${n} ${n === 1 ? 'byte' : 'bytes'} selected` : 'Insertion point';
      if (s.attrs?.paragraphStyle) sub += ` · ${s.attrs.paragraphStyle}`;
    } else if (s.mode === 'objects') {
      const one = s.items.length === 1 ? s.items[0] : null;
      icon = one ? shapeIcon(one) : 'group';
      title = one ? kindName(one) : `${s.items.length} Objects`;
      const names = one?.name ? `${one.name} · ` : '';
      sub = `${names}${s.page ? `Page ${s.page.name}` : ''}`;
    } else if (s.setup) {
      const st = s.setup;
      sub = `${st.pages} ${st.pages === 1 ? 'page' : 'pages'}${st.facingPages ? ' · Facing' : ''} · ${st.intent[0].toUpperCase()}${st.intent.slice(1)}`;
    }
    setHtml(q(this.root, '.fp-tile'), glyph(icon, 18), icon);
    setText(q(this.root, '.fp-head-title'), title);
    setText(q(this.root, '.fp-head-sub'), sub);
  }

  private update(s: Snap): void {
    for (const v of ['doc', 'obj', 'text', 'nodoc']) setHidden(q(this.root, `[data-view="${v}"]`), !(v === 'doc' ? s.mode === 'none' : v === 'obj' ? s.mode === 'objects' : v === 'text' ? s.mode === 'text' : s.mode === 'nodoc'));
    setHidden(q(this.root, '.fp-head'), s.mode === 'nodoc');
    if (s.mode === 'nodoc') return;
    this.head(s);
    if (s.mode === 'none') this.updateDoc(s);
    if (s.mode === 'objects') this.updateObj(s);
    if (s.mode === 'text') { this.tch.update(s); this.tpa.update(s); }
  }

  private updateDoc(s: Snap): void {
    const d = this.doc, st = s.setup, pg = this.curPage();
    d.w.set(st ? st.width : null);
    d.h.set(st ? st.height : null);
    d.pages.set(st ? st.pages : null);
    this.orient.set(st ? (st.width > st.height ? 'landscape' : 'portrait') : null);
    this.facing.set(st ? st.facingPages : null);
    this.intent.set(st ? st.intent : null);
    this.hunits.set(s.units);
    const m = pg?.margins;
    d.mt.set(m ? m.top : null); d.mb.set(m ? m.bottom : null); d.mi.set(m ? m.inside : null); d.mo.set(m ? m.outside : null);
    if (m && !this.marginLink.el.dataset.init) { this.marginLink.el.dataset.init = '1'; this.marginLink.set(m.top === m.bottom && m.top === m.inside && m.top === m.outside); }
    const facing = !!st?.facingPages;
    setText(q(d.mi.el, '.fp-num-txt'), facing ? 'I' : 'L');
    setText(q(d.mo.el, '.fp-num-txt'), facing ? 'O' : 'R');
    d.mi.el.title = facing ? 'Inside margin' : 'Left margin';
    d.mo.el.title = facing ? 'Outside margin' : 'Right margin';
    setText(q(d.bi.el, '.fp-num-txt'), facing ? 'I' : 'L');
    setText(q(d.bo.el, '.fp-num-txt'), facing ? 'O' : 'R');
    d.cols.set(pg ? pg.columns : null);
    d.gutter.set(ctx.recall([-1], 'colGutter') as NumVal);
    const b = st?.bleed;
    d.bt.set(b ? b[0] : null); d.bb.set(b ? b[1] : null); d.bi.set(b ? b[2] : null); d.bo.set(b ? b[3] : null);
  }

  private updateObj(s: Snap): void {
    this.obj.update(s);
    const items = s.items, ids = s.ids;
    const allText = items.every((i) => kindOf(i) === 'text');
    const allGraphic = items.every((i) => kindOf(i) === 'graphic');
    this.secs.get('frame')?.show(allText);
    this.secs.get('otype')?.show(allText);
    this.secs.get('opara')?.show(allText);
    this.secs.get('fitting')?.show(allGraphic);
    this.secs.get('align')?.show(items.length >= 2);
    if (allText) {
      this.frame.cols.set(same(items, (i) => i.columns) as NumVal);
      this.frame.gutter.set(ctx.recall(ids, 'gutter') as NumVal);
      this.frame.inset.set(ctx.recall(ids, 'inset') as NumVal);
      this.vj.set(ctx.recall(ids, 'vj') as string | null);
      this.autoSize.set(ctx.recall(ids, 'autoSize') as string | null);
      this.ch.update(s);
      this.pa.update(s);
    }
    if (allGraphic) {
      this.fit.set(null);
      this.autoFit.set(ctx.recall(ids, 'autoFit') ?? false);
    }
    const wm = ctx.recall(ids, 'wrap') as string | null;
    this.wrap.set(wm);
    this.wrapOffset.set(ctx.recall(ids, 'wrapOffset') as NumVal);
    this.wrapOffset.setDisabled(wm === 'none');
  }

  private async setupDialog(): Promise<void> {
    const cur = await ctx.ask<DocSetup>('layout.documentSetup');
    if (!cur) return;
    const d = new Dialog('Document Setup', 460);
    const vals: Record<string, unknown> = {};
    const num = (key: string, label: string, kind: NumField['spec']['kind'], v: number, min = 0) => {
      const f = new NumField({ label, title: label, kind, min, onCommit: (x) => { vals[key] = x; } });
      f.set(v);
      return f;
    };
    const intent = new Pick('Intent', (v) => { vals.intent = v; }, { cls: 'fp-dd' });
    intent.options(INTENTS);
    intent.set(cur.intent);
    const pages = num('pages', 'Pages', 'int', cur.pages, 1);
    const start = num('startPage', 'Start', 'int', cur.startPage, 1);
    const facing = new Check('Facing pages', (on) => { vals.facingPages = on; });
    facing.set(cur.facingPages);
    const w = num('width', 'W', 'measure', cur.width, 1);
    const h = num('height', 'H', 'measureV', cur.height, 1);
    const bleed = [0, 1, 2, 3].map((i) => {
      const f = new NumField({ label: ['T', 'B', cur.facingPages ? 'I' : 'L', cur.facingPages ? 'O' : 'R'][i], title: 'Bleed', kind: i < 2 ? 'measureV' : 'measure', min: 0, onCommit: (x) => { const b = (vals.bleed as number[] | undefined) ?? [...cur.bleed]; b[i] = Number(x); vals.bleed = b; } });
      f.set(cur.bleed[i] ?? 0);
      return f;
    });
    const slug = [0, 1, 2, 3].map((i) => {
      const f = new NumField({ label: ['T', 'B', cur.facingPages ? 'I' : 'L', cur.facingPages ? 'O' : 'R'][i], title: 'Slug', kind: i < 2 ? 'measureV' : 'measure', min: 0, onCommit: (x) => { const b = (vals.slug as number[] | undefined) ?? [...cur.slug]; b[i] = Number(x); vals.slug = b; } });
      f.set(cur.slug[i] ?? 0);
      return f;
    });
    const adjust = new Check('Adjust layout to the new size', (on) => { vals.adjustLayout = on; });
    const form = el('<div class="fp-form"></div>');
    const sub = (t: string) => { const n = el('<div class="fp-form-sub"></div>'); setText(n, t); return n; };
    form.append(
      labelled('Intent', intent.el),
      grid(pages.el, start.el),
      facing.el,
      sub('Page Size'),
      grid(w.el, h.el),
      sub('Bleed'),
      grid(...bleed.map((f) => f.el)),
      sub('Slug'),
      grid(...slug.map((f) => f.el)),
      adjust.el,
    );
    d.body.append(form);
    d.button('Cancel', 'ghost', () => d.close());
    d.button('OK', 'primary', () => {
      (document.activeElement as HTMLElement | null)?.blur?.();
      d.close();
      if (Object.keys(vals).length) void this.ws.runCore('layout.documentSetup', vals).then(() => ctx.bump());
    });
    d.onClose = () => d.scrim.remove();
    d.open(w.input);
  }
}
