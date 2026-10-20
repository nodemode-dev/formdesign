import '../styles/review.css';
import { editor } from '../editor/editor';
import type { ChangeEvent } from '../engine/client';
import { promptName } from '../ui/dialogs';
import { el, patchList, q, setChecked, setDisabled, setHidden, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Change { story: number; start: number; end: number; kind: 'inserted' | 'deleted'; text: string }
interface Note { story: number; id: number; at: number; author: string; text: string }
interface Condition { name: string; color: number[]; visible: boolean }

const AUTHOR_KEY = 'formdesign.notes.author';
const COLORS = [[230, 90, 60], [60, 140, 230], [70, 170, 90], [200, 150, 30], [150, 90, 200], [30, 160, 170]];

function excerpt(t: string, n = 70): string {
  const s = t.replace(/\s+/g, ' ').trim();
  return s.length > n ? `${s.slice(0, n - 1)}…` : s || '(empty)';
}

export abstract class LivePanel {
  abstract readonly root: HTMLElement;
  protected visible = false;
  private timer = 0;

  constructor(protected readonly ws: Workspace) {
    editor.on('change', (e: ChangeEvent) => { if (e?.docChanged) this.schedule(150); });
    editor.on('selection', () => this.schedule(80));
    editor.on('docs', () => this.schedule(40));
  }

  onShow(): void { this.visible = true; this.schedule(0); }

  protected schedule(ms: number): void {
    if (!this.visible) return;
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => void this.load(), ms);
  }

  protected abstract load(): Promise<void>;

  protected async run(cmd: string, params: Record<string, unknown>): Promise<unknown> {
    const r = await editor.exec(cmd, params);
    this.ws.viewport.refreshOverlays();
    this.schedule(30);
    return r;
  }
}

export class ChangesPanel extends LivePanel {
  readonly root = el(`<div class="frv-panel">
    <div class="frv-bar"><label class="frv-check"><input type="checkbox" class="frv-track"><span>Track changes</span></label><label class="frv-check"><input type="checkbox" class="frv-show"><span>Show in layout</span></label></div>
    <div class="frv-list" role="list" aria-label="Changes"></div>
    <p class="frv-empty" hidden></p>
    <div class="frv-foot"><span class="frv-count"></span><button class="cv-btn frv-btn frv-acceptall" type="button">Accept All</button><button class="cv-btn frv-btn frv-rejectall" type="button">Reject All</button></div>
  </div>`);
  private readonly list = q(this.root, '.frv-list');
  private items: Change[] = [];

  constructor(ws: Workspace) {
    super(ws);
    q<HTMLInputElement>(this.root, '.frv-track').addEventListener('change', (e) => void this.run('changes.track', { on: (e.target as HTMLInputElement).checked }));
    q<HTMLInputElement>(this.root, '.frv-show').addEventListener('change', (e) => {
      ws.viewport.opts.changes = (e.target as HTMLInputElement).checked;
      ws.viewport.invalidateAll();
    });
    q(this.root, '.frv-acceptall').addEventListener('click', () => void this.run('changes.acceptAll', {}));
    q(this.root, '.frv-rejectall').addEventListener('click', () => void this.run('changes.rejectAll', {}));
  }

  protected async load(): Promise<void> {
    if (!editor.hasDoc) { this.items = []; this.render(false); return; }
    const list = await editor.exec<Change[]>('changes.list', {}, true).catch(() => undefined);
    this.items = list ?? [];
    this.render(!!editor.doc?.trackChanges);
  }

  private render(tracking: boolean): void {
    setChecked(q<HTMLInputElement>(this.root, '.frv-track'), tracking);
    setChecked(q<HTMLInputElement>(this.root, '.frv-show'), this.ws.viewport.opts.changes);
    patchList<Change>(this.list, this.items, {
      key: (c) => `${c.story}:${c.start}:${c.kind}`,
      create: (c) => {
        const r = el('<div class="frv-row" role="listitem"><span class="frv-kind"></span><span class="frv-text"></span><span class="frv-acts"><button class="cv-btn frv-mini frv-ok" type="button">Accept</button><button class="cv-btn frv-mini frv-no" type="button">Reject</button></span></div>');
        r.addEventListener('click', (e) => {
          const c2 = this.items.find((x) => `${x.story}:${x.start}:${x.kind}` === r.dataset.key);
          if (!c2) return;
          const t = e.target as HTMLElement;
          if (t.closest('.frv-ok')) void this.run('changes.accept', { story: c2.story, start: c2.start });
          else if (t.closest('.frv-no')) void this.run('changes.reject', { story: c2.story, start: c2.start });
          else void this.run('text.select', { story: c2.story, anchor: c2.start, focus: c2.end });
        });
        this.row(r, c);
        return r;
      },
      update: (r, c) => this.row(r, c),
    });
    const empty = q(this.root, '.frv-empty');
    setHidden(empty, this.items.length > 0);
    setText(empty, !editor.hasDoc ? 'Open a document to review its changes.' : tracking ? 'No changes yet. Edits to text are recorded from now on.' : 'Turn on Track changes to record edits to text.');
    setText(q(this.root, '.frv-count'), this.items.length ? `${this.items.length} change${this.items.length === 1 ? '' : 's'}` : '');
    for (const b of this.root.querySelectorAll<HTMLButtonElement>('.frv-acceptall, .frv-rejectall')) setDisabled(b, !this.items.length);
  }

  private row(r: HTMLElement, c: Change): void {
    const k = q(r, '.frv-kind');
    setText(k, c.kind === 'inserted' ? 'Added' : 'Deleted');
    k.dataset.kind = c.kind;
    setText(q(r, '.frv-text'), excerpt(c.text));
    r.title = c.text;
  }
}

export class NotesPanel extends LivePanel {
  readonly root = el(`<div class="frv-panel">
    <div class="frv-compose"><textarea class="frv-note-in" rows="2" placeholder="Write a note, then Add at the text cursor" aria-label="New note"></textarea>
      <div class="frv-compose-row"><input class="frv-author" type="text" placeholder="Your name" aria-label="Author" spellcheck="false"><button class="cv-btn frv-btn frv-add" type="button">Add Note</button></div></div>
    <div class="frv-list" role="list" aria-label="Notes"></div>
    <p class="frv-empty" hidden></p>
  </div>`);
  private readonly list = q(this.root, '.frv-list');
  private items: Note[] = [];

  constructor(ws: Workspace) {
    super(ws);
    const author = q<HTMLInputElement>(this.root, '.frv-author');
    try { author.value = localStorage.getItem(AUTHOR_KEY) ?? ''; } catch { }
    author.addEventListener('change', () => { try { localStorage.setItem(AUTHOR_KEY, author.value.trim()); } catch { } });
    q(this.root, '.frv-add').addEventListener('click', () => void this.add());
    q<HTMLTextAreaElement>(this.root, '.frv-note-in').addEventListener('keydown', (e) => { if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void this.add(); } });
  }

  protected async load(): Promise<void> {
    this.items = editor.hasDoc ? (await editor.exec<Note[]>('note.list', {}, true).catch(() => undefined)) ?? [] : [];
    this.render();
  }

  private render(): void {
    setDisabled(q<HTMLButtonElement>(this.root, '.frv-add'), !editor.wantsText);
    patchList<Note>(this.list, this.items, {
      key: (n) => `${n.story}:${n.id}`,
      create: (n) => {
        const r = el('<div class="frv-row frv-note" role="listitem"><div class="frv-note-head"><span class="frv-note-author"></span><span class="frv-acts"><button class="cv-btn frv-mini frv-totext" type="button">To Text</button><button class="cv-btn frv-mini frv-del" type="button">Delete</button></span></div><textarea class="frv-note-text" rows="2" aria-label="Note text"></textarea></div>');
        const ta = q<HTMLTextAreaElement>(r, '.frv-note-text');
        ta.addEventListener('change', () => { const it = this.find(r); if (it && ta.value !== it.text) void this.run('note.edit', { story: it.story, id: it.id, text: ta.value }); });
        q(r, '.frv-totext').addEventListener('click', () => { const it = this.find(r); if (it) void this.run('note.convertToText', { story: it.story, id: it.id }); });
        q(r, '.frv-del').addEventListener('click', () => { const it = this.find(r); if (it) void this.run('note.delete', { story: it.story, id: it.id }); });
        this.row(r, n);
        return r;
      },
      update: (r, n) => this.row(r, n),
    });
    const empty = q(this.root, '.frv-empty');
    setHidden(empty, this.items.length > 0);
    setText(empty, editor.hasDoc ? 'No notes. Click into text with the Type tool, write a note above and press Add Note.' : 'Open a document to see its notes.');
  }

  private find(r: HTMLElement): Note | undefined { return this.items.find((n) => `${n.story}:${n.id}` === r.dataset.key); }

  private row(r: HTMLElement, n: Note): void {
    setText(q(r, '.frv-note-author'), n.author || 'Note');
    const ta = q<HTMLTextAreaElement>(r, '.frv-note-text');
    if (document.activeElement !== ta && ta.value !== n.text) ta.value = n.text;
  }

  private async add(): Promise<void> {
    const ta = q<HTMLTextAreaElement>(this.root, '.frv-note-in');
    const text = ta.value.trim();
    if (!text) return;
    if (!editor.wantsText) { toast('Click into text with the Type tool where the note belongs'); return; }
    const author = q<HTMLInputElement>(this.root, '.frv-author').value.trim();
    if (await this.run('note.new', { text, ...(author ? { author } : {}) }) !== undefined) ta.value = '';
  }
}

export class ConditionsPanel extends LivePanel {
  readonly root = el(`<div class="frv-panel">
    <div class="frv-list" role="list" aria-label="Conditions"></div>
    <p class="frv-empty" hidden></p>
    <div class="frv-foot"><span class="frv-count"></span><button class="cv-btn frv-btn frv-clear" type="button">Unconditional</button><button class="cv-btn frv-btn frv-new" type="button">New Condition</button></div>
    <p class="frv-hint">Select text, then click a condition to apply it. The eye shows or hides its text.</p>
  </div>`);
  private readonly list = q(this.root, '.frv-list');
  private items: Condition[] = [];

  constructor(ws: Workspace) {
    super(ws);
    q(this.root, '.frv-new').addEventListener('click', () => void this.create());
    q(this.root, '.frv-clear').addEventListener('click', () => void this.run('condition.apply', { name: null, only: true }));
  }

  protected async load(): Promise<void> {
    this.items = editor.hasDoc ? (await editor.exec<Condition[]>('condition.list', {}, true).catch(() => undefined)) ?? [] : [];
    this.render();
  }

  private render(): void {
    patchList<Condition>(this.list, this.items, {
      key: (c) => c.name,
      create: (c) => {
        const r = el('<div class="frv-row frv-cond" role="listitem"><button class="cv-icon-btn frv-eye" type="button"></button><span class="frv-dot"></span><button class="frv-name" type="button"></button><button class="cv-btn frv-mini frv-del" type="button">Delete</button></div>');
        q(r, '.frv-eye').addEventListener('click', () => { const it = this.items.find((x) => x.name === r.dataset.key); if (it) void this.run('condition.options', { name: it.name, visible: !it.visible }); });
        q(r, '.frv-name').addEventListener('click', () => void this.apply(r.dataset.key as string));
        q(r, '.frv-del').addEventListener('click', () => void this.run('condition.delete', { name: r.dataset.key }));
        this.row(r, c);
        return r;
      },
      update: (r, c) => this.row(r, c),
    });
    const empty = q(this.root, '.frv-empty');
    setHidden(empty, this.items.length > 0);
    setText(empty, editor.hasDoc ? 'No conditions. Conditions show or hide text for versions of one document, such as print and web.' : 'Open a document to see its conditions.');
    setText(q(this.root, '.frv-count'), this.items.length ? `${this.items.length} condition${this.items.length === 1 ? '' : 's'}` : '');
    setDisabled(q<HTMLButtonElement>(this.root, '.frv-clear'), !editor.wantsText);
  }

  private row(r: HTMLElement, c: Condition): void {
    setText(q(r, '.frv-name'), c.name);
    const eye = q<HTMLButtonElement>(r, '.frv-eye');
    const html = c.visible
      ? '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8 12 12.5 8 12.5 1.5 8 1.5 8z"/><circle cx="8" cy="8" r="2"/></svg>'
      : '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4"><path d="M2 2l12 12M6.2 4c.6-.3 1.2-.5 1.8-.5 4 0 6.5 4.5 6.5 4.5s-.6 1.1-1.7 2.2M10 11.9c-.6.4-1.3.6-2 .6C4 12.5 1.5 8 1.5 8s.8-1.4 2.2-2.7"/></svg>';
    if (eye.dataset.on !== String(c.visible)) { eye.dataset.on = String(c.visible); eye.innerHTML = html; }
    eye.title = c.visible ? 'Hide this condition' : 'Show this condition';
    eye.setAttribute('aria-label', eye.title);
    const [rr, g, b] = c.color ?? [128, 128, 128];
    q(r, '.frv-dot').style.setProperty('--frv-dot', `rgb(${rr}, ${g}, ${b})`);
    r.classList.toggle('is-hidden', !c.visible);
  }

  private async apply(name: string): Promise<void> {
    if (!editor.wantsText) { toast('Select text with the Type tool, then click a condition'); return; }
    await this.run('condition.apply', { name });
  }

  private async create(): Promise<void> {
    let i = 1;
    while (this.items.some((c) => c.name === `Condition ${i}`)) i++;
    const name = await promptName('New Condition', `Condition ${i}`, 'Create');
    if (!name) return;
    await this.run('condition.new', { name, color: COLORS[this.items.length % COLORS.length], visible: true });
  }
}

export function installReview(ws: Workspace): void {
  let changes: ChangesPanel | null = null;
  let notes: NotesPanel | null = null;
  let conds: ConditionsPanel | null = null;
  ws.registerPanel({ id: 'changes', label: 'Changes', group: 'review', column: 'side', order: 10, create: () => (changes = new ChangesPanel(ws)).root, onShow: () => changes?.onShow() });
  ws.registerPanel({ id: 'notes', label: 'Notes', group: 'review', column: 'side', order: 20, create: () => (notes = new NotesPanel(ws)).root, onShow: () => notes?.onShow() });
  ws.registerPanel({ id: 'conditions', label: 'Conditions', group: 'review', column: 'side', order: 30, create: () => (conds = new ConditionsPanel(ws)).root, onShow: () => conds?.onShow() });
  ws.addCommands(
    { id: 'ui.changes', label: 'Track Changes Panel', run: () => ws.showPanel('changes') },
    { id: 'ui.notes', label: 'Notes Panel', run: () => ws.showPanel('notes') },
    { id: 'ui.conditions', label: 'Conditional Text Panel', run: () => ws.showPanel('conditions') },
  );
  ws.addMenuItems({ menu: 'Window', items: ['ui.changes', 'ui.notes', 'ui.conditions'], at: 'end' });
}
