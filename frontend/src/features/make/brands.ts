import { api, engine } from '../../engine/client';
import { friendly } from '../../editor/editor';
import { confirmAction, Dialog } from '../../ui/dialogs';
import { delayedIndicator, el, patchList, q, setClass, setDisabled, setHidden, setText, setTitle, setValue, swapMedia } from '../../ui/dom';
import { enhanceSelect } from '../../ui/dropdown';
import { icon } from '../../ui/icons';
import { toast } from '../../ui/toast';
import type { Workspace } from '../../ui/workspace';
import { brands, brandsChanged, setRatio, templates, thumbUrl, type Brand, type FontRole, type PaletteRole } from './api';
import { glyph } from './icons';

const ROLES: Array<[PaletteRole, string, string]> = [
  ['accent', 'Accent', 'Highlights, rules, buttons'],
  ['accent2', 'Accent 2', 'A second highlight'],
  ['ink', 'Ink', 'Body text'],
  ['paper', 'Paper', 'Page background'],
  ['dark', 'Dark', 'Dark panels and type on light'],
  ['light', 'Light', 'Light panels and type on dark'],
  ['muted', 'Muted', 'Captions, folios, metadata'],
  ['tint', 'Tint', 'Soft fills behind boxes'],
];
const FONT_ROLES: Array<[FontRole, string]> = [['display', 'Display'], ['text', 'Text'], ['accent', 'Accent'], ['mono', 'Mono']];
const STYLES = ['Regular', 'Italic', 'Light', 'Medium', 'SemiBold', 'Bold', 'Bold Italic', 'ExtraBold', 'Black'];
const NEW_KEY = '__new';
const HEX = /^#[0-9a-f]{6}$/i;
const PREVIEW_DEFAULT = 'magazine-feature-spread';

interface Draft { name: string; palette: Partial<Record<PaletteRole, string>>; fonts: Partial<Record<FontRole, [string, string]>>; logo: string; voice: string }

function blank(): Draft {
  return { name: '', palette: { accent: '#e4572e', ink: '#1d1d1f', paper: '#faf8f3' }, fonts: {}, logo: '', voice: '' };
}
function fromBrand(b: Brand): Draft {
  return { name: b.name, palette: { ...(b.palette ?? {}) }, fonts: { ...(b.fonts ?? {}) } as Draft['fonts'], logo: b.logo ?? '', voice: b.voice ?? '' };
}

export class BrandKits {
  private readonly d: Dialog;
  private list: Brand[] = [];
  private current = '';
  private draft: Draft = blank();
  private saved = '';
  private dirty = false;
  private families: string[] = [];
  private readonly nameIn: HTMLInputElement;
  private readonly voice: HTMLTextAreaElement;
  private readonly pvSel: HTMLSelectElement;
  private readonly saveBtn: HTMLButtonElement;
  private readonly delBtn: HTMLButtonElement;
  private readonly status: HTMLElement;
  private readonly fontSels = new Map<FontRole, { fam: HTMLSelectElement; sty: HTMLSelectElement }>();

  constructor(private readonly ws: Workspace) {
    this.d = new Dialog('Brand Kits', 900);
    q(this.d.scrim, '.cv-dialog').classList.add('mk-bk-dialog');
    this.d.body.append(el(`<div class="mk-bk">
      <aside class="mk-bk-side">
        <div class="mk-bk-list" role="listbox" aria-label="Brand kits"></div>
        <button class="cv-btn mk-bk-new" type="button">${icon('add', 14)}<span>New brand kit</span></button>
      </aside>
      <div class="mk-bk-form">
        <label class="mk-bk-name-row"><span class="mk-bk-sec">Name</span><input class="cv-search-input mk-bk-name" type="text" spellcheck="false" placeholder="e.g. Northlight"></label>
        <div class="mk-bk-sec">Colours<span class="mk-bk-sec-note">Empty roles keep the template's colour</span></div>
        <div class="mk-bk-pal"></div>
        <div class="mk-bk-sec">Fonts<span class="mk-bk-sec-note">Default keeps the template's font</span></div>
        <div class="mk-bk-fonts"></div>
        <div class="mk-bk-sec">Logo</div>
        <div class="mk-bk-logo"><span class="mk-bk-logo-ico">${glyph('image', 16)}</span><span class="mk-bk-logo-name"></span><button class="cv-btn mk-bk-logo-pick" type="button">Choose…</button><button class="cv-icon-btn mk-bk-logo-x" type="button" title="Remove the logo" aria-label="Remove the logo">${icon('close', 14)}</button></div>
        <div class="mk-bk-sec">Voice</div>
        <textarea class="cv-search-input mk-bk-voice" rows="2" placeholder="How it sounds: warm, plain-spoken, short sentences, no jargon"></textarea>
      </div>
      <div class="mk-bk-pv">
        <div class="mk-bk-sec">Preview</div>
        <div class="mk-bk-pv-box"><div class="mk-bk-pv-page"><img alt="" decoding="async"></div></div>
        <p class="mk-bk-pv-note"></p>
        <select class="mk-bk-pv-sel" aria-label="Preview template"></select>
      </div>
    </div>`));
    this.nameIn = q<HTMLInputElement>(this.d.body, '.mk-bk-name');
    this.voice = q<HTMLTextAreaElement>(this.d.body, '.mk-bk-voice');
    this.pvSel = q<HTMLSelectElement>(this.d.body, '.mk-bk-pv-sel');
    this.delBtn = this.d.button('Delete', 'danger', () => void this.remove());
    this.status = el('<span class="mk-gal-status" aria-live="polite"></span>');
    this.d.foot.append(this.status);
    this.d.button('Close', 'ghost', () => this.d.close());
    this.saveBtn = this.d.button('Save', 'primary', () => void this.save());
    this.buildPalette();
    this.buildFonts();
    const opt = document.createElement('option');
    opt.value = PREVIEW_DEFAULT;
    opt.textContent = 'Feature Spread';
    this.pvSel.appendChild(opt);
    enhanceSelect(this.pvSel, { search: 'Search templates' });
    this.wire();
  }

  private wire(): void {
    q(this.d.body, '.mk-bk-new').addEventListener('click', () => void this.startNew());
    this.nameIn.addEventListener('input', () => { this.draft.name = this.nameIn.value; this.touch(); });
    this.voice.addEventListener('input', () => { this.draft.voice = this.voice.value; this.touch(); });
    q(this.d.body, '.mk-bk-logo-pick').addEventListener('click', () => void this.pickLogo());
    q(this.d.body, '.mk-bk-logo-x').addEventListener('click', () => { this.draft.logo = ''; this.touch(); this.syncForm(); });
    this.pvSel.addEventListener('change', () => this.preview());
    this.nameIn.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); void this.save(); } });
  }

  private buildPalette(): void {
    const host = q(this.d.body, '.mk-bk-pal');
    for (const [role, label, hint] of ROLES) {
      const row = el(`<div class="mk-bk-col"><label class="mk-bk-sw"><input type="color" tabindex="-1"></label><span class="mk-bk-role"><span class="mk-bk-role-name"></span><span class="mk-bk-role-hint"></span></span><input class="mk-bk-hex" type="text" spellcheck="false" maxlength="7" placeholder="Template"></div>`);
      row.dataset.role = role;
      setText(q(row, '.mk-bk-role-name'), label);
      setText(q(row, '.mk-bk-role-hint'), hint);
      setTitle(q(row, '.mk-bk-sw'), `Pick the ${label.toLowerCase()} colour`);
      const pick = q<HTMLInputElement>(row, 'input[type="color"]');
      const hex = q<HTMLInputElement>(row, '.mk-bk-hex');
      pick.addEventListener('input', () => { this.draft.palette[role] = pick.value.toLowerCase(); hex.value = pick.value.toLowerCase(); this.touch(); this.syncSwatch(row); });
      hex.addEventListener('input', () => {
        let v = hex.value.trim();
        if (v && !v.startsWith('#')) v = `#${v}`;
        if (!v) delete this.draft.palette[role];
        else if (HEX.test(v)) this.draft.palette[role] = v.toLowerCase();
        else return;
        this.touch();
        this.syncSwatch(row);
      });
      hex.addEventListener('blur', () => { hex.value = this.draft.palette[role] ?? ''; });
      host.appendChild(row);
    }
  }

  private syncSwatch(row: HTMLElement): void {
    const role = row.dataset.role as PaletteRole;
    const v = this.draft.palette[role];
    const sw = q(row, '.mk-bk-sw');
    setClass(sw, 'is-unset', !v);
    sw.style.setProperty('--sw', v ?? 'transparent');
    const pick = q<HTMLInputElement>(row, 'input[type="color"]');
    if (v && pick.value !== v) pick.value = v;
    setValue(q<HTMLInputElement>(row, '.mk-bk-hex'), v ?? '');
  }

  private buildFonts(): void {
    const host = q(this.d.body, '.mk-bk-fonts');
    for (const [role, label] of FONT_ROLES) {
      const row = el(`<div class="mk-bk-font"><span class="mk-bk-font-role"></span><select class="mk-bk-fam" aria-label="Family"><option value="">Default</option></select><select class="mk-bk-sty" aria-label="Style"></select></div>`);
      setText(q(row, '.mk-bk-font-role'), label);
      const fam = q<HTMLSelectElement>(row, '.mk-bk-fam');
      const sty = q<HTMLSelectElement>(row, '.mk-bk-sty');
      for (const s of STYLES) { const o = document.createElement('option'); o.value = s; o.textContent = s; sty.appendChild(o); }
      fam.addEventListener('change', () => {
        if (fam.value) this.draft.fonts[role] = [fam.value, sty.value || (role === 'display' ? 'Bold' : 'Regular')];
        else delete this.draft.fonts[role];
        this.touch();
        this.syncForm();
      });
      sty.addEventListener('change', () => { const f = this.draft.fonts[role]; if (f) { this.draft.fonts[role] = [f[0], sty.value]; this.touch(); } });
      host.appendChild(row);
      enhanceSelect(fam, { search: 'Search fonts' });
      enhanceSelect(sty);
      this.fontSels.set(role, { fam, sty });
    }
  }

  private async loadFamilies(): Promise<void> {
    if (this.families.length) return;
    try {
      const r = await engine.call<string[]>('fonts.families', {});
      this.families = (r.result ?? []).filter((f) => !/^(Unifont|OpenSymbol|Noto Color Emoji|DejaVu Math)/.test(f));
    } catch { this.families = []; }
    for (const { fam } of this.fontSels.values()) {
      for (const f of this.families) { const o = document.createElement('option'); o.value = f; o.textContent = f; fam.appendChild(o); }
    }
  }

  private async loadTemplates(): Promise<void> {
    if (this.pvSel.options.length > 1) return;
    try {
      const t = await templates();
      const keep = this.pvSel.value;
      for (const c of t.templates) {
        if (c.id === PREVIEW_DEFAULT) continue;
        const o = document.createElement('option'); o.value = c.id; o.textContent = c.name; this.pvSel.appendChild(o);
      }
      this.pvSel.value = keep;
    } catch { }
  }

  async open(name?: string): Promise<void> {
    if (!this.d.isOpen) this.d.open(this.nameIn);
    await Promise.all([this.loadFamilies(), this.loadTemplates(), this.reload()]);
    const pick = name ? this.list.find((b) => b.name === name) : this.list.find((b) => b.name === this.current) ?? this.list[0];
    if (pick) this.edit(pick.name); else this.startNew(true);
  }

  private async reload(): Promise<void> {
    this.list = await brands(true);
    this.renderList();
  }

  private renderList(): void {
    type Row = { key: string; name: string; b: Brand | null };
    const rows: Row[] = this.list.map((b) => ({ key: b.name, name: b.name, b }));
    if (this.current === NEW_KEY) rows.push({ key: NEW_KEY, name: this.draft.name || 'New brand kit', b: null });
    patchList<Row>(q(this.d.body, '.mk-bk-list'), rows, {
      key: (r) => r.key,
      create: (r) => {
        const n = el('<button class="mk-bk-item" type="button" role="option"><span class="mk-bk-chips"><i></i><i></i><i></i></span><span class="mk-bk-item-name"></span></button>');
        n.addEventListener('click', () => void this.switchTo(n.dataset.key ?? ''));
        this.updateRow(n, r);
        return n;
      },
      update: (n, r) => this.updateRow(n, r),
    });
  }

  private updateRow(n: HTMLElement, r: { key: string; name: string; b: Brand | null }): void {
    const pal = r.key === this.current ? this.draft.palette : r.b?.palette ?? {};
    const dots = Array.from(n.querySelectorAll('i'));
    (['accent', 'ink', 'paper'] as PaletteRole[]).forEach((role, i) => {
      const v = pal[role] ?? 'transparent';
      if (dots[i].style.getPropertyValue('--c') !== v) dots[i].style.setProperty('--c', v);
      setClass(dots[i], 'is-unset', !pal[role]);
    });
    setText(q(n, '.mk-bk-item-name'), r.key === this.current && this.draft.name ? this.draft.name : r.name);
    setClass(n, 'is-on', r.key === this.current);
    setClass(n, 'is-draft', r.key === NEW_KEY);
    n.setAttribute('aria-selected', String(r.key === this.current));
  }

  private async switchTo(key: string): Promise<void> {
    if (key === this.current) return;
    if (this.dirty && !(await confirmAction('Unsaved Brand Kit', `Discard the changes to “${this.draft.name || 'the new brand kit'}”?`, 'Discard', true))) return;
    if (key === NEW_KEY) return;
    this.edit(key);
  }

  private edit(name: string): void {
    const b = this.list.find((x) => x.name === name);
    if (!b) return;
    this.current = name;
    this.saved = name;
    this.draft = fromBrand(b);
    this.dirty = false;
    this.renderList();
    this.syncForm();
    this.preview();
  }

  private async startNew(force = false): Promise<void> {
    if (!force && this.dirty && !(await confirmAction('Unsaved Brand Kit', `Discard the changes to “${this.draft.name || 'the new brand kit'}”?`, 'Discard', true))) return;
    this.current = NEW_KEY;
    this.saved = '';
    this.draft = blank();
    this.dirty = false;
    this.renderList();
    this.syncForm();
    this.preview();
    this.nameIn.focus();
  }

  private touch(): void {
    this.dirty = true;
    this.renderList();
    this.syncButtons();
    setText(q(this.d.body, '.mk-bk-pv-note'), this.saved ? 'Save to see your changes here' : 'Save the kit to preview it on a template');
  }

  private syncButtons(): void {
    setDisabled(this.saveBtn, !this.draft.name.trim() || (!this.dirty && !!this.saved));
    setHidden(this.delBtn, !this.saved);
  }

  private syncForm(): void {
    setValue(this.nameIn, this.draft.name);
    if (document.activeElement !== this.voice && this.voice.value !== this.draft.voice) this.voice.value = this.draft.voice;
    for (const row of Array.from(q(this.d.body, '.mk-bk-pal').children) as HTMLElement[]) this.syncSwatch(row);
    for (const [role, { fam, sty }] of this.fontSels) {
      const f = this.draft.fonts[role];
      const famV = f?.[0] ?? '';
      if (famV && !Array.from(fam.options).some((o) => o.value === famV)) { const o = document.createElement('option'); o.value = famV; o.textContent = famV; fam.appendChild(o); }
      if (fam.value !== famV) fam.value = famV;
      const styV = f?.[1] ?? 'Regular';
      if (!Array.from(sty.options).some((o) => o.value === styV)) { const o = document.createElement('option'); o.value = styV; o.textContent = styV; sty.appendChild(o); }
      if (sty.value !== styV) sty.value = styV;
      sty.disabled = !famV;
    }
    const logo = this.draft.logo;
    setText(q(this.d.body, '.mk-bk-logo-name'), logo ? logo.split(/[\\/]/).pop() ?? logo : 'No logo');
    setTitle(q(this.d.body, '.mk-bk-logo-name'), logo);
    setClass(q(this.d.body, '.mk-bk-logo'), 'has-logo', !!logo);
    setHidden(q(this.d.body, '.mk-bk-logo-x'), !logo);
    setText(q(this.d.body, '.mk-bk-logo-pick'), logo ? 'Replace…' : 'Choose…');
    this.syncButtons();
  }

  private async pickLogo(): Promise<void> {
    const files = await this.ws.pickFiles('image/*,.svg,.pdf', false);
    if (!files.length) return;
    try {
      const [path] = await this.ws.upload(files);
      this.draft.logo = path;
      this.touch();
      this.syncForm();
    } catch (err) { toast(friendly((err as Error).message), 'error'); }
  }

  private async save(): Promise<void> {
    const name = this.draft.name.trim();
    if (!name) { this.nameIn.focus(); return; }
    const done = delayedIndicator(() => setText(this.status, 'Saving…'), () => setText(this.status, ''));
    try {
      const body: Brand = { name, palette: this.draft.palette, fonts: this.draft.fonts };
      if (this.draft.logo) body.logo = this.draft.logo;
      if (this.draft.voice.trim()) body.voice = this.draft.voice.trim();
      const res = await api<Brand>('/brands', { json: body });
      if (this.saved && this.saved !== res.name && this.saved.toLowerCase() !== res.name.toLowerCase()) {
        try { await api(`/brands/${encodeURIComponent(this.saved)}`, { method: 'DELETE' }); } catch { }
      }
      this.dirty = false;
      brandsChanged();
      await this.reload();
      this.edit(res.name);
      toast(`Saved the ${res.name} brand kit`);
    } catch (err) {
      toast(friendly((err as Error).message), 'error');
    } finally { done(); }
  }

  private async remove(): Promise<void> {
    const name = this.saved;
    if (!name) return;
    if (!(await confirmAction('Delete Brand Kit', `Delete the “${name}” brand kit? Layouts already made with it keep their colours and fonts.`, 'Delete', true))) return;
    try {
      await api(`/brands/${encodeURIComponent(name)}`, { method: 'DELETE' });
      this.dirty = false;
      brandsChanged();
      await this.reload();
      if (this.list[0]) this.edit(this.list[0].name); else void this.startNew(true);
    } catch (err) { toast(friendly((err as Error).message), 'error'); }
  }

  private preview(): void {
    const id = this.pvSel.value || PREVIEW_DEFAULT;
    const b = this.list.find((x) => x.name === this.saved);
    const box = q(this.d.body, '.mk-bk-pv-box');
    const page = q(this.d.body, '.mk-bk-pv-page');
    const img = q<HTMLImageElement>(this.d.body, '.mk-bk-pv-page img');
    setText(q(this.d.body, '.mk-bk-pv-note'), b ? `${b.name} on this template` : 'Save the kit to preview it on a template');
    const done = delayedIndicator(() => setClass(box, 'is-pending', true), () => setClass(box, 'is-pending', false));
    void templates().then((t) => {
      const c = t.templates.find((x) => x.id === id);
      return swapMedia(img, thumbUrl(id, 480, { brand: b?.name, v: b?.modified })).then(() => {
        if (c) setRatio(page, c.size[0] / c.size[1]);
        setClass(box, 'is-loaded', true);
      });
    }).catch(() => undefined).finally(done);
  }
}
