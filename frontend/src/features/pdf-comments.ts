import '../styles/review-tools.css';
import { editor, friendly } from '../editor/editor';
import { api } from '../engine/client';
import { Dialog } from '../ui/dialogs';
import { el, patchList, q, setDisabled, setHidden, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Imported { page: number; kind: string; author: string; date: string; text: string; quote: string; story: number; id: number; at: number; snapped: boolean }
interface Skipped { page: number; kind: string; author: string; reason: string; text: string }
interface Result { file: string; pages: number; found: number; imported: Imported[]; skipped: Skipped[] }

/** File > Import PDF Comments: one dialog, built once, whose two lists are patched in place for each import. */
class PdfCommentsDialog {
  private readonly d = new Dialog('Import PDF Comments', 640);
  private readonly sum = el('<p class="frt-sum" aria-live="polite"></p>');
  private readonly doneHead = el('<h3 class="frt-h">Imported as Notes</h3>');
  private readonly done = el('<ul class="frt-list" aria-label="Imported comments"></ul>');
  private readonly doneNone = el('<p class="cv-note frt-none">Nothing was imported.</p>');
  private readonly skipHead = el('<h3 class="frt-h">Could not be placed</h3>');
  private readonly skip = el('<ul class="frt-list" aria-label="Comments that could not be placed"></ul>');
  private readonly busy = el('<p class="cv-note frt-busy" hidden>Reading the PDF and finding the text under each comment…</p>');
  private readonly notes: HTMLButtonElement;
  private readonly again: HTMLButtonElement;
  private running = false;

  constructor(private readonly ws: Workspace) {
    q<HTMLElement>(this.d.scrim, '.cv-dialog').classList.add('frt-dialog');
    this.d.body.append(this.busy, this.sum, this.doneHead, this.done, this.doneNone, this.skipHead, this.skip);
    this.notes = this.d.button('Show Notes', 'ghost', () => { this.d.close(); ws.showPanel('notes'); });
    this.again = this.d.button('Choose Another PDF…', 'ghost', () => void this.run());
    this.d.button('Done', 'primary', () => this.d.close());
    this.done.addEventListener('click', (e) => {
      const li = (e.target as HTMLElement).closest<HTMLElement>('[data-key]');
      if (li) void this.reveal(li);
    });
    this.show(null);
  }

  async run(): Promise<void> {
    if (this.running) return;
    if (!editor.hasDoc) { toast('Open the document the PDF was exported from'); return; }
    const files = await this.ws.pickFiles('.pdf,application/pdf', false);
    if (!files.length) return;
    this.running = true;
    setHidden(this.busy, false);
    setDisabled(this.again, true);
    this.show(null);
    this.d.open(this.again);
    try {
      const [path] = await this.ws.upload(files);
      const r = await api<Result>('/review/pdf-comments', { json: { path } });
      editor.refreshMeta();
      this.ws.viewport.refreshOverlays();
      this.show(r);
    } catch (err) {
      this.d.close();
      toast(friendly((err as Error).message), 'error');
    } finally {
      this.running = false;
      setHidden(this.busy, true);
      setDisabled(this.again, false);
    }
  }

  private show(r: Result | null): void {
    const imported = r?.imported ?? [];
    const skipped = r?.skipped ?? [];
    setText(this.sum, r ? `${r.file}: ${r.found} comment${r.found === 1 ? '' : 's'} found on ${r.pages} page${r.pages === 1 ? '' : 's'}; ${imported.length} imported, ${skipped.length} not placed.` : '');
    setHidden(this.sum, !r);
    setHidden(this.doneHead, !r);
    setHidden(this.doneNone, !r || imported.length > 0);
    setHidden(this.skipHead, !skipped.length);
    setHidden(this.skip, !skipped.length);
    setDisabled(this.notes, !imported.length);
    patchList<Imported>(this.done, imported, {
      key: (n) => `${n.story}:${n.id}`,
      create: (n) => {
        const li = el('<li class="frt-item" tabindex="0" role="button"><span class="frt-badge"></span><span class="frt-main"><span class="frt-who"></span><span class="frt-txt"></span></span></li>');
        li.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); void this.reveal(li); } });
        this.fillDone(li, n);
        return li;
      },
      update: (li, n) => this.fillDone(li, n),
    });
    patchList<Skipped & { n: number }>(this.skip, skipped.map((s, n) => ({ ...s, n })), {
      key: (s) => String(s.n),
      create: (s) => {
        const li = el('<li class="frt-item is-skip"><span class="frt-badge"></span><span class="frt-main"><span class="frt-who"></span><span class="frt-txt"></span></span></li>');
        this.fillSkip(li, s);
        return li;
      },
      update: (li, s) => this.fillSkip(li, s),
    });
  }

  private fillDone(li: HTMLElement, n: Imported): void {
    setText(q(li, '.frt-badge'), `p${n.page}`);
    setText(q(li, '.frt-who'), `${n.kind}${n.author ? `, ${n.author}` : ''}${n.date ? `, ${n.date}` : ''}${n.snapped ? ', placed at the nearest text' : ''}`);
    const body = n.text.split('\n').slice(0, -1).join(' ').slice(0, 220);
    setText(q(li, '.frt-txt'), body);
    li.title = 'Go to this note';
  }

  private fillSkip(li: HTMLElement, s: Skipped): void {
    setText(q(li, '.frt-badge'), `p${s.page}`);
    setText(q(li, '.frt-who'), `${s.kind}${s.author ? `, ${s.author}` : ''}`);
    setText(q(li, '.frt-txt'), `${s.reason}${s.text ? `: ${s.text}` : ''}`);
  }

  private async reveal(li: HTMLElement): Promise<void> {
    const [story, id] = (li.dataset.key ?? '').split(':').map(Number);
    const list = await editor.exec<Array<{ story: number; id: number; at: number }>>('note.list', {}, true).catch(() => undefined);
    const n = list?.find((x) => x.story === story && x.id === id);
    if (!n) return;
    await editor.exec('text.select', { story, anchor: n.at, focus: n.at }, true).catch(() => undefined);
    this.ws.viewport.refreshOverlays();
  }
}

export function installPdfComments(ws: Workspace): void {
  let dlg: PdfCommentsDialog | null = null;
  ws.addCommands({ id: 'ui.importPdfComments', label: 'Import PDF Comments…', enabled: () => editor.hasDoc, run: () => void (dlg ??= new PdfCommentsDialog(ws)).run() });
}
