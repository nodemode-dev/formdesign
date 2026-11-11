import { api } from '../../engine/client';
import { editor, friendly } from '../../editor/editor';
import { delayedIndicator, el, patchList, q, setClass, setDisabled, setHidden, setText, setTitle, swapMedia } from '../../ui/dom';
import { enhanceSelect } from '../../ui/dropdown';
import { icon } from '../../ui/icons';
import { toast } from '../../ui/toast';
import type { Workspace } from '../../ui/workspace';
import {
  brandParam, brands, bytes, onBrandsChanged, packImageUrl, setRatio, syncBrandOptions, templateCard, templates, thumbUrl,
  TEMPLATE_COLOURS, type MakeResult, type Pack, type Ranked, type Report,
} from './api';
import { fileKind, glyph } from './icons';
import { ReportView } from './report';
import { links, madeDocs } from './store';

const ACCEPT = '.md,.markdown,.txt,.docx,.rtf,.html,.htm,.pdf,.csv,.xlsx,image/*,.gif';
const LAYOUT = /\.(fdes|fdoc|idml)$/i;
const CARDS = 6;
const PLACEHOLDER = [
  'Say what to make, its tone and colours: an 8-page magazine feature, warm and elegant, cream paper, orange accent.',
  'Lines like these become fields:',
  'Headline: The Slow Craft',
  'CTA: Read the issue',
  'Date:  Venue:  Price:  URL:',
].join('\n');

interface Card { id: string; name: string; why: string; category: string; picked?: boolean; ghost?: boolean; aspect: number }
interface Source { pack: string; title: string }

const fileKey = (f: File) => `${f.name}:${f.size}:${f.lastModified}`;

export class MakeView {
  readonly root: HTMLElement;
  private files: File[] = [];
  private source: Source | null = null;
  private pack: Pack | null = null;
  private packSig = '';
  private ranked: Ranked[] = [];
  private picked: Card | null = null;
  private selected = '';
  private variations = 1;
  private busy = false;
  private zoneDrop = false;
  private previewGen = 0;
  private readonly previewUrls = new Map<string, string>();
  private readonly aspects = new Map<string, number>();
  private names = new Map<string, string>();
  private readonly report = new ReportView();
  private reportUid: number | null = null;
  private varUids: number[] = [];
  private readonly brief: HTMLTextAreaElement;
  private readonly brandSel: HTMLSelectElement;
  onMade: () => void = () => undefined;

  constructor(private readonly ws: Workspace) {
    this.root = el(`<div class="mk">
      <div class="mk-layout">
        <section class="mk-col mk-col-in">
          <div class="mk-head"><span class="mk-head-n">1</span><span class="mk-head-t">Content</span><span class="mk-head-aside mk-files-count"></span></div>
          <div class="mk-drop" role="button" tabindex="0" aria-label="Add files to make a layout from">
            <span class="mk-drop-ico">${glyph('drop', 20)}</span>
            <span class="mk-drop-text"><strong>Drop files here</strong> or click to choose</span>
            <span class="mk-drop-hint">Markdown, Word, PDF, text, spreadsheets, images</span>
          </div>
          <div class="mk-files"></div>
          <label class="mk-brief-wrap"><span class="mk-label">Brief</span><textarea class="mk-brief" rows="7" spellcheck="true"></textarea></label>
          <div class="mk-analyse-row"><button class="cv-btn mk-analyse" type="button">${icon('search', 14)}<span class="mk-analyse-label">Analyse</span></button><span class="mk-status mk-analyse-status" aria-live="polite"></span></div>
          <div class="mk-pack" hidden>
            <div class="mk-pack-title"></div>
            <div class="mk-pack-sub"></div>
            <div class="mk-pack-facts"></div>
            <div class="mk-pack-imgs"></div>
            <div class="mk-pack-row mk-pack-kinds"><span class="mk-pack-k">Reads as</span><span class="mk-pack-v mk-tags"></span></div>
            <div class="mk-pack-row mk-pack-colors"><span class="mk-pack-k">Colours</span><span class="mk-pack-v mk-dots"></span></div>
            <div class="mk-pack-fields"></div>
            <div class="mk-pack-warn"></div>
          </div>
        </section>
        <section class="mk-col mk-col-out">
          <div class="mk-head"><span class="mk-head-n">2</span><span class="mk-head-t">Template</span><button class="mk-link mk-browse" type="button">Browse all${glyph('arrowRight', 12)}</button></div>
          <div class="mk-cards" role="listbox" aria-label="Templates that fit"></div>
          <div class="mk-pick"><span class="mk-pick-name"></span><span class="mk-pick-why"></span></div>
          <div class="mk-head"><span class="mk-head-n">3</span><span class="mk-head-t">Style</span><button class="mk-link mk-manage" type="button">Brand kits${glyph('arrowRight', 12)}</button></div>
          <div class="mk-opts">
            <label class="mk-opt mk-opt-brand"><span class="mk-label">Brand</span><select class="mk-brand" aria-label="Brand"></select></label>
            <div class="mk-opt"><span class="mk-label">Variations</span><div class="mk-stepper"><button class="mk-step-btn mk-dec" type="button" aria-label="Fewer">${icon('remove', 12)}</button><span class="mk-step-v">1</span><button class="mk-step-btn mk-inc" type="button" aria-label="More">${icon('add', 12)}</button></div></div>
          </div>
          <div class="mk-go">
            <button class="mk-make" type="button">${icon('makeLayout', 14)}<span class="mk-make-label">Make layout</span></button>
            <span class="mk-status mk-go-status" aria-live="polite"></span>
          </div>
          <div class="mk-out">
            <div class="mk-out-head"><span class="mk-label">Report</span><div class="mk-vars" role="tablist" hidden></div></div>
            <p class="mk-out-empty">What was filled, what still shows sample text and anything that needs you shows here after you make a layout.</p>
          </div>
        </section>
      </div>
    </div>`);
    this.brief = q<HTMLTextAreaElement>(this.root, '.mk-brief');
    this.brief.placeholder = PLACEHOLDER;
    this.brandSel = q<HTMLSelectElement>(this.root, '.mk-brand');
    q(this.root, '.mk-out').appendChild(this.report.root);
    setHidden(this.report.root, true);
    syncBrandOptions(this.brandSel, [], [['', 'From the brief'], [TEMPLATE_COLOURS, 'Template colours']]);
    enhanceSelect(this.brandSel);
    this.wire();
    this.renderCards();
    this.sync();
    void this.loadBrands();
    void this.meta();
  }

  private async meta(): Promise<void> {
    try {
      const t = await templates();
      this.names = new Map(t.templates.map((c) => [c.id, c.name]));
      for (const c of t.templates) this.aspects.set(c.id, c.size[0] / c.size[1]);
    } catch { }
  }

  private wire(): void {
    const drop = q(this.root, '.mk-drop');
    drop.addEventListener('click', () => void this.choose());
    drop.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); void this.choose(); } });
    drop.addEventListener('dragenter', (e) => { if (e.dataTransfer?.types.includes('Files')) { e.preventDefault(); setClass(drop, 'is-over', true); } });
    drop.addEventListener('dragover', (e) => { if (e.dataTransfer?.types.includes('Files')) { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; setClass(drop, 'is-over', true); } });
    drop.addEventListener('dragleave', (e) => { if (!drop.contains(e.relatedTarget as Node | null)) setClass(drop, 'is-over', false); });
    drop.addEventListener('drop', (e) => {
      e.preventDefault();
      setClass(drop, 'is-over', false);
      this.zoneDrop = true;
      window.setTimeout(() => { this.zoneDrop = false; }, 0);
      void readDrop(e.dataTransfer).then((files) => this.addFiles(files));
    });
    this.brief.addEventListener('input', () => this.sync());
    this.brief.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void this.make(); }
    });
    q(this.root, '.mk-analyse').addEventListener('click', () => void this.analyse());
    q(this.root, '.mk-browse').addEventListener('click', () => void this.browse());
    q(this.root, '.mk-manage').addEventListener('click', () => links.openBrands());
    q(this.root, '.mk-dec').addEventListener('click', () => this.setVariations(this.variations - 1));
    q(this.root, '.mk-inc').addEventListener('click', () => this.setVariations(this.variations + 1));
    q(this.root, '.mk-make').addEventListener('click', () => void this.make());
    this.brandSel.addEventListener('change', () => this.refreshPreviews());
    this.report.onSlot = (slot) => void this.revealSlot(slot);
    this.report.onTemplate = (id) => this.selectTemplate(id, true);
    onBrandsChanged(() => void this.loadBrands());
    editor.on('docs', () => this.followDoc());
  }

  consumeZoneDrop(): boolean {
    const was = this.zoneDrop;
    this.zoneDrop = false;
    return was;
  }

  focusStart(): void {
    if (this.files.length || this.source) this.brief.focus({ preventScroll: true });
  }

  private async choose(): Promise<void> {
    const files = await this.ws.pickFiles(ACCEPT, true);
    this.addFiles(files);
  }

  addFiles(files: File[]): void {
    const usable = files.filter((f) => !LAYOUT.test(f.name) && (f.size > 0 || /\.[a-z0-9]+$/i.test(f.name)));
    if (!usable.length) return;
    const have = new Set(this.files.map(fileKey));
    for (const f of usable) if (!have.has(fileKey(f))) { this.files.push(f); have.add(fileKey(f)); }
    if (this.source) this.source = null;
    this.renderFiles();
    this.sync();
  }

  private removeFile(key: string): void {
    if (key === 'source') this.source = null;
    else this.files = this.files.filter((f) => fileKey(f) !== key);
    this.renderFiles();
    this.sync();
  }

  private renderFiles(): void {
    type Row = { key: string; name: string; kind: string; meta: string; title: string };
    const rows: Row[] = [];
    if (this.source) rows.push({ key: 'source', name: this.source.title || 'Content pack', kind: 'pdf', meta: 'content pack', title: 'Content read earlier into a content pack' });
    for (const f of this.files) rows.push({ key: fileKey(f), name: f.name, kind: fileKind(f.name), meta: bytes(f.size), title: f.name });
    patchList<Row>(q(this.root, '.mk-files'), rows, {
      key: (r) => r.key,
      create: (r) => {
        const n = el(`<div class="mk-file"><span class="mk-file-ico"></span><span class="mk-file-name"></span><span class="mk-file-meta"></span><button class="mk-file-x" type="button" aria-label="Remove">${icon('close', 12)}</button></div>`);
        q(n, '.mk-file-ico').innerHTML = glyph(r.kind, 14);
        q(n, '.mk-file-x').addEventListener('click', () => this.removeFile(n.dataset.key ?? ''));
        this.updateFile(n, r);
        return n;
      },
      update: (n, r) => this.updateFile(n, r),
    });
    const count = this.files.length + (this.source ? 1 : 0);
    setText(q(this.root, '.mk-files-count'), count ? `${count} ${count === 1 ? 'file' : 'files'}` : '');
    setClass(q(this.root, '.mk-drop'), 'has-files', count > 0);
  }

  private updateFile(n: HTMLElement, r: { name: string; meta: string; title: string }): void {
    setText(q(n, '.mk-file-name'), r.name);
    setText(q(n, '.mk-file-meta'), r.meta);
    setTitle(n, r.title);
  }

  private sig(): string {
    return JSON.stringify([this.source?.pack ?? '', this.files.map(fileKey), this.brief.value.trim()]);
  }

  private get hasInput(): boolean { return this.files.length > 0 || !!this.source || this.brief.value.trim().length > 0; }
  private get fresh(): boolean { return !!this.pack && this.packSig === this.sig(); }

  private sync(): void {
    const stale = !!this.pack && !this.fresh;
    setClass(q(this.root, '.mk-pack'), 'is-stale', stale);
    setText(q(this.root, '.mk-analyse-label'), this.pack ? (stale ? 'Analyse again' : 'Analysed') : 'Analyse');
    setDisabled(q<HTMLButtonElement>(this.root, '.mk-analyse'), this.busy || !this.hasInput || this.fresh);
    setDisabled(q<HTMLButtonElement>(this.root, '.mk-make'), this.busy || !this.hasInput);
    setText(q(this.root, '.mk-make-label'), this.variations > 1 ? `Make ${this.variations} layouts` : 'Make layout');
    setText(q(this.root, '.mk-step-v'), String(this.variations));
    setDisabled(q<HTMLButtonElement>(this.root, '.mk-dec'), this.variations <= 1);
    setDisabled(q<HTMLButtonElement>(this.root, '.mk-inc'), this.variations >= 3);
    setClass(this.root, 'is-busy', this.busy);
  }

  private setVariations(n: number): void {
    this.variations = Math.max(1, Math.min(3, n));
    this.sync();
  }

  private async loadBrands(): Promise<void> {
    const list = await brands();
    syncBrandOptions(this.brandSel, list, [['', 'From the brief'], [TEMPLATE_COLOURS, 'Template colours']]);
  }

  private progress(el2: HTMLElement, text: string): () => void {
    return delayedIndicator(() => { setText(el2, text); setClass(el2, 'is-on', true); }, () => { setText(el2, ''); setClass(el2, 'is-on', false); });
  }

  async analyse(): Promise<boolean> {
    if (this.busy) return false;
    if (this.fresh) return true;
    if (!this.hasInput) return false;
    this.busy = true;
    this.sync();
    this.showPackPending();
    const done = this.progress(q(this.root, '.mk-analyse-status'), this.files.length ? 'Reading your files…' : 'Reading the brief…');
    const sig = this.sig();
    try {
      let pack: Pack;
      if (!this.files.length && this.source) {
        const id = this.source.pack;
        pack = await api<Pack>(`/intake/${encodeURIComponent(id)}`, { method: 'PATCH', json: { brief: this.brief.value } });
      } else {
        const fd = new FormData();
        for (const f of this.files) fd.append('files', f, f.name);
        fd.append('brief', this.brief.value);
        pack = await api<Pack>('/intake', { method: 'POST', body: fd });
      }
      await this.adopt(pack, sig);
      return true;
    } catch (err) {
      toast(friendly((err as Error).message), 'error');
      if (!this.pack) setHidden(q(this.root, '.mk-pack'), true);
      return false;
    } finally {
      done();
      this.busy = false;
      setClass(q(this.root, '.mk-pack'), 'is-pending', false);
      this.sync();
    }
  }

  private async adopt(pack: Pack, sig: string): Promise<void> {
    if (!this.aspects.size) await this.meta();
    let ranked: Ranked[] = [];
    try { ranked = (await api<{ ranked: Ranked[] }>(`/templates/rank?pack=${encodeURIComponent(pack.pack)}&limit=${CARDS}`)).ranked; } catch { ranked = []; }
    this.pack = pack;
    this.packSig = sig;
    this.ranked = ranked;
    if (!this.picked && (!this.selected || !ranked.some((r) => r.id === this.selected))) this.selected = ranked[0]?.id ?? '';
    this.renderPack();
    this.renderCards();
    this.refreshPreviews();
  }

  async loadPack(packId: string, title?: string): Promise<void> {
    this.busy = true;
    this.sync();
    this.showPackPending();
    const done = this.progress(q(this.root, '.mk-analyse-status'), 'Loading the extracted content…');
    try {
      const pack = await api<Pack>(`/intake/${encodeURIComponent(packId)}`);
      this.files = [];
      this.source = { pack: pack.pack, title: title || pack.title || 'Extracted PDF' };
      if (!this.brief.value.trim() && pack.brief?.text) this.brief.value = pack.brief.text;
      this.renderFiles();
      this.picked = null;
      this.selected = '';
      await this.adopt(pack, this.sig());
    } catch (err) {
      toast(friendly((err as Error).message), 'error');
    } finally {
      done();
      this.busy = false;
      setClass(q(this.root, '.mk-pack'), 'is-pending', false);
      this.sync();
    }
  }

  private showPackPending(): void {
    const box = q(this.root, '.mk-pack');
    setHidden(box, false);
    setClass(box, 'is-pending', true);
  }

  private renderPack(): void {
    const p = this.pack;
    const box = q(this.root, '.mk-pack');
    setHidden(box, !p);
    if (!p) return;
    setText(q(box, '.mk-pack-title'), p.title || 'Untitled content');
    setText(q(box, '.mk-pack-sub'), [p.subtitle, p.author ? `by ${p.author}` : ''].filter(Boolean).join(' · '));
    setHidden(q(box, '.mk-pack-sub'), !p.subtitle && !p.author);
    const f = p.facts ?? {};
    const facts = [
      f.words ? `${f.words.toLocaleString()} words` : '',
      f.articles ? `${f.articles} ${f.articles === 1 ? 'article' : 'articles'}` : '',
      f.images ? `${f.images} ${f.images === 1 ? 'image' : 'images'}` : '',
      f.tables ? `${f.tables} ${f.tables === 1 ? 'table' : 'tables'}` : '',
      f.readingMinutes ? `${f.readingMinutes} min read` : '',
    ].filter(Boolean);
    setText(q(box, '.mk-pack-facts'), facts.join(' · ') || 'No text or images found, only the brief');
    const imgs = q(box, '.mk-pack-imgs');
    const images = p.images ?? [];
    setHidden(imgs, !images.length);
    patchList(imgs, images, {
      key: (i) => `${p.pack}:${i.id}`,
      create: (i) => {
        const n = el(`<span class="mk-pack-img"><img alt="" decoding="async"><span class="mk-pack-role"></span></span>`);
        void swapMedia(q<HTMLImageElement>(n, 'img'), packImageUrl(p.pack, i.id)).then(() => setClass(n, 'is-loaded', true)).catch(() => undefined);
        this.updateImg(n, i);
        return n;
      },
      update: (n, i) => this.updateImg(n, i),
    });
    const b = p.brief ?? {};
    const tags = [...(b.kinds ?? []), ...(b.tone ?? [])];
    setHidden(q(box, '.mk-pack-kinds'), !tags.length);
    patchList(q(box, '.mk-tags'), tags.map((t, i) => ({ t, k: `${i < (b.kinds ?? []).length ? 'k' : 't'}:${t}` })), {
      key: (x) => x.k,
      create: (x) => { const s = el('<span class="mk-tag"></span>'); setText(s, x.t); setClass(s, 'is-tone', x.k.startsWith('t:')); return s; },
      update: (s, x) => setText(s, x.t),
    });
    const cols = b.colors ?? [];
    setHidden(q(box, '.mk-pack-colors'), !cols.length);
    patchList(q(box, '.mk-dots'), cols.map((c) => ({ c })), {
      key: (x) => x.c,
      create: (x) => { const s = el('<span class="mk-dot"></span>'); s.style.setProperty('--dot', x.c); setTitle(s, x.c); return s; },
      update: () => undefined,
    });
    const fields = Object.entries(b.fields ?? {});
    const fbox = q(box, '.mk-pack-fields');
    setHidden(fbox, !fields.length);
    patchList(fbox, fields, {
      key: ([k]) => k,
      create: ([k, v]) => { const r = el('<div class="mk-field-row"><span class="mk-field-k"></span><span class="mk-field-v"></span></div>'); setText(q(r, '.mk-field-k'), k); setText(q(r, '.mk-field-v'), v); setTitle(r, v); return r; },
      update: (r, [, v]) => { setText(q(r, '.mk-field-v'), v); setTitle(r, v); },
    });
    const warn = q(box, '.mk-pack-warn');
    setHidden(warn, !(p.warnings ?? []).length);
    setText(warn, (p.warnings ?? []).join(' '));
  }

  private updateImg(n: HTMLElement, i: { name: string; role?: string; size?: string }): void {
    setTitle(n, `${i.name}${i.size ? ` · ${i.size}` : ''}${i.role ? ` · ${i.role}` : ''}`);
    const role = q(n, '.mk-pack-role');
    const show = i.role === 'hero' || i.role === 'logo';
    setText(role, show ? (i.role === 'hero' ? 'Hero' : 'Logo') : '');
    setHidden(role, !show);
  }

  private cards(): Card[] {
    const aspect = (id: string) => this.aspects.get(id) ?? 0.77;
    const out: Card[] = [];
    if (this.picked) out.push(this.picked);
    for (const r of this.ranked) {
      if (out.length >= CARDS) break;
      if (r.id === this.picked?.id) continue;
      out.push({ id: r.id, name: r.name, category: r.category, why: r.why.join(', '), aspect: aspect(r.id) });
    }
    if (!this.pack) for (let i = out.length; i < CARDS; i++) out.push({ id: `ghost-${i}`, name: '', why: '', category: '', ghost: true, aspect: [0.77, 1, 0.77, 1.3, 0.56, 0.77][i] });
    return out;
  }

  private renderCards(): void {
    const list = this.cards();
    patchList<Card>(q(this.root, '.mk-cards'), list, {
      key: (c) => c.id,
      create: (c) => {
        const n = el(`<button class="mk-card" type="button" role="option"><span class="mk-card-thumb"><span class="mk-card-page"><img alt="" decoding="async"></span></span><span class="mk-card-name"></span></button>`);
        if (!c.ghost) {
          n.addEventListener('click', () => this.selectTemplate(c.id, false));
          n.addEventListener('dblclick', () => void this.make());
        }
        this.updateCard(n, c);
        return n;
      },
      update: (n, c) => this.updateCard(n, c),
    });
    const sel = list.find((c) => c.id === this.selected && !c.ghost);
    setText(q(this.root, '.mk-pick-name'), sel ? sel.name : this.pack ? 'No template selected' : 'Templates that fit your content show here');
    setText(q(this.root, '.mk-pick-why'), sel ? (sel.picked ? 'your pick' : sel.why ? `fits: ${sel.why}` : '') : this.pack ? '' : 'Add files or a brief, then Analyse');
    setClass(q(this.root, '.mk-pick'), 'is-empty', !sel);
  }

  private updateCard(n: HTMLElement, c: Card): void {
    setClass(n, 'is-ghost', !!c.ghost);
    setClass(n, 'is-on', !c.ghost && c.id === this.selected);
    n.setAttribute('aria-selected', String(!c.ghost && c.id === this.selected));
    setText(q(n, '.mk-card-name'), c.name);
    setTitle(n, c.ghost ? '' : `${c.name}${c.why ? `\nFits: ${c.why}` : ''}`);
    setRatio(q(n, '.mk-card-page'), c.aspect);
    if (c.ghost) n.setAttribute('tabindex', '-1');
    if (!c.ghost && !n.dataset.thumb) {
      n.dataset.thumb = '1';
      void loadUnless(q<HTMLImageElement>(n, 'img'), thumbUrl(c.id, 240), () => !!n.dataset.preview).then((ok) => { if (ok) setClass(n, 'is-loaded', true); });
    }
  }

  selectTemplate(id: string, scroll: boolean): void {
    if (!this.ranked.some((r) => r.id === id) && this.picked?.id !== id) {
      void templateCard(id).then((t) => {
        if (!t) return;
        this.aspects.set(id, t.size[0] / t.size[1]);
        this.picked = { id, name: t.name, category: t.category, why: '', picked: true, aspect: t.size[0] / t.size[1] };
        this.selected = id;
        this.renderCards();
        this.refreshPreviews();
        if (scroll) q(this.root, '.mk-cards').scrollIntoView({ block: 'nearest', behavior: 'smooth' });
      });
      return;
    }
    this.selected = id;
    this.renderCards();
    if (scroll) q(this.root, '.mk-cards').scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }

  private async browse(): Promise<void> {
    const id = await links.pickTemplate();
    if (id) this.selectTemplate(id, false);
  }

  private refreshPreviews(): void {
    const gen = ++this.previewGen;
    const pack = this.pack;
    if (!pack) return;
    const brand = brandParam(this.brandSel.value);
    const ids = this.cards().filter((c) => !c.ghost).map((c) => c.id);
    ids.sort((a, b) => (a === this.selected ? -1 : b === this.selected ? 1 : 0));
    void (async () => {
      for (const id of ids) {
        if (gen !== this.previewGen) return;
        const node = q(this.root, '.mk-cards').querySelector<HTMLElement>(`[data-key="${CSS.escape(id)}"]`);
        if (!node) continue;
        try {
          const blob = await api<Blob>('/make/preview', { json: { pack: pack.pack, template: id, brand, page: 0, size: 240 } });
          if (gen !== this.previewGen) return;
          const url = URL.createObjectURL(blob);
          node.dataset.preview = '1';
          await swapMedia(q<HTMLImageElement>(node, 'img'), url);
          setClass(node, 'is-loaded', true);
          const old = this.previewUrls.get(id);
          this.previewUrls.set(id, url);
          if (old) URL.revokeObjectURL(old);
        } catch { }
      }
    })();
  }

  async make(): Promise<void> {
    if (this.busy) return;
    if (!this.fresh) {
      if (!this.hasInput || !(await this.analyse())) return;
    }
    const pack = this.pack;
    if (!pack) return;
    this.busy = true;
    this.sync();
    const done = this.progress(q(this.root, '.mk-go-status'), this.variations > 1 ? `Laying out ${this.variations} variations…` : 'Laying out…');
    try {
      const before = new Set(editor.docs.map((d) => d.uid));
      const body: Record<string, unknown> = { pack: pack.pack, variations: this.variations };
      if (this.selected) body.template = this.selected;
      const brand = brandParam(this.brandSel.value);
      if (brand !== undefined) body.brand = brand;
      const res = await api<MakeResult & { documents?: { docs: Array<{ uid: number }>; active: number } }>('/make', { json: body });
      const reports: Report[] = res.variations ?? [res as Report];
      const ov = res.documents ?? await api<{ docs: Array<{ uid: number }>; active: number }>('/documents');
      const fresh = ov.docs.map((d) => d.uid).filter((u) => !before.has(u));
      const uids = fresh.length >= reports.length ? fresh.slice(-reports.length) : [ov.docs[ov.active]?.uid ?? -1];
      reports.forEach((r, i) => { if (uids[i] !== undefined) madeDocs.set(uids[i], { report: r, pack }); });
      this.varUids = reports.length > 1 ? uids : [];
      this.showReport(uids[uids.length - 1] ?? null);
      editor.refreshMeta();
      this.onMade();
    } catch (err) {
      toast(friendly((err as Error).message), 'error');
    } finally {
      done();
      this.busy = false;
      this.sync();
    }
  }

  private showReport(uid: number | null): void {
    const made = uid === null ? undefined : madeDocs.get(uid);
    if (!made) return;
    this.reportUid = uid;
    this.report.render(made.report, made.pack, this.names);
    setHidden(this.report.root, false);
    setHidden(q(this.root, '.mk-out-empty'), true);
    const vars = q(this.root, '.mk-vars');
    const show = this.varUids.length > 1 && this.varUids.includes(uid ?? -1);
    setHidden(vars, !show);
    patchList(vars, show ? this.varUids : [], {
      key: (u) => String(u),
      create: (u) => {
        const b = el<HTMLButtonElement>('<button class="mk-var" type="button" role="tab"></button>');
        b.addEventListener('click', () => void this.openVariation(Number(b.dataset.key)));
        this.updateVar(b, u);
        return b;
      },
      update: (b, u) => this.updateVar(b, u),
    });
  }

  private updateVar(b: HTMLElement, u: number): void {
    const i = this.varUids.indexOf(u);
    setText(b, String(i + 1));
    const made = madeDocs.get(u);
    setTitle(b, `Variation ${i + 1}: ${made ? this.names.get(made.report.template) ?? made.report.template : ''}`);
    setClass(b, 'is-on', u === this.reportUid);
    b.setAttribute('aria-selected', String(u === this.reportUid));
  }

  private async openVariation(uid: number): Promise<void> {
    const idx = editor.docs.findIndex((d) => d.uid === uid);
    if (idx >= 0 && idx !== editor.active) await editor.activate(idx);
    this.showReport(uid);
  }

  private followDoc(): void {
    const uid = editor.doc?.uid;
    if (uid !== undefined && uid !== this.reportUid && madeDocs.has(uid)) this.showReport(uid);
    else if (this.reportUid !== null) {
      for (const b of Array.from(q(this.root, '.mk-vars').children) as HTMLElement[]) this.updateVar(b, Number(b.dataset.key));
    }
  }

  private async revealSlot(slot: string): Promise<void> {
    const uid = this.reportUid;
    if (uid !== null && editor.doc?.uid !== uid) {
      const idx = editor.docs.findIndex((d) => d.uid === uid);
      if (idx < 0) { toast('That layout has been closed'); return; }
      await editor.activate(idx);
    }
    links.revealSlot(slot);
  }

  scrollToReport(): void {
    q(this.root, '.mk-out').scrollIntoView({ block: 'nearest', behavior: 'smooth' });
  }
}

function loadUnless(img: HTMLImageElement, url: string, skip: () => boolean): Promise<boolean> {
  return new Promise((resolve) => {
    const probe = new Image();
    probe.decoding = 'async';
    probe.onload = () => {
      void probe.decode().catch(() => undefined).then(() => {
        if (skip()) { resolve(false); return; }
        img.src = url;
        resolve(true);
      });
    };
    probe.onerror = () => resolve(false);
    probe.src = url;
  });
}

async function readDrop(dt: DataTransfer | null): Promise<File[]> {
  if (!dt) return [];
  const entries = Array.from(dt.items ?? []).map((i) => (typeof i.webkitGetAsEntry === 'function' ? i.webkitGetAsEntry() : null));
  if (!entries.some((e) => e?.isDirectory)) return Array.from(dt.files ?? []);
  const out: File[] = [];
  const walk = async (entry: FileSystemEntry, prefix: string): Promise<void> => {
    if (entry.isFile) {
      const f = await new Promise<File | null>((res) => (entry as FileSystemFileEntry).file(res, () => res(null)));
      if (f && !f.name.startsWith('.')) out.push(prefix ? new File([f], `${prefix}${f.name}`, { type: f.type, lastModified: f.lastModified }) : f);
      return;
    }
    const reader = (entry as FileSystemDirectoryEntry).createReader();
    for (;;) {
      const batch = await new Promise<FileSystemEntry[]>((res) => reader.readEntries(res, () => res([])));
      if (!batch.length) break;
      for (const e of batch) await walk(e, `${prefix}${entry.name}/`);
    }
  };
  for (const e of entries) if (e) await walk(e, '');
  return out;
}
