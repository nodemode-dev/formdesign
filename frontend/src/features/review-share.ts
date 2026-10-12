import '../styles/review-tools.css';
import { editor, friendly } from '../editor/editor';
import { api } from '../engine/client';
import { Dialog } from '../ui/dialogs';
import { el, patchList, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';
import { LivePanel } from './review';

interface Reply { name: string; text: string; at: number; owner: boolean }
interface Comment { id: number; page: number; x: number; y: number; name: string; text: string; at: number; resolved: boolean; replies: Reply[]; placed: boolean; note: { story: number; id: number } | null }
interface Status { shared: boolean; token?: string; slug?: string; url?: string; title?: string; pages?: number; updated?: number; comments: Comment[] }

const OWNER_KEY = 'formdes.notes.author';
const COPY = '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4"><rect x="5.5" y="5.5" width="8" height="8" rx="1.5"/><path d="M10.5 5.5v-2a1.5 1.5 0 0 0-1.5-1.5H4A1.5 1.5 0 0 0 2.5 3.5V9A1.5 1.5 0 0 0 4 10.5h1.5"/></svg>';
const OPEN = '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M9 2.5h4.5V7M13.5 2.5 7.5 8.5M12 9.5v3a1 1 0 0 1-1 1H3.5a1 1 0 0 1-1-1V5a1 1 0 0 1 1-1h3"/></svg>';

function owner(): string {
  try { return localStorage.getItem(OWNER_KEY) || ''; } catch { return ''; }
}

function when(t: number): string {
  return new Date(t * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
}

/** The shared state of the open document's review, read by the dialog and the panel and refreshed by either. */
const shared: { status: Status; listeners: Array<() => void> } = { status: { shared: false, comments: [] }, listeners: [] };

async function load(): Promise<Status> {
  const s = editor.hasDoc ? await api<Status>('/review').catch(() => undefined) : undefined;
  const next = s ?? { shared: false, comments: [] };
  if (JSON.stringify(next) !== JSON.stringify(shared.status)) {
    shared.status = next;
    for (const l of shared.listeners) l();
  }
  return shared.status;
}

class ShareDialog {
  private readonly d = new Dialog('Share for Review', 540);
  private readonly intro = el('<p class="cv-note">Publishes this document as a page-by-page review site served from this computer. A reviewer opens the link in a browser, clicks a page to pin a comment with their name, and each comment arrives here as a Note at that spot. Only people on this computer can open the link.</p>');
  private readonly link = el(`<div class="frs-link" hidden><input class="frs-url" type="text" readonly spellcheck="false" aria-label="Review link"><button class="cv-icon-btn frs-copy" type="button" title="Copy the link" aria-label="Copy the link">${COPY}</button><button class="cv-icon-btn frs-open" type="button" title="Open in the browser" aria-label="Open in the browser">${OPEN}</button></div>`);
  private readonly meta = el('<p class="cv-note frs-meta" aria-live="polite"></p>');
  private readonly go: HTMLButtonElement;
  private readonly stop: HTMLButtonElement;
  private busy = false;

  constructor(private readonly ws: Workspace) {
    this.d.body.append(this.intro, this.link, this.meta);
    this.stop = this.d.button('Stop Sharing', 'danger', () => void this.stopSharing());
    this.d.button('Close', 'ghost', () => this.d.close());
    this.go = this.d.button('Share', 'primary', () => void this.share());
    q(this.link, '.frs-copy').addEventListener('click', () => { const v = q<HTMLInputElement>(this.link, '.frs-url').value; void navigator.clipboard?.writeText(v).then(() => toast('Link copied'), () => toast('Could not copy the link', 'error')); });
    q(this.link, '.frs-open').addEventListener('click', () => { const v = q<HTMLInputElement>(this.link, '.frs-url').value; if (v) void api('/system/open', { json: { url: v } }).catch(() => window.open(v, '_blank', 'noopener')); });
    shared.listeners.push(() => this.paint());
  }

  async open(): Promise<void> {
    if (!editor.hasDoc) { toast('Open a document to share it'); return; }
    this.d.open(this.go);
    await load();
    this.paint();
  }

  private paint(): void {
    const s = shared.status;
    setHidden(this.link, !s.shared);
    const url = q<HTMLInputElement>(this.link, '.frs-url');
    if (s.url && url.value !== s.url) url.value = s.url;
    setText(this.go, s.shared ? 'Update Pages' : 'Share');
    setHidden(this.stop, !s.shared);
    setDisabled(this.go, this.busy);
    const n = s.comments.length;
    setText(this.meta, s.shared ? `${s.pages ?? 0} page${s.pages === 1 ? '' : 's'} shared${s.updated ? `, updated ${when(s.updated)}` : ''}. ${n} comment${n === 1 ? '' : 's'} so far. Update Pages after you change the layout so reviewers see the new pages; comments stay.` : 'Not shared yet.');
  }

  private async share(): Promise<void> {
    if (this.busy) return;
    this.busy = true;
    setDisabled(this.go, true);
    setText(this.meta, 'Rendering the pages…');
    try {
      shared.status = await api<Status>('/review/share', { method: 'POST' });
      for (const l of shared.listeners) l();
      this.ws.showPanel('review');
      toast('Review link ready');
    } catch (err) { toast(friendly((err as Error).message), 'error'); }
    this.busy = false;
    this.paint();
  }

  private async stopSharing(): Promise<void> {
    try { await api('/review/stop', { method: 'POST' }); await load(); toast('Review site taken down'); } catch (err) { toast(friendly((err as Error).message), 'error'); }
    this.paint();
  }
}

export class ReviewPanel extends LivePanel {
  readonly root = el(`<div class="frv-panel frs-panel">
    <div class="frs-top"><span class="frs-state"></span><button class="cv-btn frv-btn frs-share" type="button">Share…</button></div>
    <div class="frv-list frs-list" role="list" aria-label="Review comments"></div>
    <p class="frv-empty frs-empty" hidden></p>
    <div class="frv-foot"><label class="frv-check"><input type="checkbox" class="frs-showres"><span>Show resolved</span></label><span class="frv-count"></span></div>
  </div>`);
  private readonly list = q(this.root, '.frs-list');
  private poll = 0;

  constructor(ws: Workspace, private readonly dialog: () => ShareDialog) {
    super(ws);
    q(this.root, '.frs-share').addEventListener('click', () => void this.dialog().open());
    q<HTMLInputElement>(this.root, '.frs-showres').addEventListener('change', () => this.render());
    shared.listeners.push(() => this.render());
    this.list.addEventListener('click', (e) => void this.click(e));
    this.list.addEventListener('keydown', (e) => {
      const t = e.target as HTMLElement;
      if (e.key === 'Enter' && t.classList.contains('frs-reply-in')) { e.preventDefault(); const row = t.closest<HTMLElement>('[data-key]'); if (row) void this.sendReply(row); }
    });
  }

  override onShow(): void {
    super.onShow();
    if (!this.poll) this.poll = window.setInterval(() => { if (this.root.isConnected && this.root.offsetParent !== null && !document.hidden) this.schedule(0); }, 3000);
  }

  protected async load(): Promise<void> {
    await load();
    this.render();
  }

  private find(row: HTMLElement): Comment | undefined {
    return shared.status.comments.find((c) => String(c.id) === row.dataset.key);
  }

  private render(): void {
    const s = shared.status;
    const showRes = q<HTMLInputElement>(this.root, '.frs-showres').checked;
    const items = s.comments.filter((c) => showRes || !c.resolved).sort((a, b) => a.page - b.page || a.id - b.id);
    setText(q(this.root, '.frs-state'), !editor.hasDoc ? 'No document open' : s.shared ? 'Shared for review' : 'Not shared');
    setClass(q(this.root, '.frs-state'), 'is-on', !!s.shared);
    patchList<Comment>(this.list, items, {
      key: (c) => String(c.id),
      create: (c) => {
        const r = el(`<div class="frv-row frs-row" role="listitem" tabindex="0"><div class="frs-head"><span class="frs-pin"></span><span class="frs-who"></span><span class="frs-acts"><button class="cv-btn frv-mini frs-reply" type="button">Reply</button><button class="cv-btn frv-mini frs-resolve" type="button"></button><button class="cv-btn frv-mini frs-del" type="button">Delete</button></span></div><div class="frs-text"></div><div class="frs-thread"></div><div class="frs-compose" hidden><input class="frs-reply-in" type="text" maxlength="2000" placeholder="Write a reply" aria-label="Reply"><button class="cv-btn frv-mini frs-send" type="button">Send</button></div></div>`);
        this.row(r, c);
        return r;
      },
      update: (r, c) => this.row(r, c),
    });
    const empty = q(this.root, '.frs-empty');
    setHidden(empty, items.length > 0);
    setText(empty, !editor.hasDoc ? 'Open a document to review it.' : s.shared ? 'No comments yet. Reviewers pin comments on the pages at the link; they appear here and in the Notes panel as they arrive.' : 'Use File > Share for Review to publish a review link.');
    const open = s.comments.filter((c) => !c.resolved).length;
    setText(q(this.root, '.frv-count'), s.comments.length ? `${open} open, ${s.comments.length - open} resolved` : '');
  }

  private row(r: HTMLElement, c: Comment): void {
    setText(q(r, '.frs-pin'), String(c.id));
    setText(q(r, '.frs-who'), `${c.name}, page ${c.page + 1}, ${when(c.at)}${c.placed ? '' : ', not placed'}`);
    setText(q(r, '.frs-text'), c.text);
    setText(q(r, '.frs-resolve'), c.resolved ? 'Reopen' : 'Resolve');
    setClass(r, 'is-resolved', c.resolved);
    const th = q(r, '.frs-thread');
    const list = c.replies.map((p, i) => ({ ...p, i }));
    patchList<(typeof list)[number]>(th, list, {
      key: (p) => String(p.i),
      create: (p) => { const d = el('<div class="frs-rep"><span class="frs-rwho"></span><span class="frs-rtxt"></span></div>'); this.fillReply(d, p); return d; },
      update: (d, p) => this.fillReply(d, p),
    });
  }

  private fillReply(d: HTMLElement, p: Reply): void {
    setText(q(d, '.frs-rwho'), `${p.name}${p.owner ? ' (you)' : ''}`);
    setText(q(d, '.frs-rtxt'), p.text);
  }

  private async click(e: MouseEvent): Promise<void> {
    const t = e.target as HTMLElement;
    const row = t.closest<HTMLElement>('[data-key]');
    const c = row ? this.find(row) : undefined;
    if (!row || !c) return;
    try {
      if (t.closest('.frs-reply')) {
        const box = q(row, '.frs-compose');
        setHidden(box, !box.hidden);
        if (!box.hidden) q<HTMLInputElement>(box, '.frs-reply-in').focus();
      } else if (t.closest('.frs-send')) await this.sendReply(row);
      else if (t.closest('.frs-resolve')) { await api(`/review/comments/${c.id}/resolve`, { json: { resolved: !c.resolved } }); await this.load(); }
      else if (t.closest('.frs-del')) { await api(`/review/comments/${c.id}/delete`, { method: 'POST' }); editor.refreshMeta(); await this.load(); }
      else if (!t.closest('.frs-compose')) await this.reveal(c);
    } catch (err) { toast(friendly((err as Error).message), 'error'); }
  }

  private async sendReply(row: HTMLElement): Promise<void> {
    const c = this.find(row);
    const input = q<HTMLInputElement>(row, '.frs-reply-in');
    const text = input.value.trim();
    if (!c || !text) return;
    try {
      await api(`/review/comments/${c.id}/reply`, { json: { text, name: owner() } });
      input.value = '';
      setHidden(q(row, '.frs-compose'), true);
      editor.refreshMeta();
      await this.load();
    } catch (err) { toast(friendly((err as Error).message), 'error'); }
  }

  private async reveal(c: Comment): Promise<void> {
    this.ws.viewport.showPage(c.page);
    if (!c.note) return;
    const list = await editor.exec<Array<{ story: number; id: number; at: number }>>('note.list', {}, true).catch(() => undefined);
    const n = list?.find((x) => x.story === c.note?.story && x.id === c.note?.id);
    if (n) await editor.exec('text.select', { story: n.story, anchor: n.at, focus: n.at }, true).catch(() => undefined);
    this.ws.viewport.refreshOverlays();
  }
}

export function installReviewShare(ws: Workspace): void {
  let dialog: ShareDialog | null = null;
  const dlg = () => (dialog ??= new ShareDialog(ws));
  let panel: ReviewPanel | null = null;
  ws.registerPanel({ id: 'review', label: 'Review', group: 'review', column: 'side', order: 25, create: () => (panel = new ReviewPanel(ws, dlg)).root, onShow: () => panel?.onShow() });
  ws.addCommands(
    { id: 'ui.shareReview', label: 'Share for Review…', enabled: () => editor.hasDoc, run: () => void dlg().open() },
    { id: 'ui.review', label: 'Review Panel', run: () => ws.showPanel('review') },
  );
  ws.addMenuItems({ menu: 'Window', items: ['ui.review'], at: 'end' });
  ws.addMenuItems({ menu: 'File', items: ['ui.shareReview', 'ui.importPdfComments'], at: 'start' });
}
