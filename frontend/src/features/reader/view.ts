import { api } from '../../engine/client';
import { el, patchList, q, setClass, setHidden, setText, setValue, swapMedia } from '../../ui/dom';
import { icon } from '../../ui/icons';
import { toast } from '../../ui/toast';
import { toolIcon } from '../../ui/tool-icons';

export interface PdfInfo { id: string; name: string; pages: number; sizes: Array<[number, number]>; labels: string[]; toc: Array<{ level: number; title: string; page: number }>; meta: Record<string, string> }
interface Span { x: number; y: number; w: number; h: number; text: string }
interface Hit { page: number; rects: number[][]; context: string }
interface OutlineItem { level: number; title: string; page: number }
interface PageBox { box: HTMLElement; img: HTMLImageElement; text: HTMLElement; hits: HTMLElement; res: number; textLoaded: boolean; w: number; h: number }

const ZOOMS = [0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];

export class ReaderView {
  readonly root: HTMLElement;
  private info: PdfInfo | null = null;
  private zoom = 1;
  private fit: 'width' | 'page' | null = 'width';
  private boxes: PageBox[] = [];
  private current = 0;
  private hits: Hit[] = [];
  private hitIndex = -1;
  private observer: IntersectionObserver;
  private thumbObserver: IntersectionObserver;
  private readonly scroller: HTMLElement;
  private readonly pagesHost: HTMLElement;
  private readonly thumbs: HTMLElement;
  private readonly measure = document.createElement('canvas').getContext('2d') as CanvasRenderingContext2D;
  private scrollQueued = false;
  private outline: OutlineItem[] = [];
  private drawerTab: 'outline' | 'text' = 'outline';
  private textPage = -1;
  onExtract: ((info: PdfInfo) => void) | null = null;
  onClose: (() => void) | null = null;

  constructor(host: HTMLElement) {
    this.root = el(`<section class="fr-reader" aria-label="PDF Reader" hidden>
<header class="fr-bar">
  <span class="fr-title">${toolIcon('pdf', 18)}<span class="fr-name"></span></span>
  <div class="fr-group fr-nav">
    <button class="cv-icon-btn" type="button" data-act="prev" title="Previous page">${icon('chevronUp', 16)}</button>
    <input class="cv-num fr-pagenum" type="text" inputmode="numeric" aria-label="Page">
    <span class="fr-of"></span>
    <button class="cv-icon-btn" type="button" data-act="next" title="Next page">${icon('chevronDown', 16)}</button>
  </div>
  <div class="fr-group fr-zoom">
    <button class="cv-icon-btn" type="button" data-act="zoom-out" title="Zoom out">${icon('zoomOut', 16)}</button>
    <select class="fr-zoom-sel" aria-label="Zoom"><option value="width">Fit width</option><option value="page">Fit page</option>${ZOOMS.map((z) => `<option value="${z}">${Math.round(z * 100)}%</option>`).join('')}</select>
    <button class="cv-icon-btn" type="button" data-act="zoom-in" title="Zoom in">${icon('zoomIn', 16)}</button>
  </div>
  <div class="fr-group fr-search">
    ${icon('search', 14)}<input class="fr-q" type="search" placeholder="Find in PDF" aria-label="Find in PDF" spellcheck="false">
    <span class="fr-count"></span>
    <button class="cv-icon-btn" type="button" data-act="hit-prev" title="Previous match">${icon('chevronUp', 14)}</button>
    <button class="cv-icon-btn" type="button" data-act="hit-next" title="Next match">${icon('chevronDown', 14)}</button>
  </div>
  <span class="fr-spacer"></span>
  <button class="cv-btn cv-btn-primary fr-extract" type="button" data-act="extract">${toolIcon('make', 14)}<span>Make a layout from this PDF</span></button>
  <button class="cv-icon-btn" type="button" data-act="close" title="Close the Reader (Esc)">${icon('close', 16)}</button>
</header>
<div class="fr-body">
  <aside class="fr-thumbs" aria-label="Pages"></aside>
  <main class="fr-scroll"><div class="fr-pages"></div></main>
  <aside class="fr-drawer">
    <div class="fr-tabs"><button class="fr-tab is-active" type="button" data-tab="outline">Outline</button><button class="fr-tab" type="button" data-tab="text">Text</button></div>
    <div class="fr-outline"></div>
    <div class="fr-text-tab" hidden><div class="fr-text-head"><span class="fr-text-page"></span><button class="cv-btn cv-btn-ghost" type="button" data-act="copy">Copy</button></div><pre class="fr-plain"></pre></div>
  </aside>
</div>
</section>`);
    host.appendChild(this.root);
    this.scroller = q(this.root, '.fr-scroll');
    this.pagesHost = q(this.root, '.fr-pages');
    this.thumbs = q(this.root, '.fr-thumbs');
    this.observer = new IntersectionObserver((es) => this.onVisible(es), { root: this.scroller, rootMargin: '800px 0px' });
    this.thumbObserver = new IntersectionObserver((es) => {
      for (const e of es) {
        if (!e.isIntersecting) continue;
        const img = e.target.querySelector('img') as HTMLImageElement;
        const n = Number((e.target as HTMLElement).dataset.page);
        if (img && !img.dataset.src && this.info) { img.dataset.src = '1'; void swapMedia(img, `/api/reader/${this.info.id}/page/${n}.png?maxSide=180`).catch(() => undefined); }
      }
    }, { root: this.thumbs, rootMargin: '300px 0px' });
    this.wire();
  }

  get isOpen(): boolean { return !this.root.hidden; }

  async open(info: PdfInfo): Promise<void> {
    this.info = info;
    this.hits = [];
    this.hitIndex = -1;
    this.textPage = -1;
    setText(q(this.root, '.fr-name'), info.name);
    setText(q(this.root, '.fr-of'), `of ${info.pages}`);
    setText(q(this.root, '.fr-count'), '');
    setValue(q<HTMLInputElement>(this.root, '.fr-q'), '');
    setHidden(this.root, false);
    this.buildPages();
    this.fit = 'width';
    this.applyZoom(true);
    this.scroller.scrollTop = 0;
    this.setCurrent(0);
    this.loadOutline();
  }

  close(): void {
    setHidden(this.root, true);
    this.onClose?.();
  }

  private wire(): void {
    this.root.addEventListener('click', (e) => {
      const b = (e.target as HTMLElement).closest<HTMLElement>('[data-act]');
      if (b) { this.act(b.dataset.act as string); return; }
      const tab = (e.target as HTMLElement).closest<HTMLElement>('[data-tab]');
      if (tab) { this.showTab(tab.dataset.tab as 'outline' | 'text'); return; }
      const th = (e.target as HTMLElement).closest<HTMLElement>('.fr-thumb');
      if (th) { this.goTo(Number(th.dataset.page)); return; }
      const ol = (e.target as HTMLElement).closest<HTMLElement>('.fr-ol-item');
      if (ol) this.goTo(Number(ol.dataset.page));
    });
    const page = q<HTMLInputElement>(this.root, '.fr-pagenum');
    page.addEventListener('change', () => { const n = parseInt(page.value, 10); if (Number.isFinite(n)) this.goTo(n - 1); });
    page.addEventListener('keydown', (e) => { if (e.key === 'Enter') page.blur(); });
    const sel = q<HTMLSelectElement>(this.root, '.fr-zoom-sel');
    sel.addEventListener('change', () => {
      if (sel.value === 'width' || sel.value === 'page') { this.fit = sel.value; } else { this.fit = null; this.zoom = Number(sel.value); }
      this.applyZoom();
    });
    const qi = q<HTMLInputElement>(this.root, '.fr-q');
    let timer = 0;
    qi.addEventListener('input', () => { window.clearTimeout(timer); timer = window.setTimeout(() => void this.search(qi.value), 260); });
    qi.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); this.stepHit(e.shiftKey ? -1 : 1); } });
    this.scroller.addEventListener('scroll', () => {
      if (this.scrollQueued) return;
      this.scrollQueued = true;
      requestAnimationFrame(() => { this.scrollQueued = false; this.trackCurrent(); });
    }, { passive: true });
    this.scroller.addEventListener('wheel', (e) => {
      if (!(e.ctrlKey || e.metaKey)) return;
      e.preventDefault();
      this.fit = null;
      this.zoom = Math.min(6, Math.max(0.25, this.zoom * Math.exp(-e.deltaY * 0.002)));
      this.applyZoom();
    }, { passive: false });
    new ResizeObserver(() => { if (this.isOpen && this.fit) this.applyZoom(); }).observe(this.scroller);
    window.addEventListener('keydown', (e) => {
      if (!this.isOpen) return;
      const t = e.target as HTMLElement;
      const typing = t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT');
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'f') { e.preventDefault(); e.stopImmediatePropagation(); qi.focus(); qi.select(); return; }
      if (e.metaKey || e.ctrlKey || typing) return;
      e.stopImmediatePropagation();
      if (e.key === 'Escape') this.close();
      else if (e.key === 'PageDown' || e.key === 'ArrowRight') { e.preventDefault(); this.goTo(this.current + 1); }
      else if (e.key === 'PageUp' || e.key === 'ArrowLeft') { e.preventDefault(); this.goTo(this.current - 1); }
      else if (e.key === 'Home') this.goTo(0);
      else if (e.key === 'End') this.goTo((this.info?.pages ?? 1) - 1);
    }, true);
  }

  private act(a: string): void {
    if (a === 'prev') this.goTo(this.current - 1);
    else if (a === 'next') this.goTo(this.current + 1);
    else if (a === 'zoom-in' || a === 'zoom-out') {
      const next = a === 'zoom-in' ? ZOOMS.find((z) => z > this.zoom + 0.01) : [...ZOOMS].reverse().find((z) => z < this.zoom - 0.01);
      if (next) { this.fit = null; this.zoom = next; this.applyZoom(); }
    } else if (a === 'hit-prev') this.stepHit(-1);
    else if (a === 'hit-next') this.stepHit(1);
    else if (a === 'close') this.close();
    else if (a === 'extract' && this.info) this.onExtract?.(this.info);
    else if (a === 'copy') {
      const t = q(this.root, '.fr-plain').textContent ?? '';
      void navigator.clipboard?.writeText(t).then(() => toast('Copied the page text'), () => toast('Copying is not allowed here', 'error'));
    }
  }

  private buildPages(): void {
    const info = this.info as PdfInfo;
    for (const b of this.boxes) this.observer.unobserve(b.box);
    this.pagesHost.replaceChildren();
    this.boxes = info.sizes.map(([w, h], n) => {
      const box = el(`<div class="fr-page" data-page="${n}"><img class="fr-img" alt="" decoding="async" draggable="false"><div class="fr-textlayer"></div><div class="fr-hits"></div></div>`);
      this.pagesHost.appendChild(box);
      this.observer.observe(box);
      return { box, img: q<HTMLImageElement>(box, 'img'), text: q(box, '.fr-textlayer'), hits: q(box, '.fr-hits'), res: 0, textLoaded: false, w, h };
    });
    for (const t of Array.from(this.thumbs.children)) this.thumbObserver.unobserve(t);
    patchList(this.thumbs, info.sizes.map((s, n) => ({ n, s })), {
      key: (it) => `${info.id}:${it.n}`,
      create: (it) => {
        const node = el(`<button class="fr-thumb" type="button" data-page="${it.n}"><span class="fr-thumb-box"><img alt="" decoding="async" draggable="false"></span><span class="fr-thumb-label"></span></button>`);
        q<HTMLElement>(node, '.fr-thumb-box').style.aspectRatio = `${it.s[0]} / ${it.s[1]}`;
        setText(q(node, '.fr-thumb-label'), info.labels[it.n] ?? String(it.n + 1));
        this.thumbObserver.observe(node);
        return node;
      },
      update: () => undefined,
    });
  }

  private applyZoom(first = false): void {
    if (!this.info) return;
    const anchor = this.current;
    const box = this.boxes[anchor];
    const frac = box ? (this.scroller.scrollTop - box.box.offsetTop) / Math.max(1, box.box.offsetHeight) : 0;
    const maxW = Math.max(...this.info.sizes.map((s) => s[0]));
    const maxH = Math.max(...this.info.sizes.map((s) => s[1]));
    const availW = this.scroller.clientWidth - 64, availH = this.scroller.clientHeight - 48;
    if (this.fit === 'width') this.zoom = Math.max(0.2, Math.min(6, availW / maxW));
    else if (this.fit === 'page') this.zoom = Math.max(0.2, Math.min(6, Math.min(availW / maxW, availH / maxH)));
    this.pagesHost.style.setProperty('--fr-z', String(this.zoom));
    for (const b of this.boxes) {
      b.box.style.width = `${Math.round(b.w * this.zoom)}px`;
      b.box.style.height = `${Math.round(b.h * this.zoom)}px`;
    }
    const sel = q<HTMLSelectElement>(this.root, '.fr-zoom-sel');
    const val = this.fit ?? String(this.zoom);
    if (![...sel.options].some((o) => o.value === val)) {
      let custom = sel.querySelector<HTMLOptionElement>('option[data-custom]');
      if (!custom) { custom = el<HTMLOptionElement>('<option data-custom></option>'); sel.appendChild(custom); }
      custom.value = val;
      custom.textContent = `${Math.round(this.zoom * 100)}%`;
    }
    sel.value = val;
    if (!first && box) this.scroller.scrollTop = box.box.offsetTop + frac * box.box.offsetHeight;
    this.refreshVisible();
  }

  private refreshVisible(): void {
    const top = this.scroller.scrollTop - 800, bottom = this.scroller.scrollTop + this.scroller.clientHeight + 800;
    this.boxes.forEach((b, n) => {
      if (b.box.offsetTop + b.box.offsetHeight >= top && b.box.offsetTop <= bottom) this.load(n);
    });
  }

  private onVisible(es: IntersectionObserverEntry[]): void {
    for (const e of es) if (e.isIntersecting) this.load(Number((e.target as HTMLElement).dataset.page));
  }

  private load(n: number): void {
    const b = this.boxes[n];
    if (!b || !this.info) return;
    const want = Math.ceil((Math.max(b.w, b.h) * this.zoom * Math.min(2, window.devicePixelRatio || 1)) / 256) * 256;
    if (want > b.res * 1.15 || (b.res > want * 2.2)) {
      b.res = want;
      void swapMedia(b.img, `/api/reader/${this.info.id}/page/${n}.png?maxSide=${want}`).catch(() => undefined);
    }
    if (!b.textLoaded) {
      b.textLoaded = true;
      void this.loadText(n);
    }
  }

  private async loadText(n: number): Promise<void> {
    const info = this.info;
    if (!info) return;
    let layer: { width: number; height: number; spans: Span[] };
    try { layer = await api(`/reader/${info.id}/text/${n}`); } catch { return; }
    if (this.info !== info) return;
    const b = this.boxes[n];
    const frag = document.createDocumentFragment();
    for (const s of layer.spans) {
      if (!s.text.trim() || s.h <= 0) continue;
      const fs = s.h * 0.88;
      this.measure.font = `100px sans-serif`;
      const natural = this.measure.measureText(s.text).width * fs / 100;
      const sx = natural > 0 ? (s.w / natural) : 1;
      const span = document.createElement('span');
      span.textContent = s.text;
      span.style.left = `${(s.x / layer.width) * 100}%`;
      span.style.top = `${(s.y / layer.height) * 100}%`;
      span.style.fontSize = `calc(var(--fr-z) * ${fs.toFixed(2)}px)`;
      span.style.transform = `scaleX(${sx.toFixed(4)})`;
      frag.appendChild(span);
    }
    b.text.appendChild(frag);
  }

  goTo(n: number): void {
    if (!this.info) return;
    const k = Math.max(0, Math.min(this.info.pages - 1, n));
    const b = this.boxes[k];
    if (!b) return;
    this.scroller.scrollTop = b.box.offsetTop - 16;
    this.setCurrent(k);
  }

  private trackCurrent(): void {
    const mid = this.scroller.scrollTop + this.scroller.clientHeight * 0.35;
    let k = 0;
    for (let i = 0; i < this.boxes.length; i++) {
      if (this.boxes[i].box.offsetTop <= mid) k = i; else break;
    }
    if (k !== this.current) this.setCurrent(k);
  }

  private setCurrent(k: number): void {
    this.current = k;
    setValue(q<HTMLInputElement>(this.root, '.fr-pagenum'), this.info?.labels[k] ?? String(k + 1));
    for (const t of Array.from(this.thumbs.children) as HTMLElement[]) setClass(t, 'is-current', Number(t.dataset.page) === k);
    const th = this.thumbs.children[k] as HTMLElement | undefined;
    if (th) {
      const r = th.getBoundingClientRect(), pr = this.thumbs.getBoundingClientRect();
      if (r.top < pr.top || r.bottom > pr.bottom) this.thumbs.scrollTop += r.top - pr.top - pr.height / 2 + r.height / 2;
    }
    if (this.drawerTab === 'text') void this.loadPlain(k);
  }

  private async search(qtext: string): Promise<void> {
    const info = this.info;
    if (!info) return;
    for (const b of this.boxes) b.hits.replaceChildren();
    this.hits = [];
    this.hitIndex = -1;
    if (qtext.trim().length < 2) { setText(q(this.root, '.fr-count'), ''); return; }
    try { this.hits = await api<Hit[]>(`/reader/${info.id}/search?q=${encodeURIComponent(qtext.trim())}`); } catch { this.hits = []; }
    if (this.info !== info) return;
    this.hits.forEach((h, i) => {
      const b = this.boxes[h.page];
      if (!b) return;
      for (const r of h.rects) {
        const mark = document.createElement('i');
        mark.dataset.hit = String(i);
        mark.style.left = `${(r[0] / b.w) * 100}%`;
        mark.style.top = `${(r[1] / b.h) * 100}%`;
        mark.style.width = `${(r[2] / b.w) * 100}%`;
        mark.style.height = `${(r[3] / b.h) * 100}%`;
        b.hits.appendChild(mark);
      }
    });
    setText(q(this.root, '.fr-count'), this.hits.length ? `${this.hits.length >= 200 ? '200+' : this.hits.length} found` : 'No matches');
    if (this.hits.length) this.stepHit(1);
  }

  private stepHit(d: number): void {
    if (!this.hits.length) return;
    this.hitIndex = (this.hitIndex + d + this.hits.length) % this.hits.length;
    for (const m of Array.from(this.pagesHost.querySelectorAll<HTMLElement>('.fr-hits i'))) setClass(m, 'is-current', m.dataset.hit === String(this.hitIndex));
    const h = this.hits[this.hitIndex];
    const b = this.boxes[h.page];
    if (!b) return;
    const y = b.box.offsetTop + ((h.rects[0]?.[1] ?? 0) / b.h) * b.box.offsetHeight;
    this.scroller.scrollTop = Math.max(0, y - this.scroller.clientHeight / 3);
    setText(q(this.root, '.fr-count'), `${this.hitIndex + 1} of ${this.hits.length >= 200 ? '200+' : this.hits.length}`);
  }

  private async loadOutline(): Promise<void> {
    const info = this.info;
    if (!info) return;
    const host = q(this.root, '.fr-outline');
    let items: OutlineItem[] = info.toc.map((t) => ({ level: t.level + 1, title: t.title, page: t.page }));
    host.replaceChildren(el('<p class="fr-empty">Reading the document structure</p>'));
    if (!items.length) {
      try {
        const s = await api<{ blocks: Array<{ type: string; level?: number; text: string; page?: number }> }>(`/reader/${info.id}/structure`);
        items = s.blocks.filter((b) => b.type === 'heading' && typeof b.page === 'number').map((b) => ({ level: b.level ?? 3, title: b.text, page: b.page as number }));
      } catch { items = []; }
    }
    if (this.info !== info) return;
    this.outline = items.slice(0, 400);
    host.replaceChildren();
    if (!this.outline.length) { host.appendChild(el('<p class="fr-empty">This PDF has no headings to outline.</p>')); return; }
    for (const it of this.outline) {
      const row = el(`<button class="fr-ol-item" type="button" data-page="${it.page}"><span class="fr-ol-title"></span><span class="fr-ol-page"></span></button>`);
      row.style.paddingLeft = `${8 + (Math.min(it.level, 4) - 1) * 12}px`;
      setClass(row, 'is-top', it.level <= 1);
      setText(q(row, '.fr-ol-title'), it.title);
      setText(q(row, '.fr-ol-page'), info.labels[it.page] ?? String(it.page + 1));
      host.appendChild(row);
    }
  }

  private showTab(t: 'outline' | 'text'): void {
    this.drawerTab = t;
    for (const b of Array.from(this.root.querySelectorAll<HTMLElement>('.fr-tab'))) setClass(b, 'is-active', b.dataset.tab === t);
    setHidden(q(this.root, '.fr-outline'), t !== 'outline');
    setHidden(q(this.root, '.fr-text-tab'), t !== 'text');
    if (t === 'text') void this.loadPlain(this.current);
  }

  private async loadPlain(n: number): Promise<void> {
    const info = this.info;
    if (!info || this.textPage === n) return;
    this.textPage = n;
    setText(q(this.root, '.fr-text-page'), `Page ${info.labels[n] ?? n + 1}`);
    try {
      const r = await api<{ text: string }>(`/reader/${info.id}/plain/${n}`);
      if (this.info === info && this.textPage === n) setText(q(this.root, '.fr-plain'), r.text.replace(/\r\n?/g, '\n'));
    } catch { }
  }
}
