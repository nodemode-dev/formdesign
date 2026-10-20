import { editor, type PageInfo } from '../../editor/editor';
import type { ChangeEvent } from '../../engine/client';
import { el, patchList, q, setClass, setHidden, setText, setTitle, swapMedia } from '../../ui/dom';
import type { MenuItem } from '../../ui/menu';
import type { Workspace } from '../../ui/workspace';
import { glyph } from './glyphs';
import {
  Later, Settle, docPages, inspectNow, intersects, isShown, onceLayout, openMenu, pageSize, parentName, parentPrefix,
  setStyle, sizeName, spreadOf, type InspectData, type InspectParent,
} from './shared';

interface PageNode { el: HTMLElement; thumb: HTMLElement; img: HTMLImageElement; label: HTMLElement; badge: HTMLElement; abs: number; dirty: boolean; loading: boolean; seen: boolean }
interface View {
  uid: number; root: HTMLElement; list: HTMLElement; marker: HTMLElement; nodes: Map<number, PageNode>; pages: PageInfo[];
  selected: Set<number>; anchor: number; scroll: number; pending: Set<number>; shown: boolean;
}
interface Remap { uid: number; count: number; from: (abs: number) => number | null }

const SIZES: Record<string, number> = { small: 54, medium: 80, large: 116 };
const LABEL_H = 18;
const ROW_GAP = 10;
const KEY = 'formdesign.pages.thumbs';

function loadSize(): string { try { const v = localStorage.getItem(KEY); return v && SIZES[v] ? v : 'medium'; } catch { return 'medium'; } }

export class PagesPanel {
  readonly root: HTMLElement;
  private readonly scroller: HTMLElement;
  private readonly parentsEl: HTMLElement;
  private readonly parentsMeta: HTMLElement;
  private readonly doneBtn: HTMLButtonElement;
  private readonly viewsEl: HTMLElement;
  private readonly docSec: HTMLElement;
  private readonly empty: HTMLElement;
  private readonly footInfo: HTMLElement;
  private readonly sizeBtn: HTMLButtonElement;
  private readonly delBtn: HTMLButtonElement;
  private readonly views = new Map<number, View>();
  private readonly pagesByUid = new Map<number, PageInfo[]>();
  private active: View | null = null;
  private staging: View | null = null;
  private parents: InspectParent[] = [];
  private facing = true;
  private layoutUid: number | null = null;
  private size = loadSize();
  private scale = 0.1;
  private inflight = 0;
  private remap: Remap | null = null;
  private current = -1;
  private pickedParent: string | null = null;
  private readonly io: IntersectionObserver;
  private readonly later = new Later(() => this.render());
  private readonly later2 = new Later(() => void this.refreshInspect());
  private readonly settle = new Settle(() => this.flushDirty(), 400, 1600);

  constructor(private readonly ws: Workspace) {
    this.root = el(`<div class="pg-panel" tabindex="-1">
<div class="pg-scroll">
<section class="pg-sec pg-parents-sec"><header class="pg-sec-head"><span class="pg-sec-title">Parents</span><span class="pg-sec-meta"></span><button class="pg-chip pg-done" type="button" hidden>Done</button></header><div class="pg-parents"></div></section>
<section class="pg-sec pg-doc-sec"><header class="pg-sec-head"><span class="pg-sec-title">Pages</span><span class="pg-sec-meta pg-doc-meta"></span></header><div class="pg-views"></div></section>
<div class="pg-empty" hidden><span class="pg-empty-icon">${glyph('pageAdd', 22)}</span><span>No document open</span></div>
</div>
<footer class="pg-foot">
<button class="pg-size" type="button" title="Edit page size of the selected pages"><span class="pg-size-label"></span>${glyph('chevronDown', 10)}</button>
<span class="pg-foot-info"></span>
<button class="pg-icon" type="button" data-act="parent" title="New Parent">${glyph('parentAdd', 16)}</button>
<button class="pg-icon" type="button" data-act="insert" title="Insert Page">${glyph('pageAdd', 16)}</button>
<button class="pg-icon" type="button" data-act="delete" title="Delete Selected Pages">${glyph('trash', 16)}</button>
</footer>
</div>`);
    this.scroller = q(this.root, '.pg-scroll');
    this.parentsEl = q(this.root, '.pg-parents');
    this.parentsMeta = q(this.root, '.pg-parents-sec .pg-sec-meta');
    this.doneBtn = q<HTMLButtonElement>(this.root, '.pg-done');
    this.viewsEl = q(this.root, '.pg-views');
    this.docSec = q(this.root, '.pg-doc-sec');
    this.empty = q(this.root, '.pg-empty');
    this.footInfo = q(this.root, '.pg-foot-info');
    this.sizeBtn = q<HTMLButtonElement>(this.root, '.pg-size');
    this.delBtn = q<HTMLButtonElement>(this.root, '[data-act="delete"]');
    this.io = new IntersectionObserver((entries) => this.onSeen(entries), { root: this.scroller, rootMargin: '160px 0px' });
    this.wire();
  }

  private wire(): void {
    editor.on('docs', () => { this.dropClosed(); this.later.run(0); this.later2.run(60); });
    editor.on('layout', () => { this.layoutUid = editor.doc?.uid ?? null; this.later.run(0); this.later2.run(30); });
    editor.on('change', (e: ChangeEvent) => this.onChange(e));
    new ResizeObserver(() => { if (isShown(this.root)) { this.later.run(16); this.later2.run(40); this.pump(); } }).observe(this.root);
    const vp = this.ws.viewport;
    const prev = vp.onView;
    let queued = false;
    vp.onView = () => {
      prev?.();
      if (queued) return;
      queued = true;
      requestAnimationFrame(() => { queued = false; this.syncCurrent(); });
    };
    this.doneBtn.addEventListener('click', () => void this.exitParents());
    this.sizeBtn.addEventListener('click', () => { const r = this.sizeBtn.getBoundingClientRect(); openMenu(this.ws, this.sizeMenu(), r.left, r.top - 6); });
    this.root.querySelector('[data-act="parent"]')?.addEventListener('click', () => this.ws.commands.run('pages.newParent'));
    this.root.querySelector('[data-act="insert"]')?.addEventListener('click', () => this.ws.commands.run('pages.insertOne'));
    this.delBtn.addEventListener('click', () => this.ws.commands.run('pages.delete'));
    this.root.addEventListener('keydown', (e) => this.onKey(e));
    this.root.addEventListener('contextmenu', (e) => this.onContext(e));
    this.scroller.addEventListener('scroll', () => { if (this.active) this.active.scroll = this.scroller.scrollTop; }, { passive: true });
  }

  onShow(): void { this.later.run(0); this.later2.run(0); }

  get uid(): number | null { return editor.doc?.uid ?? null; }
  get view(): View | null { return this.active && this.active.uid === this.uid ? this.active : null; }
  get pages(): PageInfo[] { return this.view?.pages ?? []; }
  get editingParents(): boolean { return !!editor.layout?.editingParents; }
  get parentList(): InspectParent[] { return this.parents; }
  get thumbSize(): string { return this.size; }

  selectedPages(): number[] {
    const v = this.view;
    if (!v) return [];
    const sel = [...v.selected].filter((a) => a < v.pages.length).sort((a, b) => a - b);
    if (sel.length) return sel;
    const cur = this.ws.viewport.currentPage();
    return cur < v.pages.length ? [cur] : [];
  }

  setThumbSize(s: string): void {
    if (!SIZES[s] || s === this.size) return;
    this.size = s;
    try { localStorage.setItem(KEY, s); } catch { }
    this.render();
    for (const v of this.views.values()) for (const n of v.nodes.values()) n.dirty = true;
    this.pump();
  }

  expectRemap(count: number, from: (abs: number) => number | null): void {
    const uid = this.uid;
    if (uid !== null) this.remap = { uid, count, from };
  }

  private dropClosed(): void {
    const open = new Set(editor.docs.map((d) => d.uid));
    for (const [uid, v] of this.views) {
      if (open.has(uid)) continue;
      for (const n of v.nodes.values()) this.io.unobserve(n.thumb);
      v.root.remove();
      this.views.delete(uid);
      this.pagesByUid.delete(uid);
      if (this.active === v) this.active = null;
      if (this.staging === v) this.staging = null;
    }
  }

  private makeView(uid: number): View {
    const root = el('<div class="pg-view" hidden><div class="pg-spreads"></div><div class="pg-marker" hidden></div></div>');
    const v: View = { uid, root, list: q(root, '.pg-spreads'), marker: q(root, '.pg-marker'), nodes: new Map(), pages: [], selected: new Set(), anchor: -1, scroll: 0, pending: new Set(), shown: false };
    this.viewsEl.appendChild(root);
    this.views.set(uid, v);
    return v;
  }

  render(): void {
    const uid = this.uid;
    setHidden(this.empty, uid !== null);
    setHidden(this.docSec, uid === null);
    setHidden(q(this.root, '.pg-parents-sec'), uid === null);
    setClass(this.root, 'is-empty', uid === null);
    if (uid === null) { this.syncFoot(); return; }
    const lay = editor.layout;
    if (!lay || this.layoutUid !== uid) return;
    if (!lay.editingParents) this.pagesByUid.set(uid, lay.pages);
    const pages = this.pagesByUid.get(uid) ?? [];
    let v = this.views.get(uid);
    const fresh = !v;
    if (!v) v = this.makeView(uid);
    this.patchView(v, pages);
    setClass(this.root, 'is-editing-parents', lay.editingParents);
    setHidden(this.doneBtn, !lay.editingParents);
    if (this.active !== v) {
      if (fresh && this.active && isShown(this.root)) void this.stage(v);
      else this.show(v);
    }
    this.syncCurrent();
    this.syncFoot();
    this.patchParents();
    this.pump();
  }

  private show(v: View): void {
    this.staging = null;
    for (const o of this.views.values()) setHidden(o.root, o !== v);
    this.active = v;
    v.shown = true;
    if (this.scroller.scrollTop !== v.scroll) this.scroller.scrollTop = v.scroll;
    for (const n of v.nodes.values()) if (!n.img.getAttribute('src')) n.dirty = true;
  }

  private async stage(v: View): Promise<void> {
    this.staging = v;
    const room = this.scroller.clientHeight + 120;
    const first: PageNode[] = [];
    const rows = new Map<number, PageInfo[]>();
    for (const p of v.pages) rows.set(p.spread, [...(rows.get(p.spread) ?? []), p]);
    let y = this.parentsEl.offsetHeight + 40;
    for (const [, list] of [...rows.entries()].sort((a, b) => a[0] - b[0])) {
      if (y > room) break;
      for (const p of list) { const n = v.nodes.get(p.abs); if (n) first.push(n); }
      y += Math.max(...list.map((p) => (p.rect[3] - p.rect[1]) * this.scale)) + LABEL_H + ROW_GAP;
    }
    const urls = first.map((n) => editor.pageUrl(n.abs, this.side()));
    const probes = urls.map((u) => new Promise<void>((res) => { const im = new Image(); im.decoding = 'async'; im.onload = () => { void im.decode().catch(() => undefined).then(() => res()); }; im.onerror = () => res(); im.src = u; }));
    await Promise.race([Promise.all(probes), new Promise((r) => window.setTimeout(r, 1500))]);
    if (this.staging !== v || this.uid !== v.uid) return;
    first.forEach((n, i) => { n.img.src = urls[i]; n.dirty = false; n.seen = true; });
    this.show(v);
    this.pump();
  }

  private side(): number { return this.size === 'large' ? 240 : 160; }

  private patchView(v: View, pages: PageInfo[]): void {
    const remap = this.remap && this.remap.uid === v.uid && this.remap.count === pages.length ? this.remap : null;
    if (remap) this.remap = null;
    const pool = new Map<number, HTMLImageElement>();
    const used = new Set<HTMLImageElement>();
    if (remap) for (const [abs, n] of v.nodes) pool.set(abs, n.img);
    const structural = pages.length !== v.pages.length || pages.some((p, i) => v.pages[i]?.spread !== p.spread);
    v.pages = pages;
    for (const a of [...v.selected]) if (a >= pages.length) v.selected.delete(a);
    const height = SIZES[this.size];
    let maxH = 1, maxW = 1;
    for (const p of pages) {
      const [w, h] = pageSize(p.rect);
      maxH = Math.max(maxH, h);
      maxW = Math.max(maxW, Math.abs(p.rect[0]) * 2, Math.abs(p.rect[2]) * 2, w);
    }
    const avail = Math.max(120, (this.scroller.clientWidth || 260) - 28);
    this.scale = Math.min(height / maxH, avail / maxW);
    const k = this.scale;
    const bySpread = new Map<number, PageInfo[]>();
    for (const p of pages) { const l = bySpread.get(p.spread) ?? []; l.push(p); bySpread.set(p.spread, l); }
    for (const [abs, n] of v.nodes) if (abs >= pages.length) { this.io.unobserve(n.thumb); n.el.remove(); v.nodes.delete(abs); }
    const spreads = [...bySpread.entries()].sort((a, b) => a[0] - b[0]);
    patchList(v.list, spreads, {
      key: ([s]) => `s${s}`,
      create: ([s, list]) => { const row = el('<div class="pg-spread"></div>'); this.patchSpread(v, row, s, list, k, remap, pool, used); return row; },
      update: (row, [s, list]) => this.patchSpread(v, row, s, list, k, remap, pool, used),
    });
    if (remap) for (const n of v.nodes.values()) n.dirty = true;
    if (structural) for (const n of v.nodes.values()) n.dirty = true;
    setText(q(this.root, '.pg-doc-meta'), pages.length ? `${pages.length} ${pages.length === 1 ? 'page' : 'pages'} · ${spreads.length} ${spreads.length === 1 ? 'spread' : 'spreads'}` : '');
  }

  private patchSpread(v: View, row: HTMLElement, spread: number, list: PageInfo[], k: number, remap: Remap | null, pool: Map<number, HTMLImageElement>, used: Set<HTMLImageElement>): void {
    const sorted = [...list].sort((a, b) => a.index - b.index);
    let hmax = 0;
    const nodes: HTMLElement[] = [];
    for (const p of sorted) {
      let n = v.nodes.get(p.abs);
      if (!n) { n = this.makeNode(p.abs); v.nodes.set(p.abs, n); }
      if (remap) {
        const from = remap.from(p.abs);
        let img = from !== null ? pool.get(from) : undefined;
        if (img && used.has(img)) img = img.cloneNode() as HTMLImageElement;
        const cur = n.thumb.querySelector('img');
        const next = img ?? el<HTMLImageElement>('<img class="pg-img" alt="" draggable="false">');
        used.add(next);
        if (cur !== next) { if (cur && cur.parentNode === n.thumb) cur.remove(); n.thumb.prepend(next); }
        n.img = next;
      }
      const [w, h] = pageSize(p.rect);
      const pw = Math.max(6, Math.round(w * k)), ph = Math.max(6, Math.round(h * k));
      hmax = Math.max(hmax, ph);
      setStyle(n.el, 'left', `calc(50% + ${Math.round(p.rect[0] * k)}px)`);
      setStyle(n.el, 'width', `${pw}px`);
      setStyle(n.thumb, 'height', `${ph}px`);
      setStyle(n.thumb, 'aspect-ratio', `${round2(w)} / ${round2(h)}`);
      const right = p.rect[0] >= -0.5;
      setClass(n.el, 'is-right', right);
      setText(n.label, p.name);
      setText(n.badge, p.parent ?? '');
      setHidden(n.badge, !p.parent);
      setTitle(n.el, `Page ${p.name}${p.parent ? ` · ${this.parentLabel(p.parent)}` : ' · no parent'} · ${sizeName(w, h)}`);
      setClass(n.el, 'is-selected', v.selected.has(p.abs));
      n.el.dataset.abs = String(p.abs);
      nodes.push(n.el);
    }
    setStyle(row, 'height', `${hmax + LABEL_H}px`);
    row.dataset.spread = String(spread);
    let cursor = row.firstElementChild;
    for (const node of nodes) {
      if (node !== cursor) row.insertBefore(node, cursor);
      else cursor = cursor.nextElementSibling;
    }
    while (cursor) { const next = cursor.nextElementSibling; if (!nodes.includes(cursor as HTMLElement)) cursor.remove(); cursor = next; }
  }

  private makeNode(abs: number): PageNode {
    const node = el('<div class="pg-page"><div class="pg-thumb"><img class="pg-img" alt="" draggable="false"><span class="pg-badge"></span></div><span class="pg-label"></span></div>');
    const n: PageNode = { el: node, thumb: q(node, '.pg-thumb'), img: q<HTMLImageElement>(node, '.pg-img'), label: q(node, '.pg-label'), badge: q(node, '.pg-badge'), abs, dirty: true, loading: false, seen: false };
    (n.thumb as HTMLElement & { _pg?: PageNode })._pg = n;
    node.addEventListener('pointerdown', (e) => this.onPageDown(e, node));
    node.addEventListener('dblclick', (e) => { e.preventDefault(); void this.fit(Number(node.dataset.abs)); });
    this.io.observe(n.thumb);
    return n;
  }

  private parentLabel(prefix: string): string {
    return this.parents.find((p) => parentPrefix(p.label) === prefix)?.label ?? `${prefix}-Parent`;
  }

  private onSeen(entries: IntersectionObserverEntry[]): void {
    for (const e of entries) {
      const n = (e.target as HTMLElement & { _pg?: PageNode })._pg;
      if (n) n.seen = e.isIntersecting;
    }
    this.pump();
  }

  private pump(): void {
    const v = this.view;
    if (!v || !v.shown || this.staging || !isShown(this.root)) return;
    for (const n of v.nodes.values()) {
      if (this.inflight >= 3) return;
      if (!n.dirty || n.loading || !n.seen) continue;
      n.dirty = false;
      n.loading = true;
      this.inflight++;
      const img = n.img, url = editor.pageUrl(n.abs, this.side()), uid = v.uid;
      swapMedia(img, url).catch(() => undefined).finally(() => {
        n.loading = false;
        this.inflight--;
        if (this.uid !== uid) n.dirty = true;
        this.pump();
      });
    }
  }

  private onChange(e: ChangeEvent): void {
    if (!e.docChanged) return;
    this.later2.run(220);
    const v = this.view;
    if (!v) return;
    if (e.damage === null || e.damage === undefined) { for (const p of v.pages) v.pending.add(p.abs); }
    else if (e.damage.length) {
      const lay = editor.layout;
      if (lay?.editingParents) {
        const touched = new Set<string>();
        for (const d of e.damage) for (const s of lay.slots) if (intersects(s.bounds, d)) touched.add(parentPrefix(this.parents[spreadOf(s).index]?.label));
        for (const p of v.pages) if (p.parent && touched.has(p.parent)) v.pending.add(p.abs);
      } else {
        for (const d of e.damage) for (const p of v.pages) if (intersects(p.rect, d)) v.pending.add(p.abs);
      }
    }
    if (v.pending.size) this.settle.poke();
  }

  private flushDirty(): void {
    const v = this.view;
    if (!v) return;
    for (const abs of v.pending) { const n = v.nodes.get(abs); if (n) n.dirty = true; }
    v.pending.clear();
    this.pump();
  }

  private async refreshInspect(): Promise<void> {
    if (this.uid === null || !isShown(this.root)) return;
    const d: InspectData | null = await inspectNow();
    if (!d) return;
    this.parents = d.parents ?? [];
    this.facing = d.settings?.facingPages ?? true;
    this.patchParents();
    this.syncFoot();
  }

  private patchParents(): void {
    const editing = this.editingParents;
    const lay = editor.layout;
    let curParent = -1;
    if (editing && lay?.slots.length) curParent = spreadOf(lay.slots[this.ws.viewport.currentSpread()] ?? lay.slots[0]).index;
    const first = this.pages[0];
    const [pw, ph] = first ? pageSize(first.rect) : [612, 792];
    const scale = Math.min(30 / ph, 24 / pw);
    const w = Math.max(10, Math.round(pw * scale)), h = Math.max(10, Math.round(ph * scale));
    const items: Array<{ key: string; label: string; prefix: string | null; index: number; count: number }> = [
      { key: 'none', label: '[None]', prefix: null, index: -1, count: 0 },
      ...this.parents.map((p, i) => ({ key: `p:${p.label}`, label: p.label, prefix: parentPrefix(p.label), index: i, count: p.items })),
    ];
    const used = new Map<string, number>();
    for (const p of this.pages) if (p.parent) used.set(p.parent, (used.get(p.parent) ?? 0) + 1);
    patchList(this.parentsEl, items, {
      key: (it) => it.key,
      create: (it) => {
        const t = el(`<div class="pg-parent" role="button" tabindex="-1"><span class="pg-parent-pages"><span class="pg-mini"><span class="pg-mini-letter"></span></span><span class="pg-mini"><span class="pg-mini-letter"></span></span></span><span class="pg-parent-text"><span class="pg-parent-name"></span><span class="pg-parent-meta"></span></span></div>`);
        t.addEventListener('pointerdown', (e) => this.onParentDown(e, t));
        t.addEventListener('dblclick', () => { const i = Number(t.dataset.index); if (i >= 0) void this.editParent(i); });
        this.patchParent(t, it, w, h, used, curParent);
        return t;
      },
      update: (t, it) => this.patchParent(t, it, w, h, used, curParent),
    });
    setText(this.parentsMeta, editing ? 'Editing' : this.parents.length ? String(this.parents.length) : '');
  }

  private patchParent(t: HTMLElement, it: { key: string; label: string; prefix: string | null; index: number; count: number }, w: number, h: number, used: Map<string, number>, cur: number): void {
    t.dataset.index = String(it.index);
    t.dataset.prefix = it.prefix ?? '';
    setText(q(t, '.pg-parent-name'), it.prefix ? `${it.prefix}-${parentName(it.label)}` : it.label);
    const n = it.prefix ? used.get(it.prefix) ?? 0 : this.pages.filter((p) => !p.parent).length;
    setText(q(t, '.pg-parent-meta'), `${n} ${n === 1 ? 'page' : 'pages'}`);
    const minis = t.querySelectorAll<HTMLElement>('.pg-mini');
    minis.forEach((m, i) => {
      setStyle(m, 'width', `${w}px`);
      setStyle(m, 'height', `${h}px`);
      setHidden(m, i === 1 && !this.facing);
      setText(q(m, '.pg-mini-letter'), it.prefix ?? '');
    });
    setClass(t, 'is-none', !it.prefix);
    setClass(t, 'is-current', this.editingParents && it.index === cur);
    setClass(t, 'is-picked', this.pickedParent === it.key);
    setTitle(t, it.prefix ? `${it.label}: double-click to edit, drag onto a page to apply` : 'No parent: drag onto a page to remove its parent');
  }

  private syncCurrent(): void {
    const v = this.view;
    const cur = v && !this.editingParents ? this.ws.viewport.currentPage() : -1;
    if (v && cur !== this.current) {
      v.nodes.get(this.current)?.el.classList.remove('is-current');
      this.current = cur;
    }
    if (v && cur >= 0) setClass(v.nodes.get(cur)?.el ?? null, 'is-current', true);
    if (this.editingParents) this.patchParents();
    this.syncFoot();
  }

  private syncFoot(): void {
    const v = this.view;
    const pages = v?.pages ?? [];
    const has = !!v && pages.length > 0;
    this.sizeBtn.disabled = !has;
    const sel = has ? this.selectedPages() : [];
    const p = pages[sel[0] ?? -1];
    if (p) { const [w, h] = pageSize(p.rect); setText(q(this.sizeBtn, '.pg-size-label'), sizeName(w, h)); }
    else if (!has) setText(q(this.sizeBtn, '.pg-size-label'), '');
    let info = '', tip = '';
    if (this.editingParents) {
      const lay = editor.layout;
      const i = lay?.slots.length ? spreadOf(lay.slots[this.ws.viewport.currentSpread()] ?? lay.slots[0]).index : -1;
      info = `Editing ${this.parents[i]?.label ?? 'parents'}`;
    } else if (has && !p) {
      return;
    } else if (has && p) {
      const n = v?.selected.size ?? 0;
      info = n > 1 ? `${n} pages selected` : `Page ${p.name} · ${p.parent ? this.parentLabel(p.parent) : 'No parent'}`;
      const spreads = new Set(pages.map((x) => x.spread)).size;
      tip = `${pages.length} ${pages.length === 1 ? 'page' : 'pages'} in ${spreads} ${spreads === 1 ? 'spread' : 'spreads'}`;
    }
    setText(this.footInfo, info);
    setTitle(this.footInfo, tip);
    this.delBtn.disabled = !has || pages.length <= sel.length;
    for (const b of this.root.querySelectorAll<HTMLButtonElement>('.pg-foot .pg-icon:not([data-act="delete"])')) b.disabled = !has;
  }

  private select(abs: number, e: { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean }): void {
    const v = this.view;
    if (!v) return;
    if (e.shiftKey && v.anchor >= 0) {
      v.selected.clear();
      for (let k = Math.min(v.anchor, abs); k <= Math.max(v.anchor, abs); k++) v.selected.add(k);
    } else if (e.metaKey || e.ctrlKey) {
      if (v.selected.has(abs)) v.selected.delete(abs); else v.selected.add(abs);
      v.anchor = abs;
    } else {
      v.selected = new Set([abs]);
      v.anchor = abs;
    }
    for (const [a, n] of v.nodes) setClass(n.el, 'is-selected', v.selected.has(a));
    this.syncFoot();
  }

  selectAbs(list: number[]): void {
    const v = this.view;
    if (!v) return;
    v.selected = new Set(list);
    v.anchor = list[0] ?? -1;
    for (const [a, n] of v.nodes) setClass(n.el, 'is-selected', v.selected.has(a));
    this.syncFoot();
  }

  private async go(abs: number, fit: boolean): Promise<void> {
    if (this.editingParents) { await editor.exec('layout.parents.edit', { on: false }); await onceLayout(); }
    if (fit) this.ws.viewport.fitPage(abs); else this.ws.viewport.showPage(abs);
  }

  private async fit(abs: number): Promise<void> { if (Number.isFinite(abs)) await this.go(abs, true); }

  private async exitParents(): Promise<void> {
    const abs = this.view?.anchor ?? 0;
    await editor.exec('layout.parents.edit', { on: false });
    await onceLayout();
    this.ws.viewport.fitPage(Math.max(0, Math.min(abs, this.pages.length - 1)));
  }

  async editParent(index: number): Promise<void> {
    if (!this.editingParents) { await editor.exec('layout.parents.edit', { on: true }); await onceLayout(); }
    const lay = editor.layout;
    const slot = lay?.slots.findIndex((s) => spreadOf(s).index === index) ?? -1;
    if (slot >= 0) this.ws.viewport.fitSpread(slot);
  }

  private onKey(e: KeyboardEvent): void {
    const t = e.target as HTMLElement;
    if (t.tagName === 'INPUT') return;
    const v = this.view;
    if (!v || !v.pages.length) return;
    const step = (d: number): void => {
      const from = v.anchor >= 0 ? v.anchor : this.ws.viewport.currentPage();
      const to = Math.max(0, Math.min(v.pages.length - 1, from + d));
      this.select(to, { shiftKey: e.shiftKey, metaKey: false, ctrlKey: false });
      if (!e.shiftKey) v.anchor = to;
      void this.go(to, false);
      v.nodes.get(to)?.el.scrollIntoView({ block: 'nearest' });
    };
    if (e.key === 'Delete' || e.key === 'Backspace') { if (v.selected.size) this.ws.commands.run('pages.delete'); }
    else if (e.key === 'ArrowLeft' || e.key === 'ArrowUp') step(-1);
    else if (e.key === 'ArrowRight' || e.key === 'ArrowDown') step(1);
    else if (e.key === 'Enter') { if (v.anchor >= 0) void this.fit(v.anchor); }
    else return;
    e.preventDefault();
    e.stopPropagation();
  }

  private onContext(e: MouseEvent): void {
    e.preventDefault();
    const page = (e.target as HTMLElement).closest<HTMLElement>('.pg-page');
    const v = this.view;
    if (!v) return;
    if (page) {
      const abs = Number(page.dataset.abs);
      if (!v.selected.has(abs)) this.select(abs, { shiftKey: false, metaKey: false, ctrlKey: false });
      openMenu(this.ws, this.contextMenu(), e.clientX, e.clientY);
      return;
    }
    const parent = (e.target as HTMLElement).closest<HTMLElement>('.pg-parent');
    if (parent && Number(parent.dataset.index) >= 0) {
      openMenu(this.ws, ['pages.editParents', 'pages.newParent', '-', 'pages.applyParent'], e.clientX, e.clientY);
      return;
    }
    openMenu(this.ws, ['pages.insert', 'pages.newParent', '-', 'pages.panelSmall', 'pages.panelMedium', 'pages.panelLarge'], e.clientX, e.clientY);
  }

  contextMenu(): MenuItem[] {
    return ['pages.insert', 'pages.duplicateSpread', 'pages.delete', '-', 'pages.applyParent', 'pages.overrideAll', 'pages.removeOverrides', 'pages.detachAll', '-', 'pages.move', 'pages.section', '-', { label: 'Page Size', items: this.sizeMenu() }];
  }

  sizeMenu(): MenuItem[] { return ['pages.size.letter', 'pages.size.legal', 'pages.size.tabloid', '-', 'pages.size.a5', 'pages.size.a4', 'pages.size.a3', 'pages.size.b5', '-', 'pages.size.hd', '-', 'pages.size.custom']; }

  private onPageDown(e: PointerEvent, node: HTMLElement): void {
    if (e.button !== 0) return;
    this.root.focus({ preventScroll: true });
    const v = this.view;
    if (!v) return;
    const abs = Number(node.dataset.abs);
    const x0 = e.clientX, y0 = e.clientY;
    let dragging = false;
    let ghost: HTMLElement | null = null;
    let target: { abs: number; after: boolean } | null = null;
    const moving = v.selected.has(abs) && v.selected.size > 1 ? [...v.selected].sort((a, b) => a - b) : [abs];
    const move = (ev: PointerEvent): void => {
      if (!dragging && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 5) return;
      if (!dragging) {
        dragging = true;
        ghost = el('<div class="pg-ghost"></div>');
        setText(ghost, moving.length > 1 ? `${moving.length} pages` : `Page ${v.pages[abs]?.name ?? ''}`);
        document.body.appendChild(ghost);
        for (const a of moving) v.nodes.get(a)?.el.classList.add('is-dragging');
      }
      if (ghost) { ghost.style.left = `${ev.clientX + 12}px`; ghost.style.top = `${ev.clientY + 10}px`; }
      target = this.dropAt(v, ev.clientX, ev.clientY);
      this.showMarker(v, target);
      this.autoScroll(ev.clientY);
    };
    const up = (ev: PointerEvent): void => {
      document.removeEventListener('pointermove', move, true);
      document.removeEventListener('pointerup', up, true);
      ghost?.remove();
      setHidden(v.marker, true);
      for (const n of v.nodes.values()) n.el.classList.remove('is-dragging');
      if (!dragging) {
        this.select(abs, ev);
        if (!ev.shiftKey && !ev.metaKey && !ev.ctrlKey) void this.go(abs, false);
        return;
      }
      if (target) void this.movePages(moving, target.abs + (target.after ? 1 : 0));
    };
    document.addEventListener('pointermove', move, true);
    document.addEventListener('pointerup', up, true);
  }

  private onParentDown(e: PointerEvent, tile: HTMLElement): void {
    if (e.button !== 0) return;
    const v = this.view;
    const x0 = e.clientX, y0 = e.clientY;
    let dragging = false;
    let ghost: HTMLElement | null = null;
    let over: HTMLElement | null = null;
    const prefix = tile.dataset.prefix || null;
    const move = (ev: PointerEvent): void => {
      if (!dragging && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 5) return;
      if (!dragging) { dragging = true; ghost = el('<div class="pg-ghost"></div>'); setText(ghost, prefix ? `Apply ${prefix}-Parent` : 'Remove parent'); document.body.appendChild(ghost); }
      if (ghost) { ghost.style.left = `${ev.clientX + 12}px`; ghost.style.top = `${ev.clientY + 10}px`; }
      const hit = (document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null)?.closest<HTMLElement>('.pg-page') ?? null;
      const page = hit && v?.root.contains(hit) ? hit : null;
      if (page !== over) { over?.classList.remove('is-drop'); over = page; over?.classList.add('is-drop'); }
      this.autoScroll(ev.clientY);
    };
    const up = (): void => {
      document.removeEventListener('pointermove', move, true);
      document.removeEventListener('pointerup', up, true);
      ghost?.remove();
      over?.classList.remove('is-drop');
      if (!dragging) {
        this.pickedParent = tile.dataset.index === '-1' ? 'none' : `p:${this.parents[Number(tile.dataset.index)]?.label}`;
        this.patchParents();
        return;
      }
      if (over && v) {
        const abs = Number(over.dataset.abs);
        const pages = v.selected.has(abs) ? [...v.selected].sort((a, b) => a - b) : [abs];
        void editor.exec('layout.pages.applyParent', { pages, parent: prefix });
      }
    };
    document.addEventListener('pointermove', move, true);
    document.addEventListener('pointerup', up, true);
  }

  private dropAt(v: View, x: number, y: number): { abs: number; after: boolean } | null {
    let best: { abs: number; after: boolean } | null = null;
    let bd = Infinity;
    for (const [abs, n] of v.nodes) {
      const r = n.thumb.getBoundingClientRect();
      const dx = x < r.left ? r.left - x : x > r.right ? x - r.right : 0;
      const dy = y < r.top ? r.top - y : y > r.bottom + LABEL_H ? y - r.bottom - LABEL_H : 0;
      const d = dx * dx + dy * dy * 4;
      if (d < bd) { bd = d; best = { abs, after: x > (r.left + r.right) / 2 }; }
    }
    return best;
  }

  private showMarker(v: View, t: { abs: number; after: boolean } | null): void {
    const n = t ? v.nodes.get(t.abs) : null;
    if (!t || !n) { setHidden(v.marker, true); return; }
    const host = v.root.getBoundingClientRect(), r = n.thumb.getBoundingClientRect();
    const x = (t.after ? r.right + 3 : r.left - 3) - host.left;
    setStyle(v.marker, 'left', `${Math.round(x - 1)}px`);
    setStyle(v.marker, 'top', `${Math.round(r.top - host.top - 4)}px`);
    setStyle(v.marker, 'height', `${Math.round(r.height + 8)}px`);
    setHidden(v.marker, false);
  }

  private autoScroll(y: number): void {
    const r = this.scroller.getBoundingClientRect();
    if (y < r.top + 28) this.scroller.scrollTop -= 12;
    else if (y > r.bottom - 28) this.scroller.scrollTop += 12;
  }

  async movePages(moving: number[], gap: number): Promise<void> {
    const v = this.view;
    if (!v) return;
    const n = v.pages.length;
    const set = new Set(moving);
    const rest = [...Array(n).keys()].filter((a) => !set.has(a));
    const at = gap - moving.filter((a) => a < gap).length;
    const desired = [...rest.slice(0, at), ...moving, ...rest.slice(at)];
    const arr = [...Array(n).keys()];
    const cmds: Array<{ command: string; params: Record<string, unknown> }> = [];
    for (let pos = 0; pos < n; pos++) {
      if (arr[pos] === desired[pos]) continue;
      const from = arr.indexOf(desired[pos]);
      cmds.push({ command: 'layout.pages.move', params: { from, to: pos } });
      const [x] = arr.splice(from, 1);
      arr.splice(pos, 0, x);
    }
    if (!cmds.length) return;
    this.expectRemap(n, (abs) => desired[abs] ?? null);
    const r = await editor.batch(cmds);
    if (r === undefined) { this.remap = null; return; }
    await onceLayout();
    this.selectAbs(moving.map((a) => desired.indexOf(a)));
  }
}

function round2(n: number): string { return String(Math.round(n * 100) / 100); }
