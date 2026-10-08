import '../styles/book.css';
import { editor } from '../editor/editor';
import { api } from '../engine/client';
import { Dialog, promptName } from '../ui/dialogs';
import { el, patchList, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Chapter { path: string; name: string; pages: number | null; firstPage: number | null; open: boolean }
interface BookInfo { path: string; name: string; styleSource: number; chapters: Chapter[]; changed?: string[]; reopen?: string[] }
interface Listing { books: Array<{ path: string; name: string; documents: number }>; documents: Array<{ path: string; name: string }> }

const KEY = 'formdes.book.last';

class BookDialog {
  private readonly d = new Dialog('Book', 620);
  private readonly root = el(`<div class="fb-book">
    <div class="fb-top"><select class="cv-select fb-pick" aria-label="Book"></select><button class="cv-btn fb-new" type="button">New Book</button></div>
    <div class="fb-list" role="listbox" aria-label="Chapters"></div>
    <p class="fb-empty" hidden></p>
    <div class="fb-add"><select class="cv-select fb-docs" aria-label="Document to add"></select><button class="cv-btn fb-addbtn" type="button">Add Document</button></div>
    <div class="fb-tools">
      <button class="cv-btn fb-up" type="button">Move Up</button><button class="cv-btn fb-down" type="button">Move Down</button>
      <button class="cv-btn fb-src" type="button">Style Source</button><button class="cv-btn fb-rm" type="button">Remove</button>
    </div>
    <p class="fb-status" aria-live="polite"></p>
  </div>`);
  private readonly list = q(this.root, '.fb-list');
  private book: BookInfo | null = null;
  private picked = -1;
  private busy = false;
  private readonly actions: Record<string, HTMLButtonElement> = {};

  constructor(private readonly ws: Workspace) {
    this.d.body.appendChild(this.root);
    for (const [k, label, primary] of [['sync', 'Synchronize Styles', false], ['paginate', 'Update Numbering', false], ['export', 'Export Book PDF', true]] as const) {
      const b = el<HTMLButtonElement>(`<button class="cv-btn${primary ? ' cv-btn-primary' : ''}" type="button"></button>`);
      setText(b, label);
      b.addEventListener('click', () => void this.act(k));
      this.d.foot.appendChild(b);
      this.actions[k] = b;
    }
    q(this.root, '.fb-pick').addEventListener('change', (e) => void this.load((e.target as HTMLSelectElement).value));
    q(this.root, '.fb-new').addEventListener('click', () => void this.create());
    q(this.root, '.fb-addbtn').addEventListener('click', () => void this.add());
    q(this.root, '.fb-up').addEventListener('click', () => void this.edit({ op: 'move', index: this.picked, to: this.picked - 1 }, this.picked - 1));
    q(this.root, '.fb-down').addEventListener('click', () => void this.edit({ op: 'move', index: this.picked, to: this.picked + 1 }, this.picked + 1));
    q(this.root, '.fb-src').addEventListener('click', () => void this.edit({ op: 'source', index: this.picked }, this.picked));
    q(this.root, '.fb-rm').addEventListener('click', () => void this.edit({ op: 'remove', index: this.picked }, -1));
  }

  async open(): Promise<void> {
    this.d.open();
    await this.refresh();
  }

  private async refresh(prefer?: string): Promise<void> {
    let l: Listing;
    try { l = await api<Listing>('/books'); } catch (err) { this.status((err as Error).message); return; }
    const pick = q<HTMLSelectElement>(this.root, '.fb-pick');
    pick.replaceChildren(...(l.books.length ? l.books.map((b) => new Option(`${b.name} (${b.documents})`, b.path)) : [new Option('No books yet', '')]));
    const docs = q<HTMLSelectElement>(this.root, '.fb-docs');
    docs.replaceChildren(...(l.documents.length ? l.documents.map((d) => new Option(d.name, d.path)) : [new Option('Save a document to add it', '')]));
    let last = prefer ?? '';
    if (!last) { try { last = localStorage.getItem(KEY) ?? ''; } catch { last = ''; } }
    const path = l.books.some((b) => b.path === last) ? last : l.books[0]?.path ?? '';
    pick.value = path;
    await this.load(path);
  }

  private async load(path: string): Promise<void> {
    if (!path) { this.book = null; this.render(); return; }
    try { localStorage.setItem(KEY, path); } catch { }
    this.status('Reading the book…');
    try { this.book = await api<BookInfo>(`/books/info?path=${encodeURIComponent(path)}`); this.status(''); } catch (err) { this.book = null; this.status((err as Error).message); }
    this.picked = Math.min(this.picked, (this.book?.chapters.length ?? 0) - 1);
    this.render();
  }

  private render(): void {
    const chapters = this.book?.chapters ?? [];
    patchList<Chapter>(this.list, chapters, {
      key: (c) => c.path,
      create: (c) => {
        const r = el('<div class="fb-row" role="option" tabindex="-1"><span class="fb-src-mark" title="Style source"></span><span class="fb-name"></span><span class="fb-pages"></span><span class="fb-open"></span></div>');
        r.addEventListener('click', () => { this.picked = (this.book?.chapters ?? []).findIndex((x) => x.path === r.dataset.key); this.render(); });
        r.addEventListener('dblclick', () => void this.openChapter(r.dataset.key as string));
        this.row(r, c);
        return r;
      },
      update: (r, c) => this.row(r, c),
    });
    const empty = q(this.root, '.fb-empty');
    setHidden(empty, chapters.length > 0);
    setText(empty, this.book ? 'Add saved documents as chapters. They keep their own files; the book lists them in order.' : 'Make a book to collect documents into chapters, number their pages in sequence, share styles and export one PDF.');
    const has = this.picked >= 0 && this.picked < chapters.length;
    setDisabled(q<HTMLButtonElement>(this.root, '.fb-up'), !has || this.picked === 0 || this.busy);
    setDisabled(q<HTMLButtonElement>(this.root, '.fb-down'), !has || this.picked >= chapters.length - 1 || this.busy);
    setDisabled(q<HTMLButtonElement>(this.root, '.fb-src'), !has || this.busy);
    setDisabled(q<HTMLButtonElement>(this.root, '.fb-rm'), !has || this.busy);
    setDisabled(q<HTMLButtonElement>(this.root, '.fb-addbtn'), !this.book || !q<HTMLSelectElement>(this.root, '.fb-docs').value || this.busy);
    for (const b of Object.values(this.actions)) setDisabled(b, !chapters.length || this.busy);
    this.d.setTitle(this.book ? `Book · ${this.book.name}` : 'Book');
  }

  private row(r: HTMLElement, c: Chapter): void {
    const i = (this.book?.chapters ?? []).indexOf(c);
    setText(q(r, '.fb-name'), c.name);
    const range = c.pages && c.firstPage ? (c.pages > 1 ? `${c.firstPage} to ${c.firstPage + c.pages - 1}` : `${c.firstPage}`) : '';
    setText(q(r, '.fb-pages'), range);
    setText(q(r, '.fb-open'), c.open ? 'open' : '');
    setClass(q(r, '.fb-src-mark'), 'is-on', i === this.book?.styleSource);
    setClass(r, 'is-picked', i === this.picked);
    r.title = c.path;
  }

  private status(t: string): void { setText(q(this.root, '.fb-status'), t); }

  private async create(): Promise<void> {
    const name = await promptName('New Book', 'Book', 'Create');
    if (!name) return;
    try { const b = await api<BookInfo>('/books', { json: { name } }); await this.refresh(b.path); } catch (err) { toast((err as Error).message, 'error'); }
  }

  private async add(): Promise<void> {
    const doc = q<HTMLSelectElement>(this.root, '.fb-docs').value;
    if (!this.book || !doc) return;
    await this.edit({ op: 'add', doc }, this.book.chapters.length);
  }

  private async edit(body: Record<string, unknown>, pick: number): Promise<void> {
    if (!this.book) return;
    this.busy = true;
    this.render();
    try {
      this.book = await api<BookInfo>('/books/edit', { json: { path: this.book.path, ...body } });
      this.picked = pick;
      this.status('');
    } catch (err) { this.status((err as Error).message); }
    this.busy = false;
    this.render();
  }

  private async act(kind: 'sync' | 'paginate' | 'export'): Promise<void> {
    if (!this.book) return;
    this.busy = true;
    this.render();
    try {
      if (kind === 'export') {
        this.status('Exporting every chapter as one PDF…');
        const r = await fetch(`/api/books/export.pdf?path=${encodeURIComponent(this.book.path)}`);
        if (!r.ok) throw new Error((await r.json().catch(() => ({ detail: r.statusText }))).detail);
        const url = URL.createObjectURL(await r.blob());
        const a = el<HTMLAnchorElement>('<a hidden></a>');
        a.href = url;
        a.download = `${this.book.name}.pdf`;
        document.body.appendChild(a);
        a.click();
        a.remove();
        window.setTimeout(() => URL.revokeObjectURL(url), 4000);
        this.status(`Exported ${this.book.name}.pdf`);
      } else {
        this.status(kind === 'sync' ? 'Copying styles and swatches from the style source…' : 'Numbering pages across the chapters…');
        const r = await api<BookInfo>(`/books/${kind}`, { json: { path: this.book.path } });
        this.book = r;
        const n = r.changed?.length ?? 0;
        this.status(`${kind === 'sync' ? 'Styles synchronized' : 'Numbering updated'}: ${n} chapter${n === 1 ? '' : 's'} changed.${r.reopen?.length ? ` Reopen ${r.reopen.join(', ')} to see it.` : ''}`);
      }
    } catch (err) { this.status((err as Error).message); }
    this.busy = false;
    this.render();
  }

  private async openChapter(path: string): Promise<void> {
    try { await api('/documents/open', { json: { path } }); editor.refreshMeta(); this.d.close(); this.ws.viewport.refreshOverlays(); } catch (err) { toast((err as Error).message, 'error'); }
  }
}

export function installBook(ws: Workspace): void {
  let dialog: BookDialog | null = null;
  ws.addCommands({ id: 'ui.book', label: 'Book…', run: () => void (dialog ??= new BookDialog(ws)).open() });
  ws.addMenuItems({ menu: 'File', items: ['ui.book'], at: 'start' });
}
