import { confirmAction, Dialog } from '../../ui/dialogs';
import { el, patchList, q, setClass, setDisabled, setHidden, setStyleVar, setText, setTitle } from '../../ui/dom';
import { ctx, type Snap, type SwatchInfo } from './ctx';
import { grid, iconBtn, labelled, NumField, paintChip, same, Seg } from './fields';
import { glyph } from './glyphs';
import { fmtNum } from './units';

type Target = 'fill' | 'stroke';

function textMode(s: Snap, textOn: boolean): boolean { return s.mode === 'text' || (textOn && s.mode === 'objects' && s.kinds.has('text')); }

export class SwatchesPanel {
  readonly root = el(`<div class="fp-panel fp-swp">
<div class="fp-swtop">
<div class="fp-fs" role="group" aria-label="Fill or stroke"><button class="fp-fs-stroke" type="button" title="Stroke: swatches apply to the stroke"><span class="fp-chip"></span></button><button class="fp-fs-fill" type="button" title="Fill: swatches apply to the fill"><span class="fp-chip"></span></button></div>
<div class="fp-swtargets"></div>
<span class="fp-grow"></span>
</div>
<div class="fp-list fp-swlist" role="listbox" aria-label="Swatches"></div>
<div class="fp-foot"><span class="fp-foot-meta"></span><span class="fp-grow"></span></div>
</div>`);
  private target: Target = 'fill';
  private textOn = false;
  private picked = '';
  private readonly tint: NumField;
  private readonly applies: Seg;
  private readonly del: HTMLButtonElement;
  private readonly list: HTMLElement;

  constructor() {
    this.list = q(this.root, '.fp-swlist');
    this.tint = new NumField({ label: 'Tint', title: 'Tint', kind: 'pct', min: 0, max: 100, step: 1, decimals: 0, presets: [100, 90, 80, 70, 60, 50, 40, 30, 20, 10], cls: 'fp-tint', onCommit: () => this.applyTint() });
    this.applies = new Seg([{ id: 'container', icon: 'frameBox', title: 'Formatting affects container' }, { id: 'text', icon: 'textT', title: 'Formatting affects text' }], (id) => { this.textOn = id === 'text'; ctx.schedule(); }, 'is-sm');
    q(this.root, '.fp-swtargets').append(this.applies.el);
    q(this.root, '.fp-swtop').append(this.tint.el);
    q(this.root, '.fp-fs-fill').addEventListener('click', () => this.setTarget('fill'));
    q(this.root, '.fp-fs-stroke').addEventListener('click', () => this.setTarget('stroke'));
    const foot = q(this.root, '.fp-foot');
    this.del = iconBtn('trash', 'Delete swatch', () => void this.remove());
    foot.append(iconBtn('plus', 'New swatch', () => void this.create()), this.del);
    this.list.addEventListener('keydown', (e) => this.key(e));
    this.list.tabIndex = 0;
    ctx.on((s) => this.update(s));
  }

  private setTarget(t: Target): void { this.target = t; this.update(ctx.snap); }

  private current(s: Snap): { name: string | null; tint: number | null } {
    if (textMode(s, this.textOn)) {
      const c = s.attrs?.chars ?? {};
      return this.target === 'fill' ? { name: String(c.fill ?? '[Black]'), tint: Number(c.fillTint ?? 1) } : { name: String(c.stroke ?? '[None]'), tint: Number(c.strokeTint ?? 1) };
    }
    if (s.mode === 'objects') {
      const name = (this.target === 'fill' ? same(s.items, (i) => i.fill) : same(s.items, (i) => i.stroke?.swatch)) as string | null;
      return { name, tint: null };
    }
    return { name: null, tint: null };
  }

  private apply(name: string): void {
    const s = ctx.snap;
    this.picked = name;
    const tv = this.tint.value;
    const tint = typeof tv === 'number' ? tv / 100 : 1;
    if (textMode(s, this.textOn)) {
      const attrs = this.target === 'fill' ? { fill: name, fillTint: tint } : { stroke: name, strokeTint: tint };
      void ctx.run('type.char', { attrs });
    } else if (s.mode === 'objects') {
      void ctx.run(this.target === 'fill' ? 'object.fill' : 'object.stroke', { swatch: name, tint });
    } else this.update(s);
  }

  private applyTint(): void {
    const s = ctx.snap;
    const cur = this.current(s).name ?? this.picked;
    if (!cur || cur === '[None]') return;
    if (s.mode === 'text' || s.mode === 'objects') this.apply(cur);
  }

  private update(s: Snap): void {
    const textCapable = s.mode === 'objects' && s.kinds.has('text');
    setHidden(q(this.root, '.fp-swtargets'), !textCapable);
    this.applies.set(this.textOn ? 'text' : 'container');
    const cur = this.current(s);
    const fsFill = q(this.root, '.fp-fs-fill'), fsStroke = q(this.root, '.fp-fs-stroke');
    setClass(fsFill, 'is-front', this.target === 'fill');
    setClass(fsStroke, 'is-front', this.target === 'stroke');
    const tm = textMode(s, this.textOn);
    const fillName = tm ? String(s.attrs?.chars.fill ?? '[Black]') : s.mode === 'objects' ? (same(s.items, (i) => i.fill) as string | null) : null;
    const strokeName = tm ? String(s.attrs?.chars.stroke ?? '[None]') : s.mode === 'objects' ? (same(s.items, (i) => i.stroke?.swatch) as string | null) : null;
    paintChip(q(fsFill, '.fp-chip'), fillName === null ? null : s.swatch.get(fillName));
    paintChip(q(fsStroke, '.fp-chip'), strokeName === null ? null : s.swatch.get(strokeName));
    setClass(this.root, 'is-text', tm);
    const canApply = s.mode === 'objects' || s.mode === 'text';
    this.tint.setDisabled(!canApply);
    if (tm) this.tint.set(cur.tint !== null ? Math.round(cur.tint * 100) : null);
    else if (!canApply) this.tint.set(100);
    else if (this.tint.value === null) this.tint.set(100);
    patchList<SwatchInfo>(this.list, s.swatches, {
      key: (w) => w.name,
      create: (w) => {
        const n = el(`<div class="fp-swrow" role="option" tabindex="-1"><span class="fp-chip"></span><span class="fp-swname"></span><span class="fp-swmode"></span></div>`);
        n.addEventListener('click', () => this.apply(n.dataset.key as string));
        this.row(n, w, cur.name);
        return n;
      },
      update: (n, w) => this.row(n, w, cur.name),
    });
    setText(q(this.root, '.fp-foot-meta'), s.mode === 'nodoc' ? '' : `${s.swatches.length} swatches`);
    const pk = s.swatch.get(this.picked);
    setDisabled(this.del, !pk || pk.locked || s.mode === 'nodoc');
    setTitle(this.del, pk && !pk.locked ? `Delete ${pk.name}` : 'Delete swatch');
  }

  private row(n: HTMLElement, w: SwatchInfo, applied: string | null): void {
    paintChip(q(n, '.fp-chip'), w);
    setText(q(n, '.fp-swname'), w.name);
    const mode = q(n, '.fp-swmode');
    const key = w.spot ? 'spot' : w.model === 'rgb' ? 'rgb' : w.model === 'cmyk' ? 'process' : w.model === 'lab' ? 'rgb' : '';
    if (mode.dataset.mode !== key) { mode.dataset.mode = key; mode.innerHTML = key ? glyph(key, 14) : ''; }
    const desc = w.spot ? 'Spot colour' : w.model === 'rgb' ? 'Process colour, RGB' : w.model === 'cmyk' ? `Process colour, CMYK ${w.values.map((v) => fmtNum(v * 100, 0)).join(' / ')}` : w.model === 'lab' ? 'Process colour, Lab' : w.css === 'none' ? 'No colour' : w.model === 'special' ? 'Document colour' : 'Gradient or tint';
    setTitle(n, `${w.name}\n${desc}`);
    setClass(n, 'is-applied', applied === w.name);
    setClass(n, 'is-picked', this.picked === w.name);
    setClass(n, 'is-locked', w.locked);
    setStyleVar(n, '--sw', w.css === 'none' || w.css.startsWith('linear') ? 'transparent' : w.css);
  }

  private key(e: KeyboardEvent): void {
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp' && e.key !== 'Enter' && e.key !== 'Delete' && e.key !== 'Backspace') return;
    e.preventDefault();
    e.stopPropagation();
    const names = ctx.snap.swatches.map((w) => w.name);
    let i = names.indexOf(this.picked);
    if (e.key === 'ArrowDown') i = Math.min(names.length - 1, i + 1);
    else if (e.key === 'ArrowUp') i = Math.max(0, i - 1);
    else if (e.key === 'Enter') { if (this.picked) this.apply(this.picked); return; }
    else { void this.remove(); return; }
    this.picked = names[i] ?? '';
    this.update(ctx.snap);
    this.list.querySelector<HTMLElement>(`.fp-swrow.is-picked`)?.scrollIntoView({ block: 'nearest' });
  }

  private async remove(): Promise<void> {
    const sw = ctx.snap.swatch.get(this.picked);
    if (!sw || sw.locked) return;
    const ok = await confirmAction('Delete Swatch', `Delete “${sw.name}”? Objects and text that use it change to [Black].`, 'Delete', true);
    if (!ok) return;
    if (await ctx.run('swatch.delete', { name: sw.name })) this.picked = '';
  }

  private async create(): Promise<void> {
    const base = ctx.snap.swatch.get(this.picked);
    const name = await swatchDialog(base ?? null);
    if (name) { this.picked = name; ctx.schedule(); }
  }
}

function hex(n: number): string { return Math.max(0, Math.min(255, Math.round(n))).toString(16).padStart(2, '0'); }

function rgbOf(css: string): [number, number, number] | null {
  const m = /rgb\((\d+),\s*(\d+),\s*(\d+)\)/.exec(css) ?? null;
  if (m) return [Number(m[1]), Number(m[2]), Number(m[3])];
  const h = /^#([0-9a-f]{6})$/i.exec(css);
  return h ? [parseInt(h[1].slice(0, 2), 16), parseInt(h[1].slice(2, 4), 16), parseInt(h[1].slice(4, 6), 16)] : null;
}

export function swatchDialog(base: SwatchInfo | null): Promise<string | null> {
  return new Promise((resolve) => {
    const d = new Dialog('New Color Swatch', 440);
    let model: 'cmyk' | 'rgb' = base?.model === 'rgb' ? 'rgb' : 'cmyk';
    let spot = false;
    let rgb: [number, number, number] = base && base.css !== 'none' ? (rgbOf(base.css) ?? [249, 115, 22]) : [249, 115, 22];
    const cmyk = base?.model === 'cmyk' ? base.values.map((v) => v * 100) : [0, 54, 91, 2];
    const form = el(`<div class="fp-form fp-swform">
<div class="fp-swpreview"><span class="fp-swbig"></span><div class="fp-swdesc"><div class="fp-swdesc-name"></div><div class="fp-swdesc-sub"></div></div></div>
</div>`);
    const nameIn = el<HTMLInputElement>('<input class="cv-input fp-input" type="text" spellcheck="false" placeholder="Name with colour value">');
    const modelSeg = new Seg([{ id: 'cmyk', text: 'CMYK', title: 'CMYK process colour' }, { id: 'rgb', text: 'RGB', title: 'RGB colour' }], (id) => { model = id as 'cmyk' | 'rgb'; sync(); }, 'is-text');
    const typeSeg = new Seg([{ id: 'process', text: 'Process', title: 'Process colour' }, { id: 'spot', text: 'Spot', title: 'Spot colour (its own ink)' }], (id) => { spot = id === 'spot'; sync(); }, 'is-text');
    const fields = ['C', 'M', 'Y', 'K'].map((l, i) => {
      const f = new NumField({ label: l, title: ['Cyan', 'Magenta', 'Yellow', 'Black'][i], kind: 'pct', min: 0, max: 100, step: 1, decimals: 1, onCommit: (v) => { cmyk[i] = Number(v); sync(); } });
      f.set(Math.round(cmyk[i] * 10) / 10);
      return f;
    });
    const picker = el<HTMLInputElement>('<input class="fp-colorin" type="color" aria-label="Colour">');
    const hexIn = el<HTMLInputElement>('<input class="cv-input fp-input fp-hex" type="text" spellcheck="false" maxlength="7" aria-label="Hex value">');
    picker.addEventListener('input', () => { const c = rgbOf(picker.value); if (c) { rgb = c; sync(); } });
    hexIn.addEventListener('change', () => { const v = hexIn.value.trim(); const c = rgbOf(v.startsWith('#') ? v : `#${v}`); if (c) rgb = c; sync(); });
    hexIn.addEventListener('keydown', (e) => e.stopPropagation());
    const cmykRow = grid(...fields.map((f) => f.el));
    const rgbRow = el('<div class="fp-row fp-rgbrow"></div>');
    rgbRow.append(picker, hexIn);
    form.append(labelled('Name', nameIn), labelled('Mode', modelSeg.el), labelled('Type', typeSeg.el), cmykRow, rgbRow);
    d.body.append(form);
    const big = q(form, '.fp-swbig');
    const auto = (): string => model === 'cmyk' ? `C=${fmtNum(cmyk[0], 0)} M=${fmtNum(cmyk[1], 0)} Y=${fmtNum(cmyk[2], 0)} K=${fmtNum(cmyk[3], 0)}` : `R=${rgb[0]} G=${rgb[1]} B=${rgb[2]}`;
    const css = (): string => {
      if (model === 'rgb') return `rgb(${rgb[0]}, ${rgb[1]}, ${rgb[2]})`;
      const [c, m, y, k] = cmyk.map((v) => v / 100);
      const ch = (x: number) => Math.round(255 * (1 - x) * (1 - k));
      return `rgb(${ch(c)}, ${ch(m)}, ${ch(y)})`;
    };
    const sync = () => {
      modelSeg.set(model);
      typeSeg.set(spot ? 'spot' : 'process');
      setHidden(cmykRow, model !== 'cmyk');
      setHidden(rgbRow, model !== 'rgb');
      const h = `#${hex(rgb[0])}${hex(rgb[1])}${hex(rgb[2])}`;
      if (picker.value !== h) picker.value = h;
      if (document.activeElement !== hexIn) hexIn.value = h;
      setStyleVar(big, '--chip', css());
      setText(q(form, '.fp-swdesc-name'), nameIn.value.trim() || auto());
      setText(q(form, '.fp-swdesc-sub'), `${spot ? 'Spot' : 'Process'} · ${model === 'cmyk' ? 'CMYK' : 'RGB'}`);
      nameIn.placeholder = auto();
    };
    nameIn.addEventListener('input', sync);
    nameIn.addEventListener('keydown', (e) => { e.stopPropagation(); if (e.key === 'Enter') add.click(); });
    let done = false;
    const finish = (v: string | null) => { if (done) return; done = true; resolve(v); };
    d.button('Cancel', 'ghost', () => { d.close(); });
    const add = d.button('Add', 'primary', async () => {
      (document.activeElement as HTMLElement | null)?.blur?.();
      const name = nameIn.value.trim() || auto();
      const color = model === 'cmyk' ? { c: cmyk[0], m: cmyk[1], y: cmyk[2], k: cmyk[3] } : `#${hex(rgb[0])}${hex(rgb[1])}${hex(rgb[2])}`;
      const ok = await ctx.run('swatch.create', { name, color, spot });
      if (ok) { finish(name); d.close(); }
    });
    d.onClose = () => { d.scrim.remove(); finish(null); };
    sync();
    d.open(nameIn);
  });
}
