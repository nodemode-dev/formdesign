import { editor } from '../editor/editor';
import type { Viewport } from '../editor/viewport';
import { pendingClass, q, setDisabled, setText, setTitle, type Disposer } from './dom';
import { icon } from './icons';

const INFO_KEY = 'formdesign.statusInfo';
const INFO_KINDS = ['pages', 'slots', 'selection', 'history'] as const;
type InfoKind = (typeof INFO_KINDS)[number];

export class StatusBar {
  private readonly zoom: HTMLInputElement;
  private readonly dims: HTMLElement;
  private readonly info: HTMLElement;
  private readonly busy: HTMLElement;
  private readonly hint: HTMLElement;
  private readonly undoBtn: HTMLButtonElement;
  private readonly redoBtn: HTMLButtonElement;
  private hintSegs: string[] = [];
  private infoKind: InfoKind = 'pages';
  private dismiss: Disposer | null = null;

  constructor(private readonly root: HTMLElement, private readonly viewport: Viewport) {
    root.insertAdjacentHTML('afterbegin',
      '<div class="cv-st-tools" role="group" aria-label="History and zoom">' +
      `<button class="cv-icon-btn cv-st-btn" type="button" data-cmd="undo" title="Undo" aria-label="Undo">${icon('undo', 14)}</button>` +
      `<button class="cv-icon-btn cv-st-btn" type="button" data-cmd="redo" title="Redo" aria-label="Redo">${icon('redo', 14)}</button>` +
      '<span class="cv-sep"></span>' +
      `<button class="cv-icon-btn cv-st-btn" type="button" data-cmd="zoom-out" title="Zoom Out" aria-label="Zoom Out">${icon('zoomOut', 14)}</button>` +
      '<input class="cv-st-zoom" type="text" inputmode="decimal" spellcheck="false" autocomplete="off" aria-label="Zoom percent" title="Zoom: type a percent and press Enter">' +
      `<button class="cv-icon-btn cv-st-btn" type="button" data-cmd="zoom-in" title="Zoom In" aria-label="Zoom In">${icon('zoomIn', 14)}</button>` +
      `<button class="cv-icon-btn cv-st-btn" type="button" data-cmd="fit-spread" title="Fit Spread in Window" aria-label="Fit Spread in Window">${icon('fitView', 14)}</button>` +
      '</div>' +
      '<span class="fd-st-dims cv-st-dims"></span>' +
      '<button class="cv-st-info" type="button"></button>');
    this.undoBtn = q<HTMLButtonElement>(root, '[data-cmd="undo"].cv-st-btn');
    this.redoBtn = q<HTMLButtonElement>(root, '[data-cmd="redo"].cv-st-btn');
    this.zoom = q<HTMLInputElement>(root, '.cv-st-zoom');
    this.dims = q(root, '.fd-st-dims');
    this.info = q(root, '.cv-st-info');
    this.hint = q(root, '.fd-st-hint');
    this.busy = el2('<span class="cv-st-busy">' + icon('loading', 12) + '<span>Working</span></span>');
    this.hint.after(this.busy);

    this.zoom.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        const n = parseFloat(this.zoom.value.replace(',', '.').replace('%', ''));
        if (Number.isFinite(n) && n > 0 && editor.hasDoc) this.viewport.zoomTo(Math.min(6400, Math.max(1, n)) / 100);
        this.zoom.blur();
      } else if (e.key === 'Escape') this.zoom.blur();
      e.stopPropagation();
    });
    this.zoom.addEventListener('focus', () => this.zoom.select());
    this.zoom.addEventListener('blur', () => this.syncView());

    try { const k = localStorage.getItem(INFO_KEY) as InfoKind | null; if (k && INFO_KINDS.includes(k)) this.infoKind = k; } catch { /* default */ }
    this.info.addEventListener('click', (e) => {
      const n = INFO_KINDS.length;
      this.infoKind = INFO_KINDS[(INFO_KINDS.indexOf(this.infoKind) + (e.shiftKey ? n - 1 : 1)) % n];
      try { localStorage.setItem(INFO_KEY, this.infoKind); } catch { /* not kept */ }
      this.syncInfo();
    });
    for (const k of ['docs', 'layout', 'selection', 'change'] as const) editor.on(k, () => this.syncInfo());
    for (const k of ['docs', 'change'] as const) editor.on(k, () => this.syncHistory());
    editor.on('layout', () => this.syncView());
    editor.on('busy', () => {
      if (editor.busy) { if (!this.dismiss) this.dismiss = pendingClass(this.busy); }
      else { this.dismiss?.(); this.dismiss = null; }
    });
    new ResizeObserver(() => this.fitHint()).observe(this.hint);
    this.syncView();
    this.syncInfo();
    this.syncHistory();
  }

  syncView(): void {
    const has = editor.hasDoc;
    const zt = has ? `${Number((this.viewport.view.zoom * 100).toFixed(1))}%` : '';
    if (document.activeElement !== this.zoom && this.zoom.value !== zt) this.zoom.value = zt;
    setDisabled(this.zoom, !has);
    const lay = editor.layout;
    const p = lay?.pages.find((x) => x.abs === this.viewport.currentPage()) ?? lay?.pages[0];
    const w = p ? Math.round((p.rect[2] - p.rect[0]) * 100) / 100 : 0, h = p ? Math.round((p.rect[3] - p.rect[1]) * 100) / 100 : 0;
    setText(this.dims, p ? `${w} × ${h} pt` : '');
    setTitle(this.dims, p ? 'Page size' : '');
  }

  private syncHistory(): void {
    const d = editor.doc;
    setDisabled(this.undoBtn, !d?.canUndo);
    setDisabled(this.redoBtn, !d?.canRedo);
  }

  private syncInfo(): void {
    const d = editor.doc, lay = editor.layout, kind = this.infoKind;
    let text = '', tip = '';
    if (d) {
      if (kind === 'pages') { text = `${d.pages} ${d.pages === 1 ? 'page' : 'pages'}${lay ? ` · ${new Set(lay.pages.map((p) => p.spread)).size} spreads` : ''}`; tip = 'Pages and spreads in the document'; }
      else if (kind === 'slots') { text = lay ? `${lay.slots.length} slots` : ''; tip = 'Named slots in the layout'; }
      else if (kind === 'selection') { const n = editor.selection?.items.length ?? 0; text = `${n} selected`; tip = 'Objects selected'; }
      else { text = d.undoLabel ? `Undo: ${d.undoLabel}` : 'Undo: nothing'; tip = 'What Undo would reverse'; }
    }
    setText(this.info, text);
    setTitle(this.info, `${tip}${tip ? '. ' : ''}Click for the next readout, Shift+click for the previous.`);
  }

  setHint(text: string): void {
    this.hintSegs = text.split(' · ');
    this.fitHint();
  }

  private fitHint(): void {
    const segs = this.hintSegs;
    if (!segs.length) return;
    let n = segs.length;
    setText(this.hint, segs.join(' · '));
    while (n > 1 && this.hint.scrollWidth > this.hint.clientWidth) { n--; setText(this.hint, segs.slice(0, n).join(' · ')); }
    setTitle(this.hint, n < segs.length ? segs.join(' · ') : '');
  }
}

function el2(html: string): HTMLElement {
  const t = document.createElement('template');
  t.innerHTML = html;
  return t.content.firstElementChild as HTMLElement;
}
