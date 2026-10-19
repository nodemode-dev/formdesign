import { api } from '../../engine/client';
import { editor, friendly } from '../../editor/editor';
import { confirmAction, Dialog } from '../../ui/dialogs';
import { delayedIndicator, el, patchList, q, setClass, setDisabled, setHidden, setText, setTitle, swapMedia } from '../../ui/dom';
import { enhanceSelect } from '../../ui/dropdown';
import { icon } from '../../ui/icons';
import { toast } from '../../ui/toast';
import {
  brands, categoryLabel, onBrandsChanged, pagesLabel, setRatio, sizeLabel, syncBrandOptions, templates, thumbUrl, type TemplateCard,
} from './api';

const CATS = ['all', 'magazine', 'newsletter', 'book', 'report', 'print', 'stationery', 'ad', 'web', 'presentation', 'mine'];

export class Gallery {
  private readonly d: Dialog;
  private readonly grid: HTMLElement;
  private readonly mid: HTMLElement;
  private readonly search: HTMLInputElement;
  private readonly brandSel: HTMLSelectElement;
  private readonly io: IntersectionObserver;
  private list: TemplateCard[] = [];
  private cat = 'all';
  private selected = '';
  private page = 0;
  private mode: 'create' | 'pick' = 'create';
  private resolve: ((id: string | null) => void) | null = null;
  private busy = false;
  private pvGen = 0;
  private readonly btnSample: HTMLButtonElement;
  private readonly btnEmpty: HTMLButtonElement;
  private readonly btnUse: HTMLButtonElement;
  private readonly btnDelete: HTMLButtonElement;
  private readonly status: HTMLElement;

  constructor() {
    this.d = new Dialog('New from Template', 980);
    q(this.d.scrim, '.cv-dialog').classList.add('mk-gal-dialog');
    this.d.body.append(el(`<div class="mk-gal">
      <nav class="mk-gal-cats" aria-label="Categories"></nav>
      <div class="mk-gal-mid">
        <label class="mk-gal-search">${icon('search', 16)}<input class="mk-gal-q" type="search" placeholder="Search templates" aria-label="Search templates" spellcheck="false"></label>
        <div class="mk-gal-scroll">
          <div class="mk-gal-grid" role="listbox" aria-label="Templates"></div>
          <p class="mk-gal-none" hidden></p>
        </div>
      </div>
      <aside class="mk-gal-side">
        <div class="mk-gal-pv"><div class="mk-gal-pv-page"><img alt="" decoding="async"></div></div>
        <div class="mk-gal-pager">
          <button class="mk-gal-pg mk-gal-prev" type="button" aria-label="Previous page">${icon('chevronLeft', 14)}</button>
          <span class="mk-gal-pg-label"></span>
          <button class="mk-gal-pg mk-gal-next" type="button" aria-label="Next page">${icon('chevronRight', 14)}</button>
        </div>
        <div class="mk-gal-info">
          <h3 class="mk-gal-name"></h3>
          <p class="mk-gal-kind"></p>
          <p class="mk-gal-desc"></p>
          <dl class="mk-gal-facts">
            <dt>Size</dt><dd class="mk-gal-f-size"></dd>
            <dt>Pages</dt><dd class="mk-gal-f-pages"></dd>
            <dt>Theme</dt><dd class="mk-gal-f-theme"></dd>
            <dt>Tone</dt><dd class="mk-gal-f-tone"></dd>
          </dl>
          <div class="mk-gal-slots-head"><span>Slots</span><span class="mk-gal-slots-n"></span></div>
          <div class="mk-gal-slots"></div>
        </div>
        <label class="mk-gal-brand"><span>Brand</span><select class="mk-gal-brand-sel" aria-label="Brand"></select></label>
      </aside>
    </div>`));
    this.grid = q(this.d.body, '.mk-gal-grid');
    this.mid = q(this.d.body, '.mk-gal-scroll');
    this.search = q<HTMLInputElement>(this.d.body, '.mk-gal-q');
    this.brandSel = q<HTMLSelectElement>(this.d.body, '.mk-gal-brand-sel');
    syncBrandOptions(this.brandSel, [], [['', 'Template colours']]);
    enhanceSelect(this.brandSel);
    this.status = el('<span class="mk-gal-status" aria-live="polite"></span>');
    this.btnDelete = this.d.button('Delete', 'danger', () => void this.remove());
    this.d.foot.append(this.status);
    this.d.button('Cancel', 'ghost', () => this.d.close());
    this.btnEmpty = this.d.button('Create empty', 'ghost', () => void this.create(false));
    this.btnSample = this.d.button('Create with sample content', 'primary', () => void this.create(true));
    this.btnUse = this.d.button('Use this template', 'primary', () => this.use());
    this.d.onClose = () => { const r = this.resolve; this.resolve = null; r?.(null); };
    this.io = new IntersectionObserver((entries) => {
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        const card = e.target as HTMLElement;
        this.io.unobserve(card);
        void swapMedia(q<HTMLImageElement>(card, 'img'), thumbUrl(card.dataset.key ?? '', 240)).then(() => setClass(card, 'is-loaded', true)).catch(() => setClass(card, 'is-failed', true));
      }
    }, { root: this.mid, rootMargin: '240px 0px' });
    this.search.addEventListener('input', () => this.filter());
    this.search.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') { e.preventDefault(); const first = this.visible()[0]; if (first && !this.visible().some((t) => t.id === this.selected)) this.select(first.id); }
    });
    q(this.d.body, '.mk-gal-prev').addEventListener('click', () => this.setPage(this.page - 1));
    q(this.d.body, '.mk-gal-next').addEventListener('click', () => this.setPage(this.page + 1));
    this.brandSel.addEventListener('change', () => this.preview());
    onBrandsChanged(() => void this.loadBrands());
  }

  async open(mode: 'create' | 'pick' = 'create', category?: string): Promise<string | null> {
    if (this.d.isOpen) { if (mode === this.mode) return null; this.d.close(); }
    this.mode = mode;
    if (category) this.cat = category;
    this.d.setTitle(mode === 'pick' ? 'Choose a Template' : 'New from Template');
    setHidden(this.btnSample, mode === 'pick');
    setHidden(this.btnEmpty, mode === 'pick');
    setHidden(this.btnUse, mode !== 'pick');
    setHidden(q(this.d.body, '.mk-gal-brand'), mode === 'pick');
    const result = new Promise<string | null>((r) => { this.resolve = r; });
    if (category && this.list.length) { this.renderCats(); this.filter(); this.mid.scrollTop = 0; }
    this.d.open(this.search);
    void this.loadBrands();
    await this.load(this.list.length > 0);
    if (category && !this.visible().some((t) => t.id === this.selected)) this.select(this.visible()[0]?.id ?? '');
    return result;
  }

  private async load(refresh: boolean): Promise<void> {
    try {
      const t = await templates(refresh);
      this.list = t.templates;
    } catch (err) {
      toast(friendly((err as Error).message), 'error');
      return;
    }
    patchList<TemplateCard>(this.grid, this.list, {
      key: (t) => t.id,
      create: (t) => {
        const n = el(`<button class="mk-gal-card" type="button" role="option"><span class="mk-gal-thumb"><span class="mk-gal-page"><img alt="" decoding="async"></span></span><span class="mk-gal-card-name"></span><span class="mk-gal-card-spec"></span></button>`);
        n.addEventListener('click', () => this.select(n.dataset.key ?? ''));
        n.addEventListener('dblclick', () => { this.select(n.dataset.key ?? ''); if (this.mode === 'pick') this.use(); else void this.create(true); });
        this.updateCard(n, t);
        n.dataset.key = t.id;
        this.io.observe(n);
        return n;
      },
      update: (n, t) => this.updateCard(n, t),
    });
    this.renderCats();
    this.filter();
    if (!this.selected || !this.list.some((t) => t.id === this.selected)) this.select(this.visible()[0]?.id ?? '');
    else this.select(this.selected, true);
  }

  private async loadBrands(): Promise<void> {
    syncBrandOptions(this.brandSel, await brands(), [['', 'Template colours']]);
  }

  private updateCard(n: HTMLElement, t: TemplateCard): void {
    setText(q(n, '.mk-gal-card-name'), t.name);
    setText(q(n, '.mk-gal-card-spec'), `${sizeLabel(t.size, t.unit)} · ${pagesLabel(t.pages)}`);
    setTitle(n, `${t.name}\n${t.description}`);
    setRatio(q(n, '.mk-gal-page'), t.size[0] / t.size[1]);
    setClass(n, 'is-user', t.user);
  }

  private counts(): Map<string, number> {
    const m = new Map<string, number>([['all', this.list.length], ['mine', 0]]);
    for (const t of this.list) {
      m.set(t.category, (m.get(t.category) ?? 0) + 1);
      if (t.user) m.set('mine', (m.get('mine') ?? 0) + 1);
    }
    return m;
  }

  private renderCats(): void {
    const counts = this.counts();
    const extra = [...counts.keys()].filter((c) => !CATS.includes(c));
    const cats = [...CATS.slice(0, -1), ...extra, 'mine'];
    patchList(q(this.d.body, '.mk-gal-cats'), cats, {
      key: (c) => c,
      create: (c) => {
        const b = el<HTMLButtonElement>('<button class="mk-gal-cat" type="button"><span class="mk-gal-cat-name"></span><span class="mk-gal-cat-n"></span></button>');
        b.addEventListener('click', () => { this.cat = b.dataset.key ?? 'all'; this.renderCats(); this.filter(); this.mid.scrollTop = 0; });
        if (c === 'mine') b.classList.add('is-mine');
        return b;
      },
      update: () => undefined,
    });
    for (const b of Array.from(q(this.d.body, '.mk-gal-cats').children) as HTMLElement[]) {
      const c = b.dataset.key ?? '';
      setText(q(b, '.mk-gal-cat-name'), c === 'all' ? 'All' : c === 'mine' ? 'Mine' : categoryLabel(c));
      setText(q(b, '.mk-gal-cat-n'), String(counts.get(c) ?? 0));
      setClass(b, 'is-on', c === this.cat);
      setClass(b, 'is-zero', !(counts.get(c) ?? 0));
    }
  }

  private matches(t: TemplateCard, words: string[]): boolean {
    if (this.cat === 'mine' ? !t.user : this.cat !== 'all' && t.category !== this.cat) return false;
    if (!words.length) return true;
    const hay = [t.name, t.description, t.category, t.theme, t.medium, ...t.tags, ...t.kinds, ...t.tone].join(' ').toLowerCase();
    return words.every((w) => hay.includes(w));
  }

  private visible(): TemplateCard[] {
    const words = this.search.value.trim().toLowerCase().split(/\s+/).filter(Boolean);
    return this.list.filter((t) => this.matches(t, words));
  }

  private filter(): void {
    const show = new Set(this.visible().map((t) => t.id));
    for (const n of Array.from(this.grid.children) as HTMLElement[]) setHidden(n, !show.has(n.dataset.key ?? ''));
    const none = q(this.d.body, '.mk-gal-none');
    setHidden(none, show.size > 0);
    setText(none, this.cat === 'mine' && !this.search.value.trim()
      ? 'Templates you or your agents save (save_template) show here, next to the built-in ones.'
      : `No templates match “${this.search.value.trim()}”${this.cat !== 'all' ? ` in ${this.cat === 'mine' ? 'Mine' : categoryLabel(this.cat)}` : ''}.`);
  }

  private select(id: string, keepPage = false): void {
    const t = this.list.find((x) => x.id === id);
    if (id !== this.selected || !keepPage) this.page = 0;
    this.selected = t ? id : '';
    for (const n of Array.from(this.grid.children) as HTMLElement[]) {
      const on = n.dataset.key === this.selected;
      setClass(n, 'is-on', on);
      n.setAttribute('aria-selected', String(on));
    }
    const side = q(this.d.body, '.mk-gal-side');
    setClass(side, 'is-empty', !t);
    setDisabled(this.btnSample, !t || this.busy);
    setDisabled(this.btnEmpty, !t || this.busy);
    setDisabled(this.btnUse, !t);
    setHidden(this.btnDelete, !t?.user || this.mode === 'pick');
    if (!t) return;
    setText(q(side, '.mk-gal-name'), t.name);
    setText(q(side, '.mk-gal-kind'), `${categoryLabel(t.category)} · ${t.medium === 'web' ? 'screen' : t.medium}${t.user ? ' · yours' : ''}`);
    setText(q(side, '.mk-gal-desc'), t.description);
    setText(q(side, '.mk-gal-f-size'), sizeLabel(t.size, t.unit));
    setText(q(side, '.mk-gal-f-pages'), pagesLabel(t.pages, t.facing));
    setText(q(side, '.mk-gal-f-theme'), t.theme || '–');
    setText(q(side, '.mk-gal-f-tone'), t.tone.join(', ') || '–');
    const slots = [...t.slots, ...t.flows.map((f) => `${f} text`)];
    setText(q(side, '.mk-gal-slots-n'), String(slots.length));
    patchList(q(side, '.mk-gal-slots'), slots, {
      key: (s) => s,
      create: (s) => { const c = el('<span class="mk-gal-slot"></span>'); setText(c, s); return c; },
      update: () => undefined,
    });
    this.preview();
  }

  private setPage(p: number): void {
    const t = this.list.find((x) => x.id === this.selected);
    if (!t) return;
    this.page = Math.max(0, Math.min(t.pages - 1, p));
    this.preview();
  }

  private preview(): void {
    const t = this.list.find((x) => x.id === this.selected);
    if (!t) return;
    const gen = ++this.pvGen;
    const side = q(this.d.body, '.mk-gal-side');
    setHidden(q(side, '.mk-gal-pager'), t.pages < 2);
    setText(q(side, '.mk-gal-pg-label'), `Page ${this.page + 1} of ${t.pages}`);
    setDisabled(q<HTMLButtonElement>(side, '.mk-gal-prev'), this.page <= 0);
    setDisabled(q<HTMLButtonElement>(side, '.mk-gal-next'), this.page >= t.pages - 1);
    const box = q(side, '.mk-gal-pv');
    const page = q(side, '.mk-gal-pv-page');
    const img = q<HTMLImageElement>(side, '.mk-gal-pv img');
    const brand = this.mode === 'pick' ? '' : this.brandSel.value;
    const done = delayedIndicator(() => setClass(box, 'is-pending', true), () => setClass(box, 'is-pending', false));
    const ratio = t.size[0] / t.size[1];
    if (!img.getAttribute('src')) setRatio(page, ratio);
    void swapMedia(img, thumbUrl(t.id, 480, { page: this.page, brand })).then(() => {
      if (gen !== this.pvGen) return;
      setRatio(page, ratio);
      setClass(box, 'is-loaded', true);
    }).catch(() => undefined).finally(done);
  }

  private use(): void {
    const id = this.selected;
    if (!id) return;
    const r = this.resolve;
    this.resolve = null;
    this.d.close();
    r?.(id);
  }

  private async create(sample: boolean): Promise<void> {
    const id = this.selected;
    if (!id || this.busy) return;
    this.busy = true;
    this.select(id, true);
    const done = delayedIndicator(() => setText(this.status, 'Building the document…'), () => setText(this.status, ''));
    try {
      const body: Record<string, unknown> = { sample };
      if (this.brandSel.value) body.brand = this.brandSel.value;
      await api(`/templates/${encodeURIComponent(id)}/new`, { json: body });
      editor.refreshMeta();
      this.d.close();
    } catch (err) {
      toast(friendly((err as Error).message), 'error');
    } finally {
      done();
      this.busy = false;
      this.select(this.selected, true);
    }
  }

  private async remove(): Promise<void> {
    const t = this.list.find((x) => x.id === this.selected);
    if (!t?.user) return;
    if (!(await confirmAction('Delete Template', `Delete “${t.name}”? Documents made from it are not changed.`, 'Delete', true))) return;
    try {
      await api(`/templates/${encodeURIComponent(t.id)}`, { method: 'DELETE' });
      this.selected = '';
      await this.load(true);
    } catch (err) { toast(friendly((err as Error).message), 'error'); }
  }
}
