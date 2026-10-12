import '../styles/cloud-fonts.css';
import { editor, friendly } from '../editor/editor';
import { api } from '../engine/client';
import { Dialog } from '../ui/dialogs';
import { el, patchList, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Fam { name: string; slug: string; category: string; tags: string[]; active: boolean }
interface Listing { total: number; families: Fam[]; fetched: number; categories: string[]; tags: string[]; count: number }
interface Detail { family: string; slug: string; folder: string; licence: string; licenceUrl: string; designer: string; category: string; added: string; files: Array<{ file: string; style: string; weight: number }>; axes: Array<{ tag: string; min: number; max: number }> }
interface Missing { missing: Array<{ family: string; slug: string; reason: 'used' | 'remembered' }>; unavailable: string[] }

const PAGE = 60;
const REFRESH = '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round"><path d="M13.5 8a5.5 5.5 0 1 1-1.7-4"/><path d="M13.5 2.5v3h-3"/></svg>';
const SAMPLE = 'Sphinx of black quartz, judge my vow 0123456789';
const FONTS_CHANGED = 'formdes:fonts-changed';

export function docKey(): string { return editor.doc?.path ?? ''; }

function cap(s: string): string { return s.replace(/(^|-)([a-z])/g, (_m, a, b) => `${a === '-' ? ' ' : ''}${b.toUpperCase()}`); }

const faces = new Map<string, Promise<string>>();
/** Loads a family's regular file from the engine once and returns the CSS family name that draws it. */
function loadFace(slug: string, name: string): Promise<string> {
  let p = faces.get(slug);
  if (!p) {
    const css = `fdcloud-${slug}`;
    p = new FontFace(css, `url(/api/cloudfonts/preview/${encodeURIComponent(name)})`).load().then((f) => { document.fonts.add(f); return css; });
    p.catch(() => faces.delete(slug));
    faces.set(slug, p);
  }
  return p;
}

class CloudPanel {
  readonly root = el(`<div class="cf">
    <div class="cf-tabs" role="tablist"><button class="cf-tab is-on" type="button" role="tab" aria-selected="true">Cloud</button></div>
    <div class="cf-bar">
      <input class="cf-q cv-input" type="search" placeholder="Search Google Fonts" aria-label="Search Google Fonts" spellcheck="false">
      <button class="cv-icon-btn cf-refresh" type="button" title="Refresh the family index from GitHub" aria-label="Refresh the family index">${REFRESH}</button>
    </div>
    <div class="cf-filters">
      <select class="cf-cat cv-input" aria-label="Category"><option value="">All categories</option></select>
      <select class="cf-tag cv-input" aria-label="Filter"><option value="">Any style</option></select>
      <select class="cf-state cv-input" aria-label="Show"><option value="">All</option><option value="active">Active</option><option value="inactive">Not active</option></select>
    </div>
    <p class="cf-note" role="status"></p>
    <div class="cf-list" role="listbox" aria-label="Google Fonts families"></div>
    <button class="cv-btn cf-more" type="button" hidden>Show more</button>
    <div class="cf-detail" hidden>
      <div class="cf-dhead"><span class="cf-dname"></span><span class="cf-dlic"></span></div>
      <div class="cf-dmeta"></div>
      <input class="cf-sample cv-input" type="text" aria-label="Sample text" spellcheck="false">
      <div class="cf-preview" aria-label="Preview"></div>
      <div class="cf-dfiles"></div>
      <div class="cf-dacts"><a class="cf-dlink" target="_blank" rel="noopener">Licence and files</a><button class="cv-btn cv-btn-primary cf-act" type="button">Activate</button></div>
    </div>
  </div>`);
  private readonly list = q(this.root, '.cf-list');
  private readonly note = q(this.root, '.cf-note');
  private readonly more = q<HTMLButtonElement>(this.root, '.cf-more');
  private readonly search = q<HTMLInputElement>(this.root, '.cf-q');
  private readonly detail = q(this.root, '.cf-detail');
  private readonly act = q<HTMLButtonElement>(this.root, '.cf-act');
  private readonly preview = q(this.root, '.cf-preview');
  private rows: Fam[] = [];
  private total = 0;
  private picked: Fam | null = null;
  private info: Detail | null = null;
  private seq = 0;
  private timer = 0;
  private filled = false;
  private busy = false;

  constructor() {
    const again = () => { window.clearTimeout(this.timer); this.timer = window.setTimeout(() => void this.load(false), 220); };
    this.search.addEventListener('input', again);
    this.search.addEventListener('keydown', (e) => e.stopPropagation());
    for (const s of ['.cf-cat', '.cf-tag', '.cf-state']) q(this.root, s).addEventListener('change', () => void this.load(false));
    q(this.root, '.cf-refresh').addEventListener('click', () => void this.refresh());
    this.more.addEventListener('click', () => void this.load(true));
    this.list.addEventListener('click', (e) => {
      const row = (e.target as HTMLElement).closest<HTMLElement>('[data-key]');
      const f = row ? this.rows.find((x) => x.slug === row.dataset.key) : undefined;
      if (f) void this.pick(f);
    });
    this.act.addEventListener('click', () => void this.toggle());
    const sample = q<HTMLInputElement>(this.root, '.cf-sample');
    sample.value = SAMPLE;
    sample.addEventListener('keydown', (e) => e.stopPropagation());
    sample.addEventListener('input', () => setText(this.preview, sample.value || SAMPLE));
    setText(this.preview, SAMPLE);
  }

  onShow(): void {
    if (!this.rows.length) void this.load(false);
  }

  focusSearch(): void { this.search.focus(); this.search.select(); }

  private param(): URLSearchParams {
    const p = new URLSearchParams({ q: this.search.value.trim(), category: q<HTMLSelectElement>(this.root, '.cf-cat').value, tag: q<HTMLSelectElement>(this.root, '.cf-tag').value, state: q<HTMLSelectElement>(this.root, '.cf-state').value, limit: String(PAGE) });
    return p;
  }

  private fill(sel: HTMLSelectElement, values: string[], fmt: (v: string) => string): void {
    const have = new Set(Array.from(sel.options).map((o) => o.value));
    for (const v of values) if (!have.has(v)) sel.appendChild(new Option(fmt(v), v));
  }

  async load(append: boolean): Promise<void> {
    const mine = ++this.seq;
    const p = this.param();
    if (append) p.set('offset', String(this.rows.length));
    else setText(this.note, this.rows.length ? this.note.textContent : 'Reading the Google Fonts index…');
    try {
      const r = await api<Listing>(`/cloudfonts?${p}`);
      if (mine !== this.seq) return;
      this.rows = append ? [...this.rows, ...r.families] : r.families;
      this.total = r.total;
      if (!this.filled) {
        this.filled = true;
        this.fill(q(this.root, '.cf-cat'), r.categories, cap);
        this.fill(q(this.root, '.cf-tag'), r.tags, (v) => v);
      }
      this.paint(r);
    } catch (err) {
      if (mine === this.seq) setText(this.note, friendly((err as Error).message));
    }
  }

  private paint(r?: Listing): void {
    patchList<Fam>(this.list, this.rows, {
      key: (f) => f.slug,
      create: (f) => {
        const row = el('<div class="cf-row" role="option" tabindex="0"><span class="cf-name"></span><span class="cf-cat-l"></span><span class="cf-on" aria-hidden="true"></span></div>');
        row.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); row.click(); } });
        this.fillRow(row, f);
        return row;
      },
      update: (row, f) => this.fillRow(row, f),
    });
    if (r) {
      const when = new Date(r.fetched * 1000).toLocaleDateString(undefined, { dateStyle: 'medium' });
      setText(this.note, `${this.total} of ${r.count} families. Index from github.com/google/fonts, read ${when}.`);
    }
    setHidden(this.more, this.rows.length >= this.total);
  }

  private fillRow(row: HTMLElement, f: Fam): void {
    setText(q(row, '.cf-name'), f.name);
    setText(q(row, '.cf-cat-l'), cap(f.category));
    setText(q(row, '.cf-on'), f.active ? 'Active' : '');
    setClass(row, 'is-active', f.active);
    setClass(row, 'is-on', this.picked?.slug === f.slug);
    row.setAttribute('aria-selected', String(this.picked?.slug === f.slug));
  }

  private async pick(f: Fam): Promise<void> {
    this.picked = f;
    this.info = null;
    for (const row of Array.from(this.list.children) as HTMLElement[]) { const on = row.dataset.key === f.slug; setClass(row, 'is-on', on); row.setAttribute('aria-selected', String(on)); }
    setHidden(this.detail, false);
    setText(q(this.root, '.cf-dname'), f.name);
    setText(q(this.root, '.cf-dlic'), '');
    setText(q(this.root, '.cf-dmeta'), 'Looking up the family…');
    setText(q(this.root, '.cf-dfiles'), '');
    this.syncAct();
    const mine = f.slug;
    try {
      const d = await api<Detail>(`/cloudfonts/family/${encodeURIComponent(f.name)}`);
      if (this.picked?.slug !== mine) return;
      this.info = d;
      setText(q(this.root, '.cf-dlic'), d.licence);
      setText(q(this.root, '.cf-dmeta'), `${d.designer || 'Unknown designer'} · ${cap(d.category)}${d.axes.length ? ` · variable: ${d.axes.map((a) => a.tag).join(', ')}` : ''}`);
      setText(q(this.root, '.cf-dfiles'), `${d.files.length} font file${d.files.length === 1 ? '' : 's'}: ${d.files.map((x) => `${cap(x.style)} ${x.weight}`).join(', ')}`);
      const a = q<HTMLAnchorElement>(this.root, '.cf-dlink');
      a.href = d.licenceUrl;
      setText(a, `${d.licence}, source and files`);
      this.syncAct();
      const css = await loadFace(mine, f.name);
      if (this.picked?.slug === mine) this.preview.style.fontFamily = `"${css}", sans-serif`;
    } catch (err) {
      if (this.picked?.slug === mine) setText(q(this.root, '.cf-dmeta'), friendly((err as Error).message));
    }
  }

  private syncAct(): void {
    const f = this.picked;
    setText(this.act, this.busy ? (f?.active ? 'Removing…' : 'Downloading…') : f?.active ? 'Deactivate' : 'Activate');
    setDisabled(this.act, !f || this.busy || (!f.active && !this.info));
    setClass(this.act, 'cv-btn-danger', !!f?.active);
    setClass(this.act, 'cv-btn-primary', !f?.active);
  }

  private async toggle(): Promise<void> {
    const f = this.picked;
    if (!f || this.busy) return;
    this.busy = true;
    this.syncAct();
    try {
      if (f.active) {
        const r = await api<{ note: string }>('/cloudfonts/deactivate', { json: { slug: f.slug } });
        toast(`${f.name} deactivated`);
        setText(this.note, r.note);
      } else {
        await activateFamily(f.name);
        toast(`${f.name} activated`);
      }
      f.active = !f.active;
      for (const r of this.rows) if (r.slug === f.slug) r.active = f.active;
      this.paint();
    } catch (err) { toast(friendly((err as Error).message), 'error'); }
    this.busy = false;
    this.syncAct();
  }

  private async refresh(): Promise<void> {
    setText(this.note, 'Refreshing the family index…');
    try {
      const r = await api<{ count: number }>('/cloudfonts/refresh', { method: 'POST' });
      toast(`${r.count} families in the index`);
      await this.load(false);
    } catch (err) { setText(this.note, friendly((err as Error).message)); }
  }
}

/** Downloads and activates a family, then tells the rest of the window the font list changed. */
export async function activateFamily(family: string): Promise<void> {
  await api('/cloudfonts/activate', { json: { family, doc: docKey() } });
  document.dispatchEvent(new CustomEvent(FONTS_CHANGED));
  editor.schedule();
}

/** Offers the Google Fonts families a document needs (or remembers) that are not active. */
function watchMissing(): void {
  const asked = new Set<string>();
  let timer = 0;
  const check = async (): Promise<void> => {
    const d = editor.doc;
    if (!d || asked.has(`${d.uid}`)) return;
    asked.add(`${d.uid}`);
    let r: Missing;
    try { r = await api<Missing>('/cloudfonts/missing', { json: { doc: docKey() } }); } catch { return; }
    if (!r.missing.length) return;
    const dlg = new Dialog('Missing Fonts', 460);
    const body = el(`<div class="cf-miss"><p class="cv-note">${editor.doc?.title ?? 'This document'} uses fonts that are not on this computer, and Google Fonts has them. Activating downloads the files into your Cloud Fonts folder; each family keeps its own licence.</p><div class="cf-miss-list"></div><p class="cf-miss-st" role="status"></p></div>`);
    const list = q(body, '.cf-miss-list');
    r.missing.forEach(() => list.appendChild(el(`<label class="cf-miss-row"><input type="checkbox" checked data-fam=""><span></span><em></em></label>`)));
    Array.from(list.children).forEach((row, i) => {
      const m = r.missing[i];
      q<HTMLInputElement>(row as HTMLElement, 'input').dataset.fam = m.family;
      setText(q(row as HTMLElement, 'span'), m.family);
      setText(q(row as HTMLElement, 'em'), m.reason === 'used' ? 'used in this document' : 'activated for this document before');
    });
    dlg.body.append(body);
    dlg.button('Not Now', 'ghost', () => dlg.close());
    const go = dlg.button('Activate', 'primary', async () => {
      const chosen = Array.from(list.querySelectorAll<HTMLInputElement>('input:checked')).map((i) => i.dataset.fam as string);
      setDisabled(go, true);
      for (const f of chosen) {
        setText(q(body, '.cf-miss-st'), `Downloading ${f}…`);
        try { await activateFamily(f); } catch (err) { toast(friendly((err as Error).message), 'error'); }
      }
      dlg.close();
      if (chosen.length) toast(`${chosen.length} font${chosen.length === 1 ? '' : 's'} activated`);
    });
    dlg.open(go);
  };
  editor.on('docs', () => { window.clearTimeout(timer); timer = window.setTimeout(() => void check(), 600); });
}

export function installCloudFonts(ws: Workspace): void {
  let panel: CloudPanel | null = null;
  ws.registerPanel({ id: 'fonts', label: 'Fonts', group: 'fonts', column: 'side', order: 45, create: () => (panel = new CloudPanel()).root, onShow: () => panel?.onShow() });
  ws.addCommands({ id: 'ui.findMoreFonts', label: 'Find More Fonts…', run: () => { ws.showPanel('fonts'); window.setTimeout(() => panel?.focusSearch(), 50); } });
  ws.addMenuItems({ menu: 'Type', items: ['ui.findMoreFonts'], at: 'end' });
  watchMissing();
}
