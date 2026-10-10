import { api } from '../../engine/client';
import { editor, friendly } from '../../editor/editor';
import { Dialog } from '../../ui/dialogs';
import { delayedIndicator, el, q, reserve, setClass, setDisabled, setHidden, setText, setTitle, swapMedia } from '../../ui/dom';
import { toast } from '../../ui/toast';
import { Check, copyText, field, Pick, row, Seg } from './form';
import { docIcon } from './icons';
import { capabilities, join, safeName, stem } from './paths';
import { formatBytes } from './units';
import { WebOptions } from './web-options';

export type ExportFormat = 'pdf' | 'idml' | 'epub' | 'epub-fixed' | 'html' | 'web' | 'png' | 'jpg' | 'svg' | 'txt' | 'xml';

interface FormatInfo { id: ExportFormat; label: string; sub: string; icon: string; ext: string }

const FORMATS: FormatInfo[] = [
  { id: 'pdf', label: 'PDF', sub: 'Print or interactive', icon: 'pdf', ext: '.pdf' },
  { id: 'idml', label: 'InDesign Markup', sub: 'IDML package', icon: 'idml', ext: '.idml' },
  { id: 'epub', label: 'EPUB', sub: 'Reflowable e-book', icon: 'epub', ext: '.epub' },
  { id: 'epub-fixed', label: 'EPUB Fixed Layout', sub: 'Pages as designed', icon: 'epubFixed', ext: '.epub' },
  { id: 'html', label: 'HTML', sub: 'One self-contained page', icon: 'html', ext: '.html' },
  { id: 'web', label: 'Website', sub: 'Pages that fit any screen', icon: 'web', ext: '.zip' },
  { id: 'png', label: 'PNG', sub: 'A page as an image', icon: 'image', ext: '.png' },
  { id: 'jpg', label: 'JPEG', sub: 'A page as a photo', icon: 'image', ext: '.jpg' },
  { id: 'svg', label: 'SVG', sub: 'A page as vector graphics', icon: 'image', ext: '.svg' },
  { id: 'txt', label: 'Text', sub: 'The selected story', icon: 'text', ext: '.txt' },
  { id: 'xml', label: 'XML', sub: 'Tagged content', icon: 'xml', ext: '.xml' },
];

export const CORE_EXPORTS: Record<string, ExportFormat> = {
  'file.exportPdf': 'pdf', 'file.exportIdml': 'idml', 'file.exportEpub': 'epub', 'file.exportFixedEpub': 'epub-fixed',
  'file.exportHtml': 'html', 'file.exportText': 'txt', 'file.exportXml': 'xml', 'file.exportSvg': 'svg',
};

type Dest = 'download' | 'folder';

export class ExportDialog {
  private readonly d = new Dialog('Export', 820);
  private fmt: ExportFormat = 'pdf';
  private exportsDir = '';
  private nameTouched = false;
  private running = false;
  private readonly list = el('<div class="fd-ex-formats" role="listbox" aria-label="Format"></div>');
  private readonly groups = new Map<string, HTMLElement>();
  private readonly fmtTitle = el('<div class="fd-ex-title"><span class="fd-ex-ticon"></span><div><div class="fd-ex-tname"></div><div class="fd-ex-tsub"></div></div></div>');

  private readonly pdfMode = new Seg<'print' | 'interactive'>([{ value: 'print', label: 'Print' }, { value: 'interactive', label: 'Interactive' }], 'print');
  private readonly pdfPages = new Seg<'all' | 'range'>([{ value: 'all', label: 'All' }, { value: 'range', label: 'Range' }], 'all');
  private readonly pdfRange = el<HTMLInputElement>('<input class="fd-f-text" type="text" spellcheck="false" placeholder="1-3, 5">');
  private readonly pdfSpreads = new Check('Spreads');
  private readonly pdfBleed = new Check('Include bleed', true);
  private readonly pdfMarks = new Check('Crop marks');
  private readonly pdfFlatten = new Pick<string>([{ value: '', label: 'Off' }, { value: 'high', label: 'High resolution' }, { value: 'medium', label: 'Medium resolution' }, { value: 'low', label: 'Low resolution' }], '', 'Transparency flattener');
  private readonly pdfStandard = new Pick<string>([{ value: 'none', label: 'None' }, { value: 'x4', label: 'PDF/X-4' }, { value: 'a2b', label: 'PDF/A-2b' }], 'none', 'Standard');
  private readonly pdfLayout = new Pick<string>([{ value: 'single', label: 'Single page' }, { value: 'continuous', label: 'Continuous' }, { value: 'twoUp', label: 'Two-up' }, { value: 'twoUpCover', label: 'Two-up with cover' }, { value: 'twoUpContinuous', label: 'Two-up continuous' }], 'single', 'Page layout');
  private readonly pdfView = new Pick<string>([{ value: 'fitPage', label: 'Fit page' }, { value: 'fitWidth', label: 'Fit width' }, { value: 'actual', label: 'Actual size' }], 'fitPage', 'Opening view');
  private readonly pdfFull = new Check('Open in full screen');
  private readonly pdfBookmarks = new Check('Show bookmarks', true);
  private readonly pdfTagged = new Check('Tagged PDF (accessible structure)', true);
  private readonly pdfCompress = new Check('Compress images', true);
  private readonly idmlEmbed = new Check('Embed images in the package', true);
  private readonly metaTitle = el<HTMLInputElement>('<input class="fd-f-text" type="text" spellcheck="false">');
  private readonly metaAuthor = el<HTMLInputElement>('<input class="fd-f-text" type="text" spellcheck="false">');
  private readonly metaLang = el<HTMLInputElement>('<input class="fd-f-text" type="text" spellcheck="false" value="en">');
  private readonly epubCover = new Check('Use the first page as the cover', true);
  private readonly imgPage = new Pick<string>([{ value: '0', label: 'Page 1' }], '0', 'Page');
  private readonly imgScale = new Seg<'1' | '2' | '3' | '4'>([{ value: '1', label: '1×' }, { value: '2', label: '2×' }, { value: '3', label: '3×' }, { value: '4', label: '4×' }], '2');
  private readonly imgBleed = new Check('Include bleed');
  private readonly imgInfo = el('<p class="fd-f-note"></p>');
  private readonly imgPrev = el('<div class="fd-ex-prev"><img class="fd-ex-previmg" alt="" decoding="async"></div>');
  private readonly txtFormat = new Seg<'txt' | 'rtf' | 'tagged'>([{ value: 'txt', label: 'Plain' }, { value: 'rtf', label: 'RTF' }, { value: 'tagged', label: 'Tagged' }], 'txt');
  private readonly txtNote = el('<p class="fd-f-note"></p>');
  private readonly web = new WebOptions();

  private readonly dest = new Seg<Dest>([
    { value: 'download', label: 'Download', icon: docIcon('download', 15) },
    { value: 'folder', label: 'Exports folder', icon: docIcon('folder', 15) },
  ], 'download');
  private readonly fileName = el<HTMLInputElement>('<input class="fd-f-text" type="text" spellcheck="false" autocomplete="off">');
  private readonly folderLine = el(`<p class="fd-ex-folder">${docIcon('folder', 13)}<span class="fd-ex-folder-path"><bdi class="fd-ex-folder-text"></bdi></span></p>`);
  private readonly result = el(`<div class="fd-ex-result" hidden><span class="fd-ex-ok">${docIcon('check', 15)}</span><div class="fd-ex-rtext"><div class="fd-ex-rhead"></div><div class="fd-ex-rpath"></div><ul class="fd-ex-warn"></ul></div><button class="cv-icon-btn fd-ex-copy" type="button" title="Copy the path" aria-label="Copy the path">${docIcon('copy', 15)}</button></div>`);
  private readonly status = el('<span class="fd-ex-status" aria-live="polite"></span>');
  private readonly go: HTMLButtonElement;
  private readonly cancel: HTMLButtonElement;
  private lastPath = '';

  constructor() {
    q<HTMLElement>(this.d.scrim, '.cv-dialog').classList.add('fd-export');
    const ticon = q(this.fmtTitle, '.fd-ex-ticon');
    for (const name of new Set(FORMATS.map((f) => f.icon))) {
      const s = el(docIcon(name, 22));
      s.dataset.icon = name;
      s.hidden = true;
      ticon.appendChild(s);
    }
    for (const f of FORMATS) {
      const b = el<HTMLButtonElement>(`<button class="fd-ex-fmt" type="button" role="option">${docIcon(f.icon, 18)}<span class="fd-ex-fname"></span><span class="fd-ex-fsub"></span></button>`);
      setText(q(b, '.fd-ex-fname'), f.label);
      setText(q(b, '.fd-ex-fsub'), f.sub);
      b.dataset.fmt = f.id;
      b.addEventListener('click', () => this.setFormat(f.id));
      this.list.appendChild(b);
    }

    const pdf = this.group('pdf');
    pdf.append(
      field('Kind', this.pdfMode.root),
      row(field('Pages', this.pdfPages.root), field('Range', this.pdfRange)),
      this.sub('print', field('Marks and bleed', el('<div class="fd-f-checks"></div>'))),
      this.sub('print', row(field('Transparency', this.pdfFlatten.root), field('Standard', this.pdfStandard.root))),
      this.sub('interactive', row(field('Page layout', this.pdfLayout.root), field('Opening view', this.pdfView.root))),
      this.sub('interactive', el('<div class="fd-f-checks fd-ex-ichecks"></div>')),
      el('<div class="fd-f-checks fd-ex-pchecks"></div>'),
    );
    q(pdf, '.fd-f-checks').append(this.pdfBleed.root, this.pdfMarks.root);
    q(pdf, '.fd-ex-ichecks').append(this.pdfFull.root, this.pdfBookmarks.root);
    q(pdf, '.fd-ex-pchecks').append(this.pdfSpreads.root, this.pdfTagged.root, this.pdfCompress.root);
    this.pdfMode.onChange = () => this.syncPdf();
    this.pdfPages.onChange = () => { this.syncPdf(); if (this.pdfPages.value === 'range') this.pdfRange.focus(); };
    this.pdfRange.addEventListener('focus', () => { if (this.pdfPages.value !== 'range') { this.pdfPages.set('range'); this.syncPdf(); } });

    this.group('idml').append(el('<div class="fd-f-checks"></div>'));
    q(this.groups.get('idml') as HTMLElement, '.fd-f-checks').appendChild(this.idmlEmbed.root);
    this.group('idml').appendChild(el('<p class="fd-f-note">IDML keeps frames, threads, styles and swatches, so the layout opens in InDesign and other layout tools.</p>'));

    const meta = this.group('meta');
    meta.append(field('Title', this.metaTitle), row(field('Author', this.metaAuthor), field('Language', this.metaLang)));
    const cover = this.group('epub');
    cover.append(el('<div class="fd-f-checks"></div>'), el('<p class="fd-f-note">Text reflows to the reader’s screen and type size; images keep their place in the reading order.</p>'));
    q(cover, '.fd-f-checks').appendChild(this.epubCover.root);
    this.group('epub-fixed').appendChild(el('<p class="fd-f-note">Each page keeps its design, with its text kept live for search and reading aloud.</p>'));
    this.group('html').appendChild(el('<p class="fd-f-note">One page with the styles inline and the images embedded, the stories and graphics in reading order.</p>'));
    this.group('web').append(this.web.root, el('<p class="fd-f-note">A folder of web pages that reflow for phones, in the document’s type and colours, with a page-by-page edition. To put it online from here, use File &gt; Publish Online.</p>'));

    const img = this.group('image');
    img.append(row(field('Page', this.imgPage.root), field('Scale', this.imgScale.root)), el('<div class="fd-f-checks"></div>'), this.imgInfo, this.imgPrev);
    q(img, '.fd-f-checks').appendChild(this.imgBleed.root);
    this.imgScale.onChange = () => { this.syncImage(); this.syncName(); };
    this.imgPage.onChange = () => { this.syncImage(); this.syncName(); };
    this.imgBleed.onChange = () => this.syncImage();

    this.group('txt').append(field('Format', this.txtFormat.root), this.txtNote);
    this.txtFormat.onChange = () => this.syncName();
    this.group('xml').appendChild(el('<p class="fd-f-note">The tagged content in reading order, for databases and other publishing systems.</p>'));

    const opts = el('<div class="fd-ex-opts"></div>');
    opts.append(this.fmtTitle, ...this.groups.values());
    const destBox = el('<div class="fd-ex-dest"></div>');
    destBox.append(field('Save to', this.dest.root), field('File name', this.fileName), this.folderLine, this.result);
    const right = el('<div class="fd-ex-right"></div>');
    right.append(opts, destBox);
    const grid = el('<div class="fd-ex-grid"></div>');
    grid.append(this.list, right);
    this.d.body.appendChild(grid);
    this.d.body.classList.add('fd-ex-body');

    this.d.foot.appendChild(this.status);
    this.cancel = this.d.button('Cancel', 'ghost', () => this.d.close());
    this.go = this.d.button('Export', 'primary', () => void this.run());
    this.dest.onChange = () => { if (this.fmt === 'web') this.syncName(); else this.syncDest(); };
    this.fileName.addEventListener('input', () => { this.nameTouched = true; setHidden(this.result, true); this.syncDest(); });
    q(this.result, '.fd-ex-copy').addEventListener('click', () => void copyText(this.lastPath).then((ok) => toast(ok ? 'Path copied' : 'Could not copy the path', ok ? 'info' : 'error')));
    this.d.scrim.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && (e.target as HTMLElement).tagName === 'INPUT') { e.preventDefault(); this.go.click(); }
    });
    this.d.onClose = () => undefined;
  }

  async open(fmt?: ExportFormat): Promise<void> {
    if (!editor.hasDoc) { toast('Open a document to export it'); return; }
    this.nameTouched = false;
    setHidden(this.result, true);
    setText(this.status, '');
    setText(this.cancel, 'Cancel');
    const doc = editor.doc;
    this.metaTitle.value = stem(doc?.title);
    const pages = editor.layout?.pages ?? [];
    this.imgPage.setOptions(pages.map((p) => ({ value: String(p.abs), label: `Page ${p.name}` })));
    this.imgPage.set(String(pages.find((p) => p.abs === 0)?.abs ?? 0));
    this.setFormat(fmt ?? this.fmt, true);
    this.syncDest();
    this.d.open(this.go);
    const c = await capabilities();
    this.exportsDir = c.paths?.exports ?? '';
    this.syncDest();
  }

  private group(id: string): HTMLElement {
    let g = this.groups.get(id);
    if (!g) { g = el('<div class="fd-ex-group"></div>'); g.dataset.group = id; this.groups.set(id, g); }
    return g;
  }

  private sub(mode: string, node: HTMLElement): HTMLElement { node.dataset.mode = mode; return node; }

  private info(): FormatInfo { return FORMATS.find((f) => f.id === this.fmt) ?? FORMATS[0]; }

  private setFormat(fmt: ExportFormat, force = false): void {
    if (fmt === this.fmt && !force) return;
    this.fmt = fmt;
    const f = this.info();
    for (const b of Array.from(this.list.children) as HTMLElement[]) { setClass(b, 'is-on', b.dataset.fmt === fmt); b.setAttribute('aria-selected', String(b.dataset.fmt === fmt)); }
    for (const n of Array.from(q(this.fmtTitle, '.fd-ex-ticon').children) as HTMLElement[]) setHidden(n, n.dataset.icon !== f.icon);
    setText(q(this.fmtTitle, '.fd-ex-tname'), f.label);
    setText(q(this.fmtTitle, '.fd-ex-tsub'), f.sub);
    const show = new Set<string>([fmt === 'png' || fmt === 'jpg' || fmt === 'svg' ? 'image' : fmt]);
    if (fmt === 'epub' || fmt === 'epub-fixed' || fmt === 'html') show.add('meta');
    for (const [id, g] of this.groups) setHidden(g, !show.has(id));
    setHidden(this.result, true);
    this.syncPdf();
    this.syncImage();
    void this.syncText();
    this.syncName();
  }

  private syncPdf(): void {
    const g = this.groups.get('pdf') as HTMLElement;
    for (const n of Array.from(g.querySelectorAll<HTMLElement>('[data-mode]'))) setHidden(n, n.dataset.mode !== this.pdfMode.value);
    setClass(this.pdfRange, 'is-off', this.pdfPages.value !== 'range');
  }

  private syncImage(): void {
    const abs = Number(this.imgPage.value);
    const p = editor.layout?.pages.find((x) => x.abs === abs) ?? editor.layout?.pages[0];
    if (!p) { setText(this.imgInfo, ''); return; }
    const bleed = this.imgBleed.value ? this.bleedPt() : 0;
    const s = Number(this.imgScale.value);
    const pw = p.rect[2] - p.rect[0], ph = p.rect[3] - p.rect[1];
    const w = Math.round((pw + bleed * 2) * s), h = Math.round((ph + bleed * 2) * s);
    setText(this.imgInfo, `${w} × ${h} pixels at ${72 * s} ppi`);
    if (this.fmt !== 'png' && this.fmt !== 'jpg') return;
    reserve(this.imgPrev, { aspect: `${pw} / ${ph}` });
    void swapMedia(q<HTMLImageElement>(this.imgPrev, 'img'), editor.pageUrl(p.abs, 480)).catch(() => undefined);
  }

  private bleedPt(): number {
    const b = editor.layout?.bleed;
    return Array.isArray(b) ? Math.max(0, ...b) : Number(b ?? 0);
  }

  private async syncText(): Promise<void> {
    if (this.fmt !== 'txt') { setDisabled(this.go, false); return; }
    let ok = editor.wantsText;
    if (!ok) {
      const ins = await editor.inspect();
      const ids = new Set<number>(editor.selection?.items ?? []);
      ok = !!ins?.spreads?.some((s: any) => (s.items ?? []).some((it: any) => ids.has(it.id) && it.story !== undefined));
    }
    setClass(this.txtNote, 'is-warn', !ok);
    setText(this.txtNote, ok ? 'Exports the story of the selected text frame, through every frame it flows into.' : 'Select a text frame, or click into its text, to export its story.');
    if (this.fmt === 'txt') setDisabled(this.go, !ok);
  }

  private ext(): string {
    if (this.fmt === 'txt' && this.txtFormat.value === 'rtf') return '.rtf';
    if (this.fmt === 'web') return this.dest.value === 'folder' ? '' : '.zip';
    return this.info().ext;
  }

  private syncName(): void {
    const base = stem(editor.doc?.title);
    let name = base;
    if (this.fmt === 'web') name += ' site';
    if (this.fmt === 'png' || this.fmt === 'jpg' || this.fmt === 'svg') {
      const p = editor.layout?.pages.find((x) => x.abs === Number(this.imgPage.value));
      if ((editor.layout?.pages.length ?? 0) > 1 && p) name += ` p${p.name}`;
    }
    if (this.nameTouched) {
      const cur = this.fileName.value.trim().replace(/\.(pdf|idml|epub|html|zip|png|jpg|svg|txt|rtf|xml)$/i, '');
      this.fileName.value = `${cur || name}${this.ext()}`;
    } else this.fileName.value = `${name}${this.ext()}`;
    this.syncDest();
  }

  private syncDest(): void {
    const folder = this.dest.value === 'folder';
    setHidden(this.folderLine, !folder || !this.result.hidden);
    const path = join(this.exportsDir, safeName(this.fileName.value));
    setText(q(this.folderLine, '.fd-ex-folder-text'), this.exportsDir ? path : 'The Exports folder');
    setTitle(this.folderLine, path);
    setText(this.go, folder ? 'Export' : 'Download');
  }

  private options(): { format: string; options: Record<string, unknown> } {
    const fmt = this.fmt;
    const o: Record<string, unknown> = {};
    const meta = () => {
      if (this.metaTitle.value.trim()) o.title = this.metaTitle.value.trim();
      if (this.metaAuthor.value.trim()) o.author = this.metaAuthor.value.trim();
      if (this.metaLang.value.trim()) o.language = this.metaLang.value.trim();
    };
    switch (fmt) {
      case 'pdf': {
        if (this.pdfPages.value === 'range' && this.pdfRange.value.trim()) o.pages = this.pdfRange.value.trim();
        o.spreads = this.pdfSpreads.value;
        o.tagged = this.pdfTagged.value;
        o.compressImages = this.pdfCompress.value;
        if (this.pdfMode.value === 'print') {
          o.bleed = this.pdfBleed.value;
          o.marks = this.pdfMarks.value ? { crop: true } : false;
          if (this.pdfFlatten.value) o.flatten = this.pdfFlatten.value;
          if (this.pdfStandard.value !== 'none') o.standard = this.pdfStandard.value;
        } else {
          o.pageLayout = this.pdfLayout.value;
          o.view = this.pdfView.value;
          o.fullScreen = this.pdfFull.value;
          o.bookmarksPanel = this.pdfBookmarks.value;
          o.media = true;
        }
        if (this.metaTitle.value.trim()) o.title = this.metaTitle.value.trim();
        return { format: 'pdf', options: o };
      }
      case 'idml': o.embedImages = this.idmlEmbed.value; return { format: 'idml', options: o };
      case 'epub': meta(); o.cover = this.epubCover.value; return { format: 'epub', options: o };
      case 'epub-fixed': meta(); return { format: 'epub-fixed', options: o };
      case 'html': meta(); delete o.author; return { format: 'html', options: o };
      case 'web': return { format: 'web', options: this.web.values() };
      case 'png': case 'jpg':
        o.page = Number(this.imgPage.value); o.scale = Number(this.imgScale.value); o.bleed = this.imgBleed.value;
        return { format: fmt, options: o };
      case 'svg':
        o.page = Number(this.imgPage.value) + 1;
        return { format: 'svg', options: o };
      case 'txt':
        if (this.txtFormat.value === 'rtf') return { format: 'rtf', options: o };
        if (this.txtFormat.value === 'tagged') o.format = 'tagged';
        return { format: 'txt', options: o };
      default: return { format: fmt, options: o };
    }
  }

  private async run(): Promise<void> {
    if (this.running || !editor.hasDoc) return;
    if (this.fmt === 'web' && this.web.problem()) { toast(this.web.problem(), 'error'); return; }
    for (const inp of Array.from(this.d.body.querySelectorAll('input'))) inp.dispatchEvent(new Event('change'));
    const { format, options } = this.options();
    const folder = this.dest.value === 'folder';
    const name = safeName(this.fileName.value || `${stem(editor.doc?.title)}${this.ext()}`);
    this.running = true;
    setDisabled(this.go, true);
    setHidden(this.result, true);
    setText(this.status, '');
    const label = this.info().label;
    const done = delayedIndicator(() => { setText(this.status, `Exporting ${label}…`); setClass(this.status, 'is-busy', true); }, () => { setText(this.status, ''); setClass(this.status, 'is-busy', false); });
    try {
      if (folder) {
        if (!this.exportsDir) this.exportsDir = (await capabilities()).paths?.exports ?? '';
        const path = join(this.exportsDir, name);
        const r = await api<{ path?: string; bytes?: number; warnings?: unknown[] }>('/documents/export', { json: { format, path, options } });
        const where = r.path ?? path;
        this.lastPath = where;
        this.showResult(`Saved ${name}${r.bytes ? ` · ${formatBytes(r.bytes)}` : ''}`, where, r.warnings ?? []);
        setText(this.cancel, 'Close');
        toast(`Exported to ${where}`);
      } else {
        const blob = await api<Blob>('/documents/export', { json: { format, options } });
        if (!(blob instanceof Blob)) throw new Error('The engine did not return a file');
        const url = URL.createObjectURL(blob);
        const a = el<HTMLAnchorElement>('<a class="fd-f-copybuf"></a>');
        a.href = url;
        a.download = name;
        document.body.appendChild(a);
        a.click();
        a.remove();
        window.setTimeout(() => URL.revokeObjectURL(url), 20000);
        toast(`Downloaded ${name} · ${formatBytes(blob.size)}`);
        this.d.close();
      }
    } catch (err) {
      toast(friendly((err as Error).message), 'error');
    } finally {
      done();
      this.running = false;
      setDisabled(this.go, false);
      if (this.d.scrim.isConnected && !this.d.scrim.contains(document.activeElement)) this.go.focus();
      void this.syncText();
    }
  }

  private showResult(head: string, path: string, warnings: unknown[]): void {
    setText(q(this.result, '.fd-ex-rhead'), head);
    setText(q(this.result, '.fd-ex-rpath'), path);
    const ul = q(this.result, '.fd-ex-warn');
    ul.replaceChildren(...warnings.slice(0, 6).map((w) => { const li = document.createElement('li'); li.textContent = typeof w === 'string' ? w : JSON.stringify(w); return li; }));
    setHidden(ul, warnings.length === 0);
    setHidden(this.result, false);
    setHidden(this.folderLine, true);
  }
}
