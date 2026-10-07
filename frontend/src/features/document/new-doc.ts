import { api } from '../../engine/client';
import { editor, friendly } from '../../editor/editor';
import { Dialog } from '../../ui/dialogs';
import { el, patchList, q, setClass, setDisabled, setText } from '../../ui/dom';
import { toast } from '../../ui/toast';
import type { Workspace } from '../../ui/workspace';
import { Check, field, IntField, LengthField, Pick, row, Seg, setLabel } from './form';
import { docIcon } from './icons';
import { CORE_UNITS, fmt, savedUnits, UNIT_NAMES, UNITS, type Unit } from './units';

type Intent = 'print' | 'web' | 'mobile';

interface Preset { id: string; name: string; intent: Intent; w: number; h: number; unit: Unit; margin: number; bleed: number; facing?: boolean; columns?: number; gutter?: number }

const IN = 72, MM = 72 / 25.4;
const PRESETS: Preset[] = [
  { id: 'letter', name: 'Letter', intent: 'print', w: 612, h: 792, unit: 'in', margin: 36, bleed: 9 },
  { id: 'legal', name: 'Legal', intent: 'print', w: 612, h: 1008, unit: 'in', margin: 36, bleed: 9 },
  { id: 'tabloid', name: 'Tabloid', intent: 'print', w: 792, h: 1224, unit: 'in', margin: 36, bleed: 9, columns: 3 },
  { id: 'a3', name: 'A3', intent: 'print', w: 297 * MM, h: 420 * MM, unit: 'mm', margin: 15 * MM, bleed: 3 * MM, columns: 3 },
  { id: 'a4', name: 'A4', intent: 'print', w: 210 * MM, h: 297 * MM, unit: 'mm', margin: 12.7 * MM, bleed: 3 * MM },
  { id: 'a5', name: 'A5', intent: 'print', w: 148 * MM, h: 210 * MM, unit: 'mm', margin: 12 * MM, bleed: 3 * MM, facing: true },
  { id: 'trade', name: 'Trade 6 × 9', intent: 'print', w: 6 * IN, h: 9 * IN, unit: 'in', margin: 0.625 * IN, bleed: 9, facing: true },
  { id: 'digest', name: 'Digest 5.5 × 8.5', intent: 'print', w: 5.5 * IN, h: 8.5 * IN, unit: 'in', margin: 0.5 * IN, bleed: 9, facing: true },
  { id: 'web-1440', name: 'Desktop 1440', intent: 'web', w: 1440, h: 900, unit: 'px', margin: 80, bleed: 0, columns: 12, gutter: 24 },
  { id: 'web-1920', name: 'Full HD 1920', intent: 'web', w: 1920, h: 1080, unit: 'px', margin: 120, bleed: 0, columns: 12, gutter: 24 },
  { id: 'web-1366', name: 'Laptop 1366', intent: 'web', w: 1366, h: 768, unit: 'px', margin: 64, bleed: 0, columns: 12, gutter: 20 },
  { id: 'leaderboard', name: 'Leaderboard', intent: 'web', w: 728, h: 90, unit: 'px', margin: 10, bleed: 0 },
  { id: 'billboard', name: 'Billboard', intent: 'web', w: 970, h: 250, unit: 'px', margin: 16, bleed: 0 },
  { id: 'rectangle', name: 'Medium Rectangle', intent: 'web', w: 300, h: 250, unit: 'px', margin: 12, bleed: 0 },
  { id: 'skyscraper', name: 'Wide Skyscraper', intent: 'web', w: 160, h: 600, unit: 'px', margin: 10, bleed: 0 },
  { id: 'square', name: 'Square Post', intent: 'mobile', w: 1080, h: 1080, unit: 'px', margin: 64, bleed: 0 },
  { id: 'portrait', name: 'Portrait Post', intent: 'mobile', w: 1080, h: 1350, unit: 'px', margin: 64, bleed: 0 },
  { id: 'story', name: 'Story', intent: 'mobile', w: 1080, h: 1920, unit: 'px', margin: 72, bleed: 0 },
  { id: 'phone', name: 'Phone 390', intent: 'mobile', w: 390, h: 844, unit: 'px', margin: 20, bleed: 0, columns: 4, gutter: 16 },
  { id: 'tablet', name: 'Tablet 1024', intent: 'mobile', w: 1024, h: 1366, unit: 'px', margin: 48, bleed: 0, columns: 8, gutter: 20 },
  { id: 'mobile-banner', name: 'Mobile Banner', intent: 'mobile', w: 320, h: 100, unit: 'px', margin: 8, bleed: 0 },
];

const INTENTS: Array<{ id: Intent; label: string; icon: string }> = [
  { id: 'print', label: 'Print', icon: 'print' },
  { id: 'web', label: 'Web', icon: 'web' },
  { id: 'mobile', label: 'Mobile', icon: 'mobile' },
];

const SVG_NS = 'http://www.w3.org/2000/svg';
const PV_W = 560, PV_H = 200;

function svgEl(tag: string, cls: string): SVGElement {
  const n = document.createElementNS(SVG_NS, tag);
  n.setAttribute('class', cls);
  return n;
}

function setAttrs(n: Element, attrs: Record<string, number | string>): void {
  for (const [k, v] of Object.entries(attrs)) {
    const s = typeof v === 'number' ? String(Math.round(v * 100) / 100) : v;
    if (n.getAttribute(k) !== s) n.setAttribute(k, s);
  }
}

export class NewDocumentDialog {
  private readonly d = new Dialog('New Document', 1080);
  private intent: Intent = 'print';
  private preset: string | null = 'letter';
  private unit: Unit = 'in';
  private linked = true;
  private readonly cats = el('<nav class="cv-nd-cats fd-nd-cats" aria-label="Intent"></nav>');
  private readonly cards = el('<div class="cv-nd-cards fd-nd-cards"></div>');
  private readonly name = el<HTMLInputElement>('<input class="fd-f-text" type="text" spellcheck="false" autocomplete="off">');
  private readonly width = new LengthField('in', 1, 15552);
  private readonly height = new LengthField('in', 1, 15552);
  private readonly units = new Pick<Unit>(UNITS.map((u) => ({ value: u, label: u })), 'in', 'Units');
  private readonly orient = new Seg<'portrait' | 'landscape'>([
    { value: 'portrait', icon: docIcon('portrait', 16), title: 'Portrait' },
    { value: 'landscape', icon: docIcon('landscape', 16), title: 'Landscape' },
  ], 'portrait');
  private readonly pages = new IntField(1, 9999);
  private readonly start = new IntField(1, 99999);
  private readonly facing = new Check('Facing pages');
  private readonly columns = new IntField(1, 40);
  private readonly gutter = new LengthField('in', 0, 1000);
  private readonly margins = { top: new LengthField('in', 0, 5000), bottom: new LengthField('in', 0, 5000), inside: new LengthField('in', 0, 5000), outside: new LengthField('in', 0, 5000) };
  private readonly bleed = new LengthField('in', 0, 432);
  private readonly linkBtn = el<HTMLButtonElement>(`<button class="fd-nd-link is-on" type="button" title="Make all margins the same" aria-pressed="true"><span class="fd-nd-link-on">${docIcon('chain', 15)}</span><span class="fd-nd-link-off">${docIcon('chainOff', 15)}</span></button>`);
  private readonly insideField: HTMLElement;
  private readonly outsideField: HTMLElement;
  private readonly summary = el('<div class="fd-nd-summary"><div class="fd-nd-size"><span class="fd-nd-dim"></span> <span class="fd-nd-unit"></span></div><div class="fd-nd-meta"></div></div>');
  private readonly svg: SVGSVGElement;
  private readonly pv: Array<{ g: SVGElement; bleed: SVGElement; page: SVGElement; margin: SVGElement; cols: SVGElement }> = [];
  private readonly spine: SVGElement;
  private readonly create: HTMLButtonElement;
  private busy = false;

  constructor(private readonly ws: Workspace) {
    const dlg = q<HTMLElement>(this.d.scrim, '.cv-dialog');
    dlg.classList.add('cv-newdoc', 'fd-newdoc');
    this.insideField = field('Inside', this.margins.inside.root);
    this.outsideField = field('Outside', this.margins.outside.root);
    this.svg = document.createElementNS(SVG_NS, 'svg') as SVGSVGElement;
    this.svg.setAttribute('class', 'fd-nd-svg');
    this.svg.setAttribute('viewBox', `0 0 ${PV_W} ${PV_H}`);
    this.svg.setAttribute('aria-hidden', 'true');
    for (let i = 0; i < 2; i++) {
      const g = svgEl('g', 'fd-pv-sheet');
      const bleed = svgEl('rect', 'fd-pv-bleed'), page = svgEl('rect', 'fd-pv-page'), margin = svgEl('rect', 'fd-pv-margin'), cols = svgEl('path', 'fd-pv-cols');
      g.append(bleed, page, cols, margin);
      this.svg.appendChild(g);
      this.pv.push({ g, bleed, page, margin, cols });
    }
    this.spine = svgEl('line', 'fd-pv-spine');
    this.svg.appendChild(this.spine);

    for (const it of INTENTS) {
      const b = el<HTMLButtonElement>(`<button class="cv-nd-cat fd-nd-cat" type="button">${docIcon(it.icon, 16)}<span class="fd-nd-catname"></span></button>`);
      setText(q(b, '.fd-nd-catname'), it.label);
      b.dataset.intent = it.id;
      b.addEventListener('click', () => this.pickIntent(it.id));
      this.cats.appendChild(b);
    }
    const browse = el<HTMLButtonElement>(`<button class="fd-nd-browse" type="button">${docIcon('templates', 16)}<span>Browse templates…</span></button>`);
    browse.addEventListener('click', () => { this.d.close(); this.ws.commands.run('ui.templates'); });
    this.cats.append(el('<span class="fd-nd-gap"></span>'), browse);

    const mid = el('<div class="cv-nd-mid fd-nd-mid"><div class="fd-nd-scroll"></div><div class="fd-nd-preview"></div></div>');
    q(mid, '.fd-nd-scroll').appendChild(this.cards);
    q(mid, '.fd-nd-preview').append(this.svg, this.summary);

    const side = el('<div class="cv-nd-side fd-nd-side"></div>');
    const unitsField = field('Units', this.units.root);
    this.units.select.title = 'Units';
    const marginHead = el('<div class="fd-nd-mhead"><span class="fd-f-label">Margins</span></div>');
    marginHead.appendChild(this.linkBtn);
    side.append(
      field('Name', this.name),
      el('<div class="fd-nd-dims"></div>'),
      row(field('Orientation', this.orient.root), field('Pages', this.pages.root)),
      row(field('Start page', this.start.root), el('<div class="fd-f-field fd-nd-facing"></div>')),
      row(field('Columns', this.columns.root), field('Gutter', this.gutter.root)),
      el('<div class="fd-nd-margins"></div>'),
      row(field('Bleed', this.bleed.root), el('<div class="fd-f-field"></div>')),
    );
    q(side, '.fd-nd-dims').append(field('Width', this.width.root), field('Height', this.height.root), unitsField);
    q(side, '.fd-nd-facing').appendChild(this.facing.root);
    const mg = q(side, '.fd-nd-margins');
    mg.append(marginHead, el('<div class="fd-nd-mgrid"></div>'));
    q(mg, '.fd-nd-mgrid').append(field('Top', this.margins.top.root), field('Bottom', this.margins.bottom.root), this.insideField, this.outsideField);

    const grid = el('<div class="cv-nd-grid fd-nd-grid"></div>');
    grid.append(this.cats, mid, side);
    this.d.body.appendChild(grid);

    const cancel = this.d.button('Cancel', 'ghost', () => this.d.close());
    cancel.classList.add('fd-nd-cancel');
    this.create = this.d.button('Create', 'primary', () => void this.submit());
    this.d.onClose = () => undefined;
    this.d.scrim.addEventListener('keydown', (e) => {
      const t = e.target as HTMLElement;
      if (e.key === 'Enter' && t.tagName === 'INPUT') { e.preventDefault(); window.setTimeout(() => this.create.click(), 0); }
    });

    this.wire();
    this.loadPreset(PRESETS[0]);
  }

  open(): void {
    const n = editor.docs.length + 1;
    this.name.value = `Untitled-${n}`;
    this.busy = false;
    setDisabled(this.create, false);
    setText(this.create, 'Create');
    this.d.open(this.name);
    this.name.select();
  }

  private wire(): void {
    const changed = () => { this.matchPreset(); this.render(); };
    this.width.onChange = () => { this.syncOrient(); changed(); };
    this.height.onChange = () => { this.syncOrient(); changed(); };
    this.units.onChange = (u) => this.setUnit(u);
    this.orient.onChange = (o) => {
      const w = this.width.value, h = this.height.value;
      if ((o === 'landscape') !== (w > h) && w !== h) { this.width.set(h); this.height.set(w); }
      changed();
    };
    this.pages.onChange = changed;
    this.start.onChange = changed;
    this.facing.onChange = changed;
    this.columns.onChange = changed;
    this.gutter.onChange = changed;
    this.bleed.onChange = changed;
    for (const k of ['top', 'bottom', 'inside', 'outside'] as const) {
      this.margins[k].onChange = (pt) => {
        if (this.linked) for (const o of Object.values(this.margins)) if (o !== this.margins[k]) o.set(pt);
        changed();
      };
    }
    this.linkBtn.addEventListener('click', () => {
      this.linked = !this.linked;
      setClass(this.linkBtn, 'is-on', this.linked);
      this.linkBtn.setAttribute('aria-pressed', String(this.linked));
      if (this.linked) { const v = this.margins.top.value; for (const o of Object.values(this.margins)) o.set(v); changed(); }
    });
  }

  private pickIntent(intent: Intent): void {
    if (intent === this.intent) return;
    this.intent = intent;
    this.cards.parentElement?.scrollTo({ top: 0 });
    const first = PRESETS.find((p) => p.intent === intent);
    if (first) this.loadPreset(first);
  }

  private loadPreset(p: Preset): void {
    this.intent = p.intent;
    this.preset = p.id;
    const unit = p.intent === 'print' ? savedUnits() ?? p.unit : 'px';
    this.width.set(p.w);
    this.height.set(p.h);
    this.pages.set(p.facing ? 2 : 1);
    this.start.set(1);
    this.facing.set(!!p.facing);
    this.columns.set(p.columns ?? 1);
    this.gutter.set(p.gutter ?? (p.intent === 'print' ? 12 : 20));
    for (const m of Object.values(this.margins)) m.set(p.margin);
    if (!this.linked) this.linkBtn.click();
    this.bleed.set(p.bleed);
    this.setUnit(unit);
    this.syncOrient();
    this.render();
  }

  private setUnit(u: Unit): void {
    this.unit = u;
    this.units.set(u);
    for (const f of [this.width, this.height, this.gutter, this.bleed, ...Object.values(this.margins)]) f.setUnit(u);
    this.render();
  }

  private syncOrient(): void { this.orient.set(this.width.value > this.height.value ? 'landscape' : 'portrait'); }

  private matchPreset(): void {
    const w = this.width.value, h = this.height.value;
    const hit = PRESETS.find((p) => p.intent === this.intent && ((Math.abs(p.w - w) < 0.6 && Math.abs(p.h - h) < 0.6) || (Math.abs(p.w - h) < 0.6 && Math.abs(p.h - w) < 0.6)));
    this.preset = hit?.id ?? null;
  }

  private render(): void {
    for (const b of Array.from(this.cats.querySelectorAll<HTMLElement>('.fd-nd-cat'))) setClass(b, 'is-on', b.dataset.intent === this.intent);
    const updateCard = (c: HTMLElement, p: Preset): void => {
      const u = p.intent === 'print' ? this.unit : 'px';
      setText(q(c, '.cv-nd-spec'), `${fmt(p.w, u)} × ${fmt(p.h, u)} ${u}`);
      setClass(c, 'is-on', p.id === this.preset);
    };
    patchList<Preset>(this.cards, PRESETS.filter((p) => p.intent === this.intent), {
      key: (p) => p.id,
      create: (p) => {
        const c = el<HTMLButtonElement>('<button class="cv-nd-card fd-nd-card" type="button"><span class="cv-nd-thumb"><span class="cv-nd-page"></span></span><span class="cv-nd-name"></span><span class="cv-nd-spec"></span></button>');
        const s = Math.min(112 / p.w, 68 / p.h);
        const pg = q<HTMLElement>(c, '.cv-nd-page');
        pg.style.width = `${Math.max(6, Math.round(p.w * s))}px`;
        pg.style.height = `${Math.max(6, Math.round(p.h * s))}px`;
        setText(q(c, '.cv-nd-name'), p.name);
        c.addEventListener('click', () => this.loadPreset(p));
        updateCard(c, p);
        return c;
      },
      update: updateCard,
    });
    const facing = this.facing.value;
    setLabel(this.insideField, facing ? 'Inside' : 'Left');
    setLabel(this.outsideField, facing ? 'Outside' : 'Right');
    this.gutter.setDisabled(this.columns.value < 2);
    this.drawPreview();
    const p = PRESETS.find((x) => x.id === this.preset);
    const u = this.unit;
    setText(q(this.summary, '.fd-nd-dim'), `${fmt(this.width.value, u)} × ${fmt(this.height.value, u)}`);
    setText(q(this.summary, '.fd-nd-unit'), UNIT_NAMES[u].toLowerCase());
    const n = this.pages.value;
    const bits = [p ? p.name : 'Custom', `${n} ${n === 1 ? 'page' : 'pages'}${facing ? ', facing' : ''}`];
    if (this.columns.value > 1) bits.push(`${this.columns.value} columns`);
    if (this.bleed.value > 0) bits.push(`${fmt(this.bleed.value, u)} ${u} bleed`);
    bits.push(INTENTS.find((i) => i.id === this.intent)?.label ?? '');
    setText(q(this.summary, '.fd-nd-meta'), bits.join(' · '));
  }

  private drawPreview(): void {
    const w = this.width.value, h = this.height.value, b = this.bleed.value;
    const spread = this.facing.value && this.pages.value >= 2;
    const sheets = spread ? 2 : 1;
    const totalW = w * sheets + b * 2, totalH = h + b * 2;
    const pad = 18;
    const s = Math.min((PV_W - pad * 2) / totalW, (PV_H - pad * 2) / totalH);
    const x0 = (PV_W - w * sheets * s) / 2, y0 = (PV_H - h * s) / 2;
    const m = { top: this.margins.top.value, bottom: this.margins.bottom.value, inside: this.margins.inside.value, outside: this.margins.outside.value };
    const cols = this.columns.value, gut = this.gutter.value;
    this.pv.forEach((v, i) => {
      const on = i < sheets;
      v.g.style.display = on ? '' : 'none';
      if (!on) return;
      const px = x0 + i * w * s;
      const left = spread && i === 0;
      const ml = left ? m.outside : m.inside, mr = left ? m.inside : m.outside;
      const bl = spread && i === 1 ? 0 : b, br = spread && i === 0 ? 0 : b;
      setAttrs(v.bleed, { x: px - bl * s, y: y0 - b * s, width: (w + bl + br) * s, height: (h + b * 2) * s });
      v.bleed.style.display = b > 0 ? '' : 'none';
      setAttrs(v.page, { x: px, y: y0, width: w * s, height: h * s });
      const mx = px + ml * s, my = y0 + m.top * s, mw = Math.max(0, (w - ml - mr) * s), mh = Math.max(0, (h - m.top - m.bottom) * s);
      setAttrs(v.margin, { x: mx, y: my, width: mw, height: mh });
      let d = '';
      if (cols > 1 && mw > 0) {
        const cw = (mw - (cols - 1) * gut * s) / cols;
        for (let k = 1; k < cols; k++) {
          const a = mx + k * cw + (k - 1) * gut * s;
          d += `M${a.toFixed(2)} ${my.toFixed(2)}V${(my + mh).toFixed(2)}`;
          if (gut > 0) d += `M${(a + gut * s).toFixed(2)} ${my.toFixed(2)}V${(my + mh).toFixed(2)}`;
        }
      }
      if (v.cols.getAttribute('d') !== d) v.cols.setAttribute('d', d);
    });
    this.spine.style.display = spread ? '' : 'none';
    setAttrs(this.spine, { x1: x0 + w * s, y1: y0, x2: x0 + w * s, y2: y0 + h * s });
  }

  private async submit(): Promise<void> {
    if (this.busy) return;
    for (const inp of Array.from(this.d.body.querySelectorAll('input'))) inp.dispatchEvent(new Event('change'));
    this.busy = true;
    setDisabled(this.create, true);
    const label = window.setTimeout(() => setText(this.create, 'Creating…'), 300);
    const facing = this.facing.value;
    const params = {
      width: this.width.value, height: this.height.value, pages: this.pages.value, facingPages: facing,
      columns: this.columns.value, gutter: this.gutter.value,
      margins: { top: this.margins.top.value, bottom: this.margins.bottom.value, inside: this.margins.inside.value, outside: this.margins.outside.value },
      bleed: this.bleed.value, title: this.name.value.trim() || 'Untitled',
    };
    try {
      await api('/documents/new', { json: params });
      const setup: Record<string, unknown> = { intent: this.intent };
      if (this.start.value !== 1) setup.startPage = this.start.value;
      await editor.batch([
        { command: 'layout.documentSetup', params: setup },
        { command: 'document.preferences', params: { horizontalUnits: CORE_UNITS[this.unit], verticalUnits: CORE_UNITS[this.unit] } },
      ], true);
      editor.refreshMeta();
      this.d.close();
    } catch (err) {
      toast(friendly((err as Error).message), 'error');
    } finally {
      window.clearTimeout(label);
      this.busy = false;
      setDisabled(this.create, false);
      setText(this.create, 'Create');
    }
  }

}
