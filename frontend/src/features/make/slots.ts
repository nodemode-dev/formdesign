import { api, engine } from '../../engine/client';
import { editor, friendly } from '../../editor/editor';
import { el, patchList, q, setClass, setDisabled, setHidden, setHtml, setText, setTitle } from '../../ui/dom';
import { icon } from '../../ui/icons';
import { toast } from '../../ui/toast';
import type { Workspace } from '../../ui/workspace';
import type { SlotRec } from './api';
import { glyph } from './icons';
import { friendlySlot, madeDocs } from './store';

interface Slot { slot: string; kind: 'text' | 'image' | 'flow'; page: number; id: number; story?: number; text: string; partial: boolean; image: string; frames: number; y: number; x: number }
type Item = { type: 'page'; key: string; page: number } | { type: 'slot'; key: string; s: Slot };

function compact(recs: SlotRec[]): Slot[] {
  const by = new Map<string, Slot>();
  for (const r of recs) {
    const had = by.get(r.slot);
    if (had) { had.frames += 1; continue; }
    const flow = r.slot.startsWith('flow:');
    const kind: Slot['kind'] = flow ? 'flow' : r.kind === 'text' ? 'text' : 'image';
    const text = r.text ?? '';
    const b = r.bounds ?? [0, 0, 0, 0];
    by.set(r.slot, {
      slot: r.slot, kind, page: r.page ?? 0, id: r.id, story: r.story, text, partial: (r.chars ?? text.length) > text.length,
      image: r.image ? r.image.split(/[\\/]/).pop() ?? r.image : '', frames: r.frames && kind !== 'image' ? r.frames : 1, y: b[1], x: b[0],
    });
  }
  return [...by.values()].sort((a, b) => a.page - b.page || a.y - b.y || a.x - b.x || a.slot.localeCompare(b.slot));
}

export class SlotsView {
  readonly root: HTMLElement;
  private readonly list: HTMLElement;
  private readonly filter: HTMLInputElement;
  private slots: Slot[] = [];
  private docUid: number | null = null;
  private stale = true;
  private loaded = false;
  private timer = 0;
  private seq = 0;
  private loading: Promise<void> | null = null;
  private readonly edited = new Map<number, Set<string>>();
  private readonly overset = new Map<number, Set<string>>();

  constructor(private readonly ws: Workspace) {
    this.root = el(`<div class="mk-sl">
      <div class="mk-sl-bar">
        <label class="mk-sl-search">${icon('search', 14)}<input class="mk-sl-filter" type="search" placeholder="Filter slots" aria-label="Filter slots" spellcheck="false"></label>
        <span class="mk-sl-count"></span>
      </div>
      <div class="mk-sl-list"></div>
      <div class="mk-sl-empty" hidden>
        <span class="mk-sl-empty-ico">${glyph('pick', 22)}</span>
        <p class="mk-sl-empty-title"></p>
        <p class="mk-sl-empty-text"></p>
        <button class="cv-btn mk-sl-empty-btn" type="button"></button>
      </div>
    </div>`);
    this.list = q(this.root, '.mk-sl-list');
    this.filter = q<HTMLInputElement>(this.root, '.mk-sl-filter');
    this.filter.addEventListener('input', () => this.render());
    this.filter.addEventListener('keydown', (e) => { if (e.key === 'Escape' && this.filter.value) { e.preventDefault(); this.filter.value = ''; this.render(); } });
    q(this.root, '.mk-sl-empty-btn').addEventListener('click', () => { const c = (q(this.root, '.mk-sl-empty-btn') as HTMLElement).dataset.cmd; if (c) this.ws.commands.run(c); });
    editor.on('change', () => this.schedule(300));
    editor.on('docs', () => { if (editor.doc?.uid !== this.docUid) this.schedule(0); });
    this.render();
  }

  private get visible(): boolean { return this.root.isConnected && !this.root.closest('[hidden]'); }

  private schedule(ms: number): void {
    this.stale = true;
    if (!this.visible) return;
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => void this.refresh(), ms);
  }

  onShow(): void { if (this.stale || !this.loaded) void this.refresh(); }

  async refresh(): Promise<void> {
    const my = ++this.seq;
    const uid = editor.doc?.uid ?? null;
    this.loading = (async () => {
      let slots: Slot[] = [];
      if (uid !== null) {
        try { slots = compact((await api<{ slots: SlotRec[] }>('/slots?full=true')).slots ?? []); } catch { slots = []; }
      }
      if (my !== this.seq) return;
      const switched = uid !== this.docUid;
      this.docUid = uid;
      this.slots = slots;
      this.stale = false;
      this.loaded = true;
      if (switched) { this.filter.value = ''; this.list.scrollTop = 0; }
      this.render();
    })();
    return this.loading;
  }

  private render(): void {
    const f = this.filter.value.trim().toLowerCase();
    const shown = f ? this.slots.filter((s) => s.slot.toLowerCase().includes(f) || friendlySlot(s.slot).includes(f) || s.text.toLowerCase().includes(f) || s.image.toLowerCase().includes(f)) : this.slots;
    const items: Item[] = [];
    let page = -1;
    for (const s of shown) {
      if (s.page !== page) { page = s.page; items.push({ type: 'page', key: `p:${page}`, page }); }
      items.push({ type: 'slot', key: `s:${s.slot}`, s });
    }
    patchList<Item>(this.list, items, {
      key: (i) => i.key,
      create: (i) => (i.type === 'page' ? this.pageNode(i.page) : this.slotNode(i.s)),
      update: (n, i) => { if (i.type === 'page') this.updatePage(n, i.page); else this.updateSlot(n, i.s); },
    });
    const made = this.docUid !== null ? madeDocs.get(this.docUid) : undefined;
    const sample = this.slots.filter((s) => this.isSample(s.slot, made?.report.kept)).length;
    setText(q(this.root, '.mk-sl-count'), this.slots.length ? `${this.slots.length} ${this.slots.length === 1 ? 'slot' : 'slots'}${sample ? ` · ${sample} sample` : ''}` : '');
    setHidden(q(this.root, '.mk-sl-bar'), !this.slots.length);
    const empty = q(this.root, '.mk-sl-empty');
    const none = !editor.hasDoc, noSlots = this.loaded && !this.slots.length, noMatch = !!this.slots.length && !shown.length;
    setHidden(empty, !(none || noSlots || noMatch));
    setClass(empty, 'is-compact', noMatch);
    const btn = q<HTMLButtonElement>(this.root, '.mk-sl-empty-btn');
    if (none) {
      setText(q(empty, '.mk-sl-empty-title'), 'No document open');
      setText(q(empty, '.mk-sl-empty-text'), 'Make a layout from your files, or start from a template, and its slots show here to edit.');
      setText(btn, 'Make a Layout…'); btn.dataset.cmd = 'ui.make';
    } else if (noSlots) {
      setText(q(empty, '.mk-sl-empty-title'), 'No slots in this document');
      setText(q(empty, '.mk-sl-empty-text'), 'Slots are the named frames a template fills: headline, hero picture, body text. Documents made from a template have them.');
      setText(btn, 'New from Template…'); btn.dataset.cmd = 'ui.templates';
    } else if (noMatch) {
      setText(q(empty, '.mk-sl-empty-title'), `No slots match “${this.filter.value.trim()}”`);
      setText(q(empty, '.mk-sl-empty-text'), '');
    }
    setHidden(btn, noMatch);
    setHidden(q(empty, '.mk-sl-empty-text'), noMatch);
  }

  private isSample(slot: string, kept: string[] | undefined): boolean {
    return !!kept?.includes(slot) && !this.edited.get(this.docUid ?? -1)?.has(slot);
  }

  private pageNode(page: number): HTMLElement {
    const n = el('<div class="mk-sl-page"></div>');
    this.updatePage(n, page);
    return n;
  }

  private updatePage(n: HTMLElement, page: number): void {
    const p = editor.layout?.pages.find((x) => x.abs === page);
    setText(n, `Page ${p?.name ?? page + 1}`);
  }

  private slotNode(s: Slot): HTMLElement {
    const n = el(`<div class="mk-sl-row">
      <div class="mk-sl-top">
        <button class="mk-sl-name" type="button"><span class="mk-sl-ico"></span><span class="mk-sl-label"></span></button>
        <span class="mk-sl-tag"></span>
        <span class="mk-sl-saved">${icon('check', 12)}<span>Saved</span></span>
      </div>
      <textarea class="mk-sl-text" rows="1" spellcheck="true"></textarea>
      <div class="mk-sl-img"><span class="mk-sl-file"></span><button class="cv-btn mk-sl-replace" type="button">Replace…</button></div>
    </div>`);
    n.dataset.slot = s.slot;
    const ta = q<HTMLTextAreaElement>(n, '.mk-sl-text');
    q(n, '.mk-sl-name').addEventListener('click', () => this.locate(n.dataset.slot ?? ''));
    ta.addEventListener('focus', () => void this.beginEdit(n, ta));
    ta.addEventListener('input', () => { n.dataset.dirty = '1'; this.fit(ta); });
    ta.addEventListener('blur', () => { void this.commit(n, ta).then(() => { delete n.dataset.editing; this.fit(ta); }); });
    ta.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) { e.preventDefault(); void this.commit(n, ta); }
      else if (e.key === 'Escape') {
        e.preventDefault();
        ta.value = n.dataset.orig ?? ta.value;
        delete n.dataset.dirty;
        ta.blur();
      }
    });
    q(n, '.mk-sl-replace').addEventListener('click', () => void this.replaceImage(n));
    this.updateSlot(n, s);
    return n;
  }

  private updateSlot(n: HTMLElement, s: Slot): void {
    n.dataset.kind = s.kind;
    n.dataset.id = String(s.id);
    n.dataset.page = String(s.page);
    n.dataset.partial = s.partial ? '1' : '';
    setHtml(q(n, '.mk-sl-ico'), glyph(s.kind === 'image' ? 'imageSlot' : s.kind === 'flow' ? 'flowSlot' : 'textSlot', 14), s.kind);
    setText(q(n, '.mk-sl-label'), s.kind === 'flow' ? `${friendlySlot(s.slot)}` : s.slot);
    setTitle(q(n, '.mk-sl-name'), `Select ${s.slot} on the page`);
    const uid = this.docUid ?? -1;
    const made = madeDocs.get(uid);
    const tag = q(n, '.mk-sl-tag');
    let t = '', tone = '';
    if (this.overset.get(uid)?.has(s.slot)) { t = 'Overset'; tone = 'warn'; }
    else if (this.isSample(s.slot, made?.report.kept)) { t = 'Sample text'; tone = 'warn'; }
    else if (made?.report.placeholders?.includes(s.slot) && !this.edited.get(uid)?.has(s.slot)) { t = 'Placeholder'; tone = 'warn'; }
    else if (s.frames > 1) { t = `${s.frames} frames`; }
    setText(tag, t);
    setHidden(tag, !t);
    setClass(tag, 'is-warn', tone === 'warn');
    const ta = q<HTMLTextAreaElement>(n, '.mk-sl-text');
    const img = q(n, '.mk-sl-img');
    setHidden(ta, s.kind === 'image');
    setHidden(img, s.kind !== 'image');
    if (s.kind === 'image') {
      setText(q(n, '.mk-sl-file'), s.image || 'Empty frame');
      setClass(q(n, '.mk-sl-file'), 'is-empty', !s.image);
      setTitle(q(n, '.mk-sl-file'), s.image);
      return;
    }
    ta.placeholder = s.kind === 'flow' ? 'Markdown: blank lines between paragraphs, ## for headings' : 'Empty';
    if (n.dataset.editing || n.dataset.dirty) return;
    const value = s.kind === 'flow' ? s.text.replace(/\n/g, '\n\n') : s.text;
    const shown = s.partial ? `${value}…` : value;
    if (ta.value !== shown) { ta.value = shown; this.fit(ta); }
    n.dataset.orig = shown;
    if (!ta.style.height) requestAnimationFrame(() => this.fit(ta));
  }

  private fit(ta: HTMLTextAreaElement): void {
    if (!ta.isConnected || ta.hidden) return;
    const max = ta.closest('.mk-sl-row')?.getAttribute('data-kind') === 'flow' ? 168 : 120;
    ta.style.height = 'auto';
    const h = Math.min(max, Math.max(26, ta.scrollHeight + 2));
    ta.style.height = `${h}px`;
  }

  private async beginEdit(n: HTMLElement, ta: HTMLTextAreaElement): Promise<void> {
    n.dataset.editing = '1';
    if (n.dataset.partial !== '1' || n.dataset.dirty) return;
    try {
      const r = await engine.call<{ text?: string }>('exec', { command: 'story.get', params: { frame: Number(n.dataset.id) } });
      const full = r.result?.text ?? '';
      if (n.dataset.dirty || !full) return;
      const value = n.dataset.kind === 'flow' ? full.replace(/\n/g, '\n\n') : full;
      const at = ta.selectionStart;
      ta.value = value;
      n.dataset.orig = value;
      n.dataset.partial = '';
      ta.setSelectionRange(Math.min(at, value.length), Math.min(at, value.length));
      this.fit(ta);
    } catch { }
  }

  private async commit(n: HTMLElement, ta: HTMLTextAreaElement): Promise<void> {
    if (!n.dataset.dirty) return;
    const slot = n.dataset.slot ?? '';
    const value = ta.value;
    delete n.dataset.dirty;
    if (value === n.dataset.orig) return;
    n.dataset.orig = value;
    const uid = this.docUid ?? -1;
    try {
      const r = await api<{ set?: string[]; errors?: string[]; overset?: string[] }>('/slots', { json: { values: { [slot]: value } } });
      if (r.errors?.length) { toast(r.errors[0], 'error'); return; }
      let ed = this.edited.get(uid);
      if (!ed) { ed = new Set(); this.edited.set(uid, ed); }
      ed.add(slot);
      let ov = this.overset.get(uid);
      if (!ov) { ov = new Set(); this.overset.set(uid, ov); }
      if (r.overset?.includes(slot)) ov.add(slot); else ov.delete(slot);
      this.flashSaved(n);
      editor.schedule();
      this.schedule(150);
    } catch (err) {
      toast(friendly((err as Error).message), 'error');
    }
  }

  private flashSaved(n: HTMLElement): void {
    const s = q(n, '.mk-sl-saved');
    setClass(s, 'is-on', true);
    window.setTimeout(() => setClass(s, 'is-on', false), 1400);
  }

  private async replaceImage(n: HTMLElement): Promise<void> {
    const files = await this.ws.pickFiles('image/*,.pdf,.svg,.eps', false);
    if (!files.length) return;
    const btn = q<HTMLButtonElement>(n, '.mk-sl-replace');
    setDisabled(btn, true);
    try {
      const [path] = await this.ws.upload(files);
      const slot = n.dataset.slot ?? '';
      const r = await api<{ errors?: string[] }>('/slots', { json: { values: { [slot]: path } } });
      if (r.errors?.length) { toast(r.errors[0], 'error'); return; }
      let ed = this.edited.get(this.docUid ?? -1);
      if (!ed) { ed = new Set(); this.edited.set(this.docUid ?? -1, ed); }
      ed.add(slot);
      this.flashSaved(n);
      editor.schedule();
      this.schedule(150);
    } catch (err) {
      toast(friendly((err as Error).message), 'error');
    } finally {
      setDisabled(btn, false);
    }
  }

  private locate(slot: string): void {
    const s = this.slots.find((x) => x.slot === slot);
    if (!s) return;
    this.ws.viewport.showPage(s.page);
    void editor.exec('selection.set', { ids: [s.id] }, true);
  }

  async reveal(slot: string): Promise<void> {
    this.ws.showPanel('slots');
    if (this.stale || !this.loaded || editor.doc?.uid !== this.docUid) await this.refresh();
    else if (this.loading) await this.loading;
    if (this.filter.value && !this.slots.some((s) => s.slot === slot && s.slot.toLowerCase().includes(this.filter.value.trim().toLowerCase()))) { this.filter.value = ''; this.render(); }
    const row = Array.from(this.list.querySelectorAll<HTMLElement>('.mk-sl-row')).find((r) => r.dataset.slot === slot);
    if (!row) { toast(`${friendlySlot(slot)} is not in this document any more`); return; }
    row.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    setClass(row, 'is-flash', false);
    void row.offsetWidth;
    setClass(row, 'is-flash', true);
    window.setTimeout(() => setClass(row, 'is-flash', false), 1300);
    this.locate(slot);
    const target = row.dataset.kind === 'image' ? q<HTMLElement>(row, '.mk-sl-replace') : q<HTMLElement>(row, '.mk-sl-text');
    target.focus({ preventScroll: true });
  }
}
