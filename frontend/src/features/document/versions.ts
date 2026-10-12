import '../../styles/versions.css';
import { editor, friendly } from '../../editor/editor';
import { api } from '../../engine/client';
import { Dialog } from '../../ui/dialogs';
import { el, patchList, q, setClass, setDisabled, setHidden, setText, swapMedia } from '../../ui/dom';
import { toast } from '../../ui/toast';
import type { Workspace } from '../../ui/workspace';
import { field } from './form';
import { formatBytes } from './units';

interface Version { id: string; time: number; kind: 'save' | 'named'; comment: string; bytes: number }
interface Listing { path: string | null; title: string | null; versions: Version[] }

const day = (t: number): string => {
  const d = new Date(t), now = new Date();
  const same = d.toDateString() === now.toDateString();
  const y = new Date(now.getTime() - 86400000).toDateString() === d.toDateString();
  const hm = d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
  return `${same ? 'Today' : y ? 'Yesterday' : d.toLocaleDateString([], { day: 'numeric', month: 'short', year: d.getFullYear() === now.getFullYear() ? undefined : 'numeric' })}, ${hm}`;
};

/** File > Version History and File > Save a Version: the copies kept under the data folder each time the document is saved. */
export class VersionsDialog {
  private readonly d = new Dialog('Version History', 780);
  private readonly comment = el<HTMLInputElement>('<input class="fd-f-text" type="text" spellcheck="true" maxlength="300" placeholder="What changed (optional)">');
  private readonly saveBtn: HTMLButtonElement;
  private readonly list = el('<div class="fd-vh-list" role="listbox" aria-label="Versions"></div>');
  private readonly empty = el('<p class="fd-f-note fd-vh-empty" hidden></p>');
  private readonly prev = el('<div class="fd-vh-prev"><img class="fd-vh-previmg" alt="" decoding="async"></div>');
  private readonly detail = el('<div class="fd-vh-detail"><div class="fd-vh-dname"></div><div class="fd-vh-dsub"></div></div>');
  private readonly status = el('<span class="fd-ex-status" aria-live="polite"></span>');
  private readonly restoreBtn: HTMLButtonElement;
  private readonly copyBtn: HTMLButtonElement;
  private versions: Version[] = [];
  private picked = '';
  private busy = false;

  constructor(private readonly ws: Workspace) {
    q<HTMLElement>(this.d.scrim, '.cv-dialog').classList.add('fd-versions');
    this.saveBtn = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">Save a Version</button>');
    const top = el('<div class="fd-vh-top"></div>');
    top.append(field('Comment', this.comment), this.saveBtn);
    const main = el('<div class="fd-vh-main"></div>');
    const side = el('<div class="fd-vh-side"></div>');
    side.append(this.prev, this.detail);
    main.append(this.list, side);
    this.d.body.append(top, main, this.empty);
    this.d.foot.appendChild(this.status);
    this.d.button('Close', 'ghost', () => this.d.close());
    this.copyBtn = this.d.button('Open a Copy', '', () => void this.openCopy());
    this.restoreBtn = this.d.button('Restore', 'primary', () => void this.restore());
    this.saveBtn.addEventListener('click', () => void this.save());
    this.comment.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); void this.save(); } });
    this.list.addEventListener('click', (e) => {
      const r = (e.target as HTMLElement).closest<HTMLElement>('.fd-vh-row');
      if (r) this.pick(r.dataset.key ?? '');
    });
  }

  async open(focusComment = false): Promise<void> {
    if (!editor.hasDoc) { toast('Open a document to see its versions'); return; }
    this.comment.value = '';
    this.d.open(focusComment ? this.comment : this.list);
    await this.load();
  }

  private async load(): Promise<void> {
    let r: Listing;
    try { r = await api<Listing>('/documents/versions'); } catch (err) { toast(friendly((err as Error).message), 'error'); return; }
    this.apply(r);
  }

  private apply(r: Listing): void {
    this.versions = r.versions;
    if (!this.versions.some((v) => v.id === this.picked)) this.picked = this.versions[0]?.id ?? '';
    this.d.setTitle(r.title ? `Version History: ${r.title}` : 'Version History');
    setHidden(this.empty, r.versions.length > 0);
    setText(this.empty, r.path ? 'No versions yet. A version is kept each time the document is saved.' : 'Save the document once to start keeping its versions.');
    patchList(this.list, this.versions, {
      key: (v) => v.id,
      create: (v) => {
        const row = el('<button class="fd-vh-row" type="button" role="option"><span class="fd-vh-when"></span><span class="fd-vh-what"></span><span class="fd-vh-size"></span></button>');
        this.patchRow(row, v);
        return row;
      },
      update: (row, v) => this.patchRow(row, v),
    });
    this.sync();
  }

  private patchRow(row: HTMLElement, v: Version): void {
    setText(q(row, '.fd-vh-when'), day(v.time));
    setText(q(row, '.fd-vh-what'), v.comment || (v.kind === 'named' ? 'Saved version' : 'Saved'));
    setText(q(row, '.fd-vh-size'), formatBytes(v.bytes));
    setClass(row, 'is-named', v.kind === 'named');
    setClass(row, 'is-picked', v.id === this.picked);
    row.setAttribute('aria-selected', String(v.id === this.picked));
  }

  private pick(id: string): void {
    this.picked = id;
    for (const row of Array.from(this.list.children) as HTMLElement[]) {
      const on = row.dataset.key === id;
      setClass(row, 'is-picked', on);
      row.setAttribute('aria-selected', String(on));
    }
    this.sync();
  }

  private sync(): void {
    const v = this.versions.find((x) => x.id === this.picked);
    setDisabled(this.restoreBtn, !v || this.busy);
    setDisabled(this.copyBtn, !v || this.busy);
    setClass(this.prev, 'is-empty', !v);
    setText(q(this.detail, '.fd-vh-dname'), v ? (v.comment || (v.kind === 'named' ? 'Saved version' : 'Saved')) : '');
    setText(q(this.detail, '.fd-vh-dsub'), v ? `${day(v.time)} · ${formatBytes(v.bytes)}` : '');
    if (v) void swapMedia(q<HTMLImageElement>(this.prev, 'img'), `/api/documents/versions/${v.id}/thumb`).catch(() => undefined);
  }

  private async run(label: string, job: () => Promise<void>): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    setText(this.status, label);
    setDisabled(this.saveBtn, true);
    this.sync();
    try { await job(); } catch (err) { toast(friendly((err as Error).message), 'error'); } finally {
      this.busy = false;
      setText(this.status, '');
      setDisabled(this.saveBtn, false);
      this.sync();
    }
  }

  private async save(): Promise<void> {
    await this.run('Saving…', async () => {
      const r = await api<Listing>('/documents/versions/save', { json: { comment: this.comment.value.trim() } });
      this.comment.value = '';
      this.apply(r);
      editor.refreshMeta();
      toast('Version saved');
    });
  }

  private async openCopy(): Promise<void> {
    const id = this.picked;
    if (!id) return;
    await this.run('Opening…', async () => {
      await api('/documents/versions/open', { json: { id } });
      editor.refreshMeta();
      this.d.close();
    });
  }

  private async restore(): Promise<void> {
    const id = this.picked;
    if (!id) return;
    await this.run('Restoring…', async () => {
      await api('/documents/versions/restore', { json: { id } });
      editor.refreshMeta();
      this.d.close();
      toast('Version restored; the document as it was is kept in the list');
    });
  }
}

export function installVersions(ws: Workspace): { open: (focusComment?: boolean) => void } {
  let dlg: VersionsDialog | null = null;
  return { open: (focusComment = false) => void (dlg ??= new VersionsDialog(ws)).open(focusComment) };
}
