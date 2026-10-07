import { editor } from '../../editor/editor';
import { engine, type ChangeEvent } from '../../engine/client';
import { el, patchList, q, setClass, setHidden, setText, setTitle } from '../../ui/dom';
import type { Workspace } from '../../ui/workspace';
import { glyph, kindGlyph } from './glyphs';
import { Later, inspectNow, isShown, openMenu, rangeText, rgb, setStyle, spreadOf, words, type InspectData, type InspectItem, type InspectLayer } from './shared';

interface Row { item: InspectItem; depth: number; open: boolean; group: boolean }

const KIND_LABEL: Record<string, string> = { text: 'Text Frame', image: 'Graphic', rect: 'Rectangle', ellipse: 'Ellipse', line: 'Line', polygon: 'Polygon', group: 'Group', frame: 'Frame', table: 'Table', object: 'Object' };

function flags(): string {
  return `<button class="ly-eye" type="button" title="Show or hide (Alt: hide the others)"><span class="ly-on">${glyph('eye', 14)}</span><span class="ly-off">${glyph('eyeOff', 14)}</span></button><button class="ly-lock" type="button" title="Lock or unlock (Alt: lock the others)"><span class="ly-on">${glyph('lock', 13)}</span><span class="ly-off">${glyph('unlock', 13)}</span></button>`;
}

export class LayersPanel {
  readonly root: HTMLElement;
  private readonly list: HTMLElement;
  private readonly scroller: HTMLElement;
  private readonly marker: HTMLElement;
  private readonly empty: HTMLElement;
  private readonly note: HTMLElement;
  private readonly footInfo: HTMLElement;
  private readonly delBtn: HTMLButtonElement;
  private data: InspectData | null = null;
  private dataUid = -1;
  private slots = new Map<number, string>();
  private files = new Map<number, string>();
  private slotsRev = -1;
  private collapsed = new Map<number, Set<number>>();
  private openGroups = new Map<number, Set<number>>();
  private renaming: HTMLInputElement | null = null;
  private spread = -1;
  private readonly later = new Later(() => void this.refresh());
  private readonly patchSoon = new Later(() => this.patch());

  constructor(private readonly ws: Workspace) {
    this.root = el(`<div class="ly-panel" tabindex="-1">
<div class="ly-scroll"><div class="ly-list"></div><p class="ly-note" hidden></p><div class="ly-marker" hidden></div>
<div class="pg-empty ly-empty" hidden><span class="pg-empty-icon">${glyph('layers', 22)}</span><span>No document open</span></div></div>
<footer class="pg-foot ly-foot"><span class="pg-foot-info"></span>
<button class="pg-icon" type="button" data-act="new" title="Create New Layer">${glyph('layerAdd', 16)}</button>
<button class="pg-icon" type="button" data-act="delete" title="Delete Selected Layer">${glyph('trash', 16)}</button></footer>
</div>`);
    this.list = q(this.root, '.ly-list');
    this.scroller = q(this.root, '.ly-scroll');
    this.marker = q(this.root, '.ly-marker');
    this.empty = q(this.root, '.ly-empty');
    this.note = q(this.root, '.ly-note');
    this.footInfo = q(this.root, '.pg-foot-info');
    this.delBtn = q<HTMLButtonElement>(this.root, '[data-act="delete"]');
    q(this.root, '[data-act="new"]').addEventListener('click', () => this.ws.commands.run('layers.new'));
    this.delBtn.addEventListener('click', () => this.ws.commands.run('layers.delete'));
    this.root.addEventListener('contextmenu', (e) => this.onContext(e));
    editor.on('docs', () => this.later.run(40));
    editor.on('layout', () => this.later.run(30));
    editor.on('selection', () => this.patchSoon.run(16));
    editor.on('change', (e: ChangeEvent) => { if (e.docChanged) this.later.run(180); });
    new ResizeObserver(() => { if (isShown(this.root) && !this.data) this.later.run(0); }).observe(this.root);
    const vp = this.ws.viewport;
    const prev = vp.onView;
    vp.onView = () => {
      prev?.();
      const s = this.currentSpread();
      if (s !== this.spread) this.patchSoon.run(60);
    };
  }

  onShow(): void { this.later.run(0); }

  get layers(): InspectLayer[] { return this.data?.layers ?? []; }
  get activeLayer(): number { return this.data?.activeLayer ?? -1; }
  get activeLayerInfo(): InspectLayer | undefined { return this.layers.find((l) => l.id === this.activeLayer); }

  private currentSpread(): number {
    const lay = editor.layout;
    if (!lay?.slots.length) return -1;
    const s = spreadOf(lay.slots[this.ws.viewport.currentSpread()] ?? lay.slots[0]);
    return s.kind === 'doc' ? s.index : -1;
  }

  private async refresh(): Promise<void> {
    if (!editor.hasDoc) { this.data = null; this.patch(); return; }
    if (!isShown(this.root)) return;
    const uid = editor.doc?.uid;
    const d = await inspectNow();
    if (!d) return;
    if (this.slotsRev !== engine.rev) {
      const rev = engine.rev;
      try {
        const [s, l] = await Promise.all([engine.call<{ slots: Array<{ id: number; slot: string }> }>('slots', {}), engine.exec<Array<{ name: string; uses: Array<{ id: number }> }>>('links.list', {})]);
        this.slots = new Map((s.result?.slots ?? []).map((x) => [x.id, x.slot]));
        this.files = new Map((l ?? []).flatMap((x) => x.uses.map((u) => [u.id, x.name] as [number, string])));
        this.slotsRev = rev;
      } catch { }
    }
    if (editor.doc?.uid !== uid) return;
    this.data = d;
    this.dataUid = uid ?? -1;
    this.patch();
  }

  private uidSet<T>(map: Map<number, Set<T>>): Set<T> {
    const uid = editor.doc?.uid ?? -1;
    let s = map.get(uid);
    if (!s) { s = new Set(); map.set(uid, s); }
    return s;
  }

  patch(): void {
    const d = editor.hasDoc && this.dataUid === editor.doc?.uid ? this.data : null;
    setHidden(this.empty, !!d || editor.hasDoc);
    setClass(this.root, 'is-empty', !editor.hasDoc);
    this.spread = this.currentSpread();
    if (!d) {
      if (!editor.hasDoc) patchList(this.list, [], { key: () => '', create: () => el('<div></div>'), update: () => undefined });
      setText(this.footInfo, '');
      this.delBtn.disabled = true;
      setHidden(this.note, true);
      return;
    }
    const sel = new Set(editor.selection?.items ?? []);
    const spread = d.spreads.find((s) => s.index === this.spread) ?? null;
    const items = spread?.items ?? [];
    const collapsed = this.uidSet(this.collapsed);
    const open = this.uidSet(this.openGroups);
    const contains = (it: InspectItem): boolean => sel.has(it.id) || (it.children ?? []).some(contains);
    patchList(this.list, d.layers, {
      key: (l) => `L${l.id}`,
      create: (l) => { const b = this.makeBlock(); this.patchBlock(b, l, d, items, sel, collapsed, open, contains); return b; },
      update: (b, l) => this.patchBlock(b, l, d, items, sel, collapsed, open, contains),
    });
    const editing = !!editor.layout?.editingParents;
    setText(this.note, editing ? 'Parent page items are edited on the canvas; the list shows document spreads.' : '');
    setHidden(this.note, !editing);
    const pages = editor.layout?.pages ?? [];
    const onSpread = pages.filter((p) => p.spread === this.spread).map((p) => p.abs);
    const where = onSpread.length ? `${onSpread.length > 1 ? 'Pages' : 'Page'} ${rangeText(onSpread, pages)}` : editing ? 'Parents' : '';
    setText(this.footInfo, `${where}${where ? ' · ' : ''}${d.layers.length} ${d.layers.length === 1 ? 'Layer' : 'Layers'}`);
    this.delBtn.disabled = d.layers.length <= 1;
  }

  private makeBlock(): HTMLElement {
    const b = el(`<div class="ly-block"><div class="ly-row ly-layer">${flags()}<button class="ly-disclose" type="button" title="Show or hide the items on this spread">${glyph('chevron', 12)}</button><span class="ly-chip"></span><span class="ly-name"></span><span class="ly-pen" title="Active layer: new objects go here">${glyph('pen', 12)}</span><button class="ly-sel" type="button" title="Select the items on this layer"></button></div><div class="ly-items"></div></div>`);
    const row = q(b, '.ly-layer');
    row.addEventListener('pointerdown', (e) => this.onLayerDown(e, b));
    q(row, '.ly-eye').addEventListener('click', (e) => { e.stopPropagation(); this.toggleLayer(b, 'visible', e.altKey); });
    q(row, '.ly-lock').addEventListener('click', (e) => { e.stopPropagation(); this.toggleLayer(b, 'locked', e.altKey); });
    q(row, '.ly-disclose').addEventListener('click', (e) => {
      e.stopPropagation();
      const id = Number(b.dataset.id), set = this.uidSet(this.collapsed);
      if (set.has(id)) set.delete(id); else set.add(id);
      this.patch();
    });
    q(row, '.ly-sel').addEventListener('click', (e) => { e.stopPropagation(); void editor.exec('layer.selectItems', { id: Number(b.dataset.id) }); });
    q(row, '.ly-name').addEventListener('dblclick', (e) => { e.stopPropagation(); this.rename(q(row, '.ly-name'), (name) => editor.exec('layer.set', { id: Number(b.dataset.id), name })); });
    q(row, '.ly-chip').addEventListener('dblclick', (e) => { e.stopPropagation(); this.ws.commands.run('layers.options'); });
    return b;
  }

  private patchBlock(b: HTMLElement, l: InspectLayer, d: InspectData, items: InspectItem[], sel: Set<number>, collapsed: Set<number>, open: Set<number>, contains: (it: InspectItem) => boolean): void {
    b.dataset.id = String(l.id);
    const row = q(b, '.ly-layer');
    setStyle(b, '--ly-color', rgb(l.color));
    setText(q(row, '.ly-name'), l.name);
    setTitle(q(row, '.ly-name'), `${l.name}${l.printable ? '' : ' (non-printing)'}`);
    setClass(row, 'is-hidden', !l.visible);
    setClass(row, 'is-locked', l.locked);
    setClass(row, 'is-active', l.id === d.activeLayer);
    setClass(row, 'is-noprint', !l.printable);
    const mine = items.filter((it) => it.layer === l.id).reverse();
    const shut = collapsed.has(l.id);
    setClass(b, 'is-collapsed', shut);
    setClass(row, 'has-items', mine.length > 0);
    setClass(row, 'has-selected', mine.some(contains));
    const rows: Row[] = [];
    const walk = (list: InspectItem[], depth: number): void => {
      for (const it of list) {
        const group = !!it.children?.length;
        const isOpen = group && open.has(it.id);
        rows.push({ item: it, depth, open: isOpen, group });
        if (isOpen) walk([...(it.children ?? [])].reverse(), depth + 1);
      }
    };
    if (!shut) walk(mine, 0);
    patchList(q(b, '.ly-items'), rows, {
      key: (r) => `I${r.item.id}`,
      create: (r) => { const n = this.makeItem(); this.patchItem(n, r, d, sel, contains); return n; },
      update: (n, r) => this.patchItem(n, r, d, sel, contains),
    });
  }

  private makeItem(): HTMLElement {
    const n = el(`<div class="ly-row ly-item">${flags()}<span class="ly-indent"></span><button class="ly-disclose" type="button" title="Show or hide the group's items">${glyph('chevron', 12)}</button><span class="ly-kind"></span><span class="ly-name"></span><span class="ly-slot"></span><span class="ly-sel"></span></div>`);
    n.addEventListener('pointerdown', (e) => this.onItemDown(e, n));
    q(n, '.ly-eye').addEventListener('click', (e) => { e.stopPropagation(); void editor.exec('object.setFlags', { ids: [Number(n.dataset.id)], hidden: !n.classList.contains('is-hidden') }); });
    q(n, '.ly-lock').addEventListener('click', (e) => { e.stopPropagation(); void editor.exec('object.setFlags', { ids: [Number(n.dataset.id)], locked: !n.classList.contains('is-locked') }); });
    q(n, '.ly-disclose').addEventListener('click', (e) => {
      e.stopPropagation();
      const id = Number(n.dataset.id), set = this.uidSet(this.openGroups);
      if (set.has(id)) set.delete(id); else set.add(id);
      this.patch();
    });
    q(n, '.ly-name').addEventListener('dblclick', (e) => { e.stopPropagation(); this.rename(q(n, '.ly-name'), (name) => editor.exec('object.rename', { id: Number(n.dataset.id), name })); });
    return n;
  }

  private label(it: InspectItem, d: InspectData): { text: string; kind: string } {
    const kind = kindGlyph(it.kind, !!it.children?.length);
    if (it.name) return { text: it.name, kind };
    if (it.story !== undefined) {
      const st = d.stories.find((s) => s.id === it.story);
      const w = st ? words(st.preview, 5) : '';
      if (w) return { text: w, kind };
    }
    const file = this.files.get(it.id);
    if (file) return { text: file, kind };
    if (it.children?.length) return { text: `Group of ${it.children.length}`, kind };
    return { text: KIND_LABEL[kind] ?? 'Object', kind };
  }

  private patchItem(n: HTMLElement, r: Row, d: InspectData, sel: Set<number>, contains: (it: InspectItem) => boolean): void {
    const it = r.item;
    n.dataset.id = String(it.id);
    const { text, kind } = this.label(it, d);
    const kindEl = q(n, '.ly-kind');
    if (kindEl.dataset.kind !== kind) { kindEl.dataset.kind = kind; kindEl.replaceChildren(el(glyph(kind, 14))); }
    setText(q(n, '.ly-name'), text);
    const slot = this.slots.get(it.id) ?? '';
    setText(q(n, '.ly-slot'), slot);
    setHidden(q(n, '.ly-slot'), !slot);
    setTitle(n, `${text}${slot ? ` · slot ${slot}` : ''}`);
    setStyle(n, '--depth', String(r.depth));
    setClass(n, 'is-group', r.group);
    setClass(n, 'is-open', r.open);
    setClass(n, 'is-hidden', it.hidden);
    setClass(n, 'is-locked', it.locked);
    setClass(n, 'is-selected', sel.has(it.id));
    setClass(n, 'has-selected', !sel.has(it.id) && contains(it));
  }

  private toggleLayer(b: HTMLElement, key: 'visible' | 'locked', alt: boolean): void {
    const id = Number(b.dataset.id);
    const l = this.layers.find((x) => x.id === id);
    if (!l) return;
    if (alt) { void editor.exec('layer.others', key === 'visible' ? { id, hide: true } : { id, lock: true }); return; }
    void editor.exec('layer.set', key === 'visible' ? { id, visible: !l.visible } : { id, locked: !l.locked });
  }

  private rename(span: HTMLElement, commit: (name: string) => Promise<unknown>): void {
    if (this.renaming) return;
    const input = el<HTMLInputElement>('<input class="ly-rename" type="text" spellcheck="false">');
    input.value = span.textContent ?? '';
    span.hidden = true;
    span.after(input);
    this.renaming = input;
    input.focus();
    input.select();
    let done = false;
    const end = (ok: boolean): void => {
      if (done) return;
      done = true;
      const v = input.value.trim();
      if (ok && v && v !== span.textContent) { setText(span, v); void commit(v); }
      input.remove();
      span.hidden = false;
      this.renaming = null;
    };
    input.addEventListener('keydown', (e) => { e.stopPropagation(); if (e.key === 'Enter') end(true); else if (e.key === 'Escape') end(false); });
    input.addEventListener('blur', () => end(true));
    input.addEventListener('pointerdown', (e) => e.stopPropagation());
  }

  private onItemDown(e: PointerEvent, n: HTMLElement): void {
    if (e.button !== 0 || (e.target as HTMLElement).closest('button, input')) return;
    const id = Number(n.dataset.id);
    const x0 = e.clientX, y0 = e.clientY;
    let dragging = false;
    let ghost: HTMLElement | null = null;
    let over: HTMLElement | null = null;
    const sel = editor.selection?.items ?? [];
    const ids = sel.includes(id) ? sel : [id];
    const move = (ev: PointerEvent): void => {
      if (!dragging && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 5) return;
      if (!dragging) { dragging = true; ghost = el('<div class="pg-ghost"></div>'); setText(ghost, ids.length > 1 ? `${ids.length} items` : q(n, '.ly-name').textContent ?? ''); document.body.appendChild(ghost); }
      if (ghost) { ghost.style.left = `${ev.clientX + 12}px`; ghost.style.top = `${ev.clientY + 10}px`; }
      const hit = (document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null)?.closest<HTMLElement>('.ly-block') ?? null;
      const row = hit && this.list.contains(hit) ? q(hit, '.ly-layer') : null;
      if (row !== over) { over?.classList.remove('is-drop'); over = row; over?.classList.add('is-drop'); }
    };
    const up = (ev: PointerEvent): void => {
      document.removeEventListener('pointermove', move, true);
      document.removeEventListener('pointerup', up, true);
      ghost?.remove();
      over?.classList.remove('is-drop');
      if (!dragging) {
        if (ev.metaKey || ev.ctrlKey) void editor.exec('selection.toggle', { id });
        else void editor.exec('selection.set', ev.shiftKey ? { ids: [id], add: true } : { ids: [id] });
        return;
      }
      const block = over?.closest<HTMLElement>('.ly-block');
      if (block) void editor.exec('object.setLayer', { layer: Number(block.dataset.id), ids });
    };
    document.addEventListener('pointermove', move, true);
    document.addEventListener('pointerup', up, true);
  }

  private onLayerDown(e: PointerEvent, b: HTMLElement): void {
    if (e.button !== 0 || (e.target as HTMLElement).closest('button, input')) return;
    const id = Number(b.dataset.id);
    const x0 = e.clientX, y0 = e.clientY;
    let dragging = false;
    let ghost: HTMLElement | null = null;
    let to = -1;
    const move = (ev: PointerEvent): void => {
      if (!dragging && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 5) return;
      if (!dragging) { dragging = true; ghost = el('<div class="pg-ghost"></div>'); setText(ghost, q(b, '.ly-name').textContent ?? ''); document.body.appendChild(ghost); b.classList.add('is-dragging'); }
      if (ghost) { ghost.style.left = `${ev.clientX + 12}px`; ghost.style.top = `${ev.clientY + 10}px`; }
      const blocks = [...this.list.querySelectorAll<HTMLElement>(':scope > .ly-block')];
      let gap = blocks.length, y = 0;
      const host = this.scroller.getBoundingClientRect();
      for (let i = 0; i < blocks.length; i++) {
        const r = q(blocks[i], '.ly-layer').getBoundingClientRect();
        if (ev.clientY < r.top + r.height / 2) { gap = i; y = r.top; break; }
        y = blocks[i].getBoundingClientRect().bottom;
      }
      const from = blocks.indexOf(b);
      to = gap > from ? gap - 1 : gap;
      setStyle(this.marker, 'top', `${Math.round(y - host.top + this.scroller.scrollTop - 1)}px`);
      setHidden(this.marker, to === from);
    };
    const up = (): void => {
      document.removeEventListener('pointermove', move, true);
      document.removeEventListener('pointerup', up, true);
      ghost?.remove();
      b.classList.remove('is-dragging');
      setHidden(this.marker, true);
      if (!dragging) { if (id !== this.activeLayer) void editor.exec('layer.activate', { id }); return; }
      const from = [...this.list.children].indexOf(b);
      if (to >= 0 && to !== from) void editor.exec('layer.move', { id, to });
    };
    document.addEventListener('pointermove', move, true);
    document.addEventListener('pointerup', up, true);
  }

  private onContext(e: MouseEvent): void {
    e.preventDefault();
    if (!editor.hasDoc) return;
    const b = (e.target as HTMLElement).closest<HTMLElement>('.ly-block');
    const item = (e.target as HTMLElement).closest<HTMLElement>('.ly-item');
    if (item) {
      const id = Number(item.dataset.id);
      if (!(editor.selection?.items ?? []).includes(id)) void editor.exec('selection.set', { ids: [id] });
      openMenu(this.ws, ['layers.moveToActive', '-', 'layers.new', 'layers.options'], e.clientX, e.clientY);
      return;
    }
    if (b) { const id = Number(b.dataset.id); if (id !== this.activeLayer) void editor.exec('layer.activate', { id }); }
    openMenu(this.ws, this.menu(), e.clientX, e.clientY);
  }

  menu(): string[] {
    return ['layers.new', 'layers.delete', 'layers.options', '-', 'layers.selectItems', 'layers.moveToActive', '-', 'layers.hideOthers', 'layers.lockOthers', 'layers.showAll', 'layers.unlockAll', '-', 'layers.mergeDown', 'layers.deleteUnused'];
  }
}
