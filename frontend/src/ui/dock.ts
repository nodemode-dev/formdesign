import { el, setHidden } from './dom';
import { icon } from './icons';

/** The side dock's panels, arranged as Photoshop's: drag a panel by its header to move it up or down the dock (a line
 *  shows where it will land), show or hide panels from the Window menu, and have the arrangement remembered on this
 *  device. Sourced in the planning notes' workspace research (Adobe's "Arrange and group panels" and "Dock or undock
 *  panels"; TeachUcomp). Every panel in the side dock moves, the Layers panel included; floating panels and grouping into
 *  tabs come later. */
export interface DockPanel { id: string; label: string; selector: string }
export const DOCK_PANELS: readonly DockPanel[] = [
  { id: 'navigator', label: 'Navigator', selector: '.cv-navigator' },
  { id: 'swatches', label: 'Swatches', selector: '.cv-swatches' },
  { id: 'layers', label: 'Layers', selector: '.cv-layers-panel' },
  { id: 'timeline', label: 'Timeline', selector: '.cv-timeline' },
  { id: 'history', label: 'History', selector: '.cv-history' },
  { id: 'info', label: 'Info', selector: 'section.cv-info' },
];
/** The side column's panel groups (Phase 21), movable like the dock's: any panel may live in either column. */
export const SIDE_PANELS: readonly DockPanel[] = [
  { id: 'properties', label: 'Properties', selector: 'section.cv-properties' },
  { id: 'brushes', label: 'Brushes', selector: 'section.cv-brushes' },
  { id: 'paragraph', label: 'Paragraph', selector: 'section.cv-paragraph' },
  { id: 'character', label: 'Character', selector: 'section.cv-character' },
];
export const ALL_PANELS: DockPanel[] = [...DOCK_PANELS, ...SIDE_PANELS];
/** Groups made by detaching a tab (1.49.2) join the panels the dock arranges, and leave once empty. */
export function registerPanel(p: DockPanel): void { if (!ALL_PANELS.some((x) => x.id === p.id)) ALL_PANELS.push(p); }
export function unregisterPanel(id: string): void { const i = ALL_PANELS.findIndex((x) => x.id === id); if (i >= 0 && !DOCK_PANELS.some((x) => x.id === id) && !SIDE_PANELS.some((x) => x.id === id)) ALL_PANELS.splice(i, 1); }
const DEFAULT_SIDE = SIDE_PANELS.map((p) => p.id);
/** order: the dock's panels, top to bottom; side: the side column's; hidden: those not shown, wherever they live. */
interface DockState { order: string[]; hidden: string[]; side: string[] }
const KEY = 'formvis.dock';
const HEIGHTS_KEY = 'formvis.dock.heights';
/** As the target's Essentials (and Photoshop's): Navigator, Swatches and Layers shown; Timeline and History in Window. */
const DEFAULT: DockState = { order: DOCK_PANELS.map((p) => p.id), hidden: ['timeline', 'history', 'info'], side: [...DEFAULT_SIDE] };

function load(): DockState {
  try {
    const r = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<DockState>;
    const ids = ALL_PANELS.map((p) => p.id), dockIds = DOCK_PANELS.map((p) => p.id);
    // The side column's panels as saved (a saved arrangement from before panels could move has none: the default).
    const side = Array.isArray(r.side) ? r.side.filter((id): id is string => ids.includes(id as string)) : [...DEFAULT_SIDE];
    const order = Array.isArray(r.order) ? r.order.filter((id): id is string => ids.includes(id as string) && !side.includes(id as string)) : [];
    // A panel missing from both takes its default column and place, not the end.
    for (const id of ids) if (!order.includes(id) && !side.includes(id)) {
      if (DEFAULT_SIDE.includes(id) && !Array.isArray(r.side)) side.push(id);
      else order.splice(Math.min(Math.max(0, dockIds.indexOf(id)), order.length), 0, id);
    }
    const hidden = Array.isArray(r.hidden) ? r.hidden.filter((id): id is string => ids.includes(id as string)) : [...DEFAULT.hidden];
    // A panel newer than the saved arrangement (the Info panel, 1.49.6) starts as it comes: hidden if it comes hidden.
    const known = new Set([...(Array.isArray(r.order) ? r.order : []), ...(Array.isArray(r.side) ? r.side : [])]);
    if (Array.isArray(r.hidden)) for (const id of DEFAULT.hidden) if (!known.has(id) && !hidden.includes(id)) hidden.push(id);
    return { order, hidden, side };
  } catch { return { ...DEFAULT, order: [...DEFAULT.order], side: [...DEFAULT_SIDE] }; }
}

export class Dock {
  private state = load();
  private readonly marker = el('<div class="cv-dock-marker" hidden></div>');
  private drag: { id: string; startY: number; moving: boolean; target: number; col: 'dock' | 'side' } | null = null;
  private suppressClick = false;
  onChange: (() => void) | null = null;

  /** The side column (its panels move to and from the dock), set by the app. */
  constructor(private readonly dock: HTMLElement, private readonly side: HTMLElement | null = null) {
    dock.appendChild(this.marker);
    for (const p of ALL_PANELS) {
      // Every header shows where to grab: the dock's own non-group panels (Timeline, History) get the groups' grip.
      const sec = document.querySelector<HTMLElement>(p.selector), hd = sec?.querySelector<HTMLElement>(':scope > .cv-panel-head');
      if (hd && !hd.querySelector('.cv-group-grip')) hd.prepend(el(`<span class="cv-group-grip" title="Drag to move the group" aria-hidden="true">${icon('grip', 14)}</span>`));
      const section = this.section(p.id), head = section?.querySelector<HTMLElement>('.cv-panel-head');
      if (!head) continue;
      head.classList.add('cv-dock-grip');
      head.addEventListener('pointerdown', (e) => {
        if (e.button !== 0 || this.locked || (e.target as HTMLElement).closest('[data-cmd]')) return;
        // A tab in a panel group of several drags itself (PanelGroup); the Layers group's tabs cannot yet, so it moves whole.
        if (section?.classList.contains('cv-group') && (e.target as HTMLElement).closest('.cv-group-tab') && section.querySelectorAll('.cv-group-tab').length > 1) return;
        // The pointer is captured only once the drag moves (move), so a plain click still reaches a tab in the header.
        this.drag = { id: p.id, startY: e.clientY, moving: false, target: -1, col: 'dock' };
        // Movement is followed on the whole document until release: a quick flick leaves the header at once.
        document.addEventListener('pointermove', this.docMove, true);
      });
      head.addEventListener('pointermove', (e) => this.move(e));
      head.addEventListener('pointerup', () => this.drop());
      head.addEventListener('pointercancel', () => { this.drag = null; setHidden(this.marker, true); });
      // A drag ends with a click on the header; it must not also fold or unfold the panel.
      head.addEventListener('click', (e) => { if (this.suppressClick) { e.stopPropagation(); e.preventDefault(); this.suppressClick = false; } }, true);
    }
    // A release anywhere ends a drag begun on a header (released off the header, it used to stay armed).
    window.addEventListener('pointerup', () => { if (this.drag) this.drop(); });
    this.apply();
  }

  isShown(id: string): boolean { return !this.state.hidden.includes(id); }
  /** Shown and holding a tab (a group whose tabs all moved away is absent). */
  private present(id: string): boolean { return this.isShown(id) && !this.section(id)?.classList.contains('is-empty'); }
  /** Re-lays out the columns (tabs moved between groups). */
  refresh(): void { this.apply(); this.onChange?.(); }

  toggle(id: string): void {
    this.state.hidden = this.isShown(id) ? [...this.state.hidden, id] : this.state.hidden.filter((h) => h !== id);
    this.save();
  }

  /** Window > Reset Panels: the default order, every panel shown. */
  reset(): void { this.state = { order: [...DEFAULT.order], hidden: [...DEFAULT.hidden], side: [...DEFAULT_SIDE] }; this.heights = {}; this.saveHeights(); this.save(); }

  /** The panels living in the dock, in order (everything not in the side column). */
  private dockIds(): string[] { return this.state.order.filter((id) => !this.state.side.includes(id)); }
  /** The side column's panels, in order. */
  sideIds(): string[] { return [...this.state.side]; }
  /** How many panels the side column shows (none: the app hides the column). */
  sideShown(): number { return this.state.side.filter((id) => this.present(id)).length; }
  /** Window > Merge Panel Columns: the side column's panels join the dock, below its own; the side column goes. */
  merge(): void { this.state = { ...this.state, order: [...this.dockIds(), ...this.state.side], side: [] }; this.save(); }
  /** Window > Separate Panel Columns: the side column's own panels return to it. */
  separate(): void { this.state = { ...this.state, order: this.dockIds().filter((id) => !DEFAULT_SIDE.includes(id)), side: [...DEFAULT_SIDE] }; this.save(); }
  get merged(): boolean { return this.state.side.length === 0; }

  /** Group heights set by dragging a splitter (Phase 21, the target's resizable groups); Layers takes what is left. */
  private heights: Record<string, number> = (() => { try { const h = JSON.parse(localStorage.getItem(HEIGHTS_KEY) ?? '{}') as Record<string, unknown>; return Object.fromEntries(Object.entries(h).filter(([, v]) => typeof v === 'number' && Number.isFinite(v))) as Record<string, number>; } catch { return {}; } })();
  private saveHeights(): void { try { localStorage.setItem(HEIGHTS_KEY, JSON.stringify(this.heights)); } catch { /* this session only */ } }


  /** A workspace's arrangement (Phase 21): the panels in this order, these hidden; ids not in the dock are ignored. */
  arrange(order: readonly string[], hidden: readonly string[]): void {
    const ids = DOCK_PANELS.map((p) => p.id);
    const o = order.filter((id) => ids.includes(id));
    for (const id of ids) if (!o.includes(id)) o.push(id);
    // A panel the workspace does not name (the Info panel, newer than the workspaces) keeps its default: hidden if it comes hidden.
    const named = new Set([...order, ...hidden]), extra = DEFAULT.hidden.filter((id) => !named.has(id));
    this.state = { order: o, hidden: [...hidden, ...extra].filter((id) => ALL_PANELS.some((p) => p.id === id)), side: [...DEFAULT_SIDE] };
    this.save();
  }

  /** Lock Workspace (Phase 21, the target's lock button): while locked, panels are not dragged. Remembered. */
  get locked(): boolean { try { return localStorage.getItem('formvis.lock') === '1'; } catch { return false; } }
  set locked(on: boolean) { try { localStorage.setItem('formvis.lock', on ? '1' : '0'); } catch { /* this session only */ } this.dock.classList.toggle('is-locked', on); }

  /** A panel's element, wherever it is (either column, or opened from a strip). */
  private section(id: string): HTMLElement | null {
    const p = ALL_PANELS.find((x) => x.id === id);
    return p ? document.querySelector<HTMLElement>(p.selector) : null;
  }

  /** Sections moved into the saved order only when it differs (moved, never rebuilt), and shown or hidden. */
  /** The column collapsed to a strip of icons, as the target's (its « button): the collapse button on the first shown
   *  group's header, the expand button atop the strip, an icon per shown group opening it as a flyout beside the strip
   *  (its element moved there and back). Remembered; the app sets the column's width (onCollapse). */
  onCollapse: ((on: boolean) => void) | null = null;
  get collapsed(): boolean { try { return localStorage.getItem('formvis.dockCollapsed') === '1'; } catch { return false; } }
  setCollapsed(on: boolean): void {
    this.closeFly();
    try { localStorage.setItem('formvis.dockCollapsed', on ? '1' : '0'); } catch { /* this session only */ }
    this.dock.classList.toggle('is-collapsed', on);
    this.renderStrip();
    this.onCollapse?.(on);
  }
  private strip: HTMLElement | null = null;
  private fly: HTMLElement | null = null;
  private flyOpen: { s: HTMLElement; mark: Comment; btn: HTMLElement } | null = null;
  private collapseBtn: HTMLElement | null = null;
  private closeFly(): void {
    const o = this.flyOpen;
    if (!o) return;
    o.mark.replaceWith(o.s);
    o.btn.classList.remove('is-on');
    this.flyOpen = null;
    if (this.fly) setHidden(this.fly, true);
  }
  private renderStrip(): void {
    if (!this.strip) {
      this.strip = el('<div class="cv-dock-strip"></div>');
      this.dock.prepend(this.strip);
      this.fly = el('<div class="cv-props-fly cv-dock-fly" hidden></div>');
      (document.getElementById('overlay-root') ?? document.body).appendChild(this.fly);
      document.addEventListener('pointerdown', (e) => { if (this.flyOpen && !this.fly?.contains(e.target as Node) && !this.strip?.contains(e.target as Node)) this.closeFly(); }, true);
      document.addEventListener('keydown', (e) => { if (this.flyOpen && e.key === 'Escape') this.closeFly(); });
    }
    const strip = this.strip;
    strip.textContent = '';
    const expand = el<HTMLButtonElement>(`<button class="cv-strip-btn cv-strip-expand" type="button" title="Expand panels" aria-label="Expand panels">${icon('collapseLeft', 16)}</button>`);
    expand.addEventListener('click', () => this.setCollapsed(false));
    strip.appendChild(expand);
    const ICONS: Record<string, string> = { navigator: 'panelNav', swatches: 'swatchGrid', layers: 'layers', timeline: 'film', history: 'history' };
    for (const id of this.dockIds().filter((x) => this.present(x))) {
      const s = this.section(id), label = DOCK_PANELS.find((p) => p.id === id)?.label ?? id;
      if (!s) continue;
      const b = el<HTMLButtonElement>(`<button class="cv-strip-btn" type="button" title="${label}" aria-label="${label}">${icon(ICONS[id] ?? 'panelProps', 18)}</button>`);
      b.dataset.group = id;
      b.addEventListener('click', () => {
        if (this.flyOpen?.s === s) { this.closeFly(); return; }
        this.closeFly();
        const fly = this.fly as HTMLElement, mark = document.createComment(id);
        s.replaceWith(mark);
        fly.appendChild(s);
        setHidden(fly, false);
        this.flyOpen = { s, mark, btn: b };
        b.classList.add('is-on');
        const r = b.getBoundingClientRect(), st = strip.getBoundingClientRect();
        fly.style.right = `${Math.round(window.innerWidth - st.left + 6)}px`;
        fly.style.top = `${Math.round(Math.max(8, Math.min(r.top, window.innerHeight - fly.offsetHeight - 8)))}px`;
      });
      strip.appendChild(b);
    }
  }
  /** The collapse button sits on whichever shown group is first. */
  private placeCollapse(): void {
    if (!this.collapseBtn) {
      this.collapseBtn = el<HTMLButtonElement>(`<button class="cv-group-collapse cv-dock-collapse" type="button" title="Collapse to icons" aria-label="Collapse to icons">${icon('collapseRight', 14)}</button>`);
      this.collapseBtn.addEventListener('click', () => this.setCollapsed(true));
    }
    const first = this.dockIds().find((x) => this.present(x)), head = first ? this.section(first)?.querySelector(':scope > .cv-panel-head') : null;
    if (head && this.collapseBtn.parentElement !== head) head.appendChild(this.collapseBtn);
  }

  private apply(): void {
    this.closeFly();
    // Each panel in its column, in order; moved only when the order differs, never rebuilt.
    const want = this.dockIds().map((id) => this.section(id)).filter((s): s is HTMLElement => !!s);
    const current = Array.from(this.dock.children).filter((c) => want.includes(c as HTMLElement));
    if (want.some((s, i) => current[i] !== s)) for (const s of want) this.dock.insertBefore(s, this.marker);
    if (this.side) {
      const sideWant = this.state.side.map((id) => this.section(id)).filter((s): s is HTMLElement => !!s);
      const sideNow = Array.from(this.side.children).filter((c) => sideWant.includes(c as HTMLElement));
      if (sideWant.some((s, i) => sideNow[i] !== s)) for (const s of sideWant) this.side.appendChild(s);
      for (const s of sideWant) { s.classList.remove('is-sized'); s.style.height = ''; }   // sizes are the dock's
    }
    for (const p of ALL_PANELS) { const s = this.section(p.id); if (s) setHidden(s, !this.present(p.id)); }
    this.splitters();
    this.placeCollapse();
    this.dock.classList.toggle('is-collapsed', this.collapsed);
    if (this.collapsed) this.renderStrip();
  }

  /** A splitter between each pair of shown groups: dragging it sizes the group above, or the one below when the one
   *  above is Layers (which takes what is left); a double-click returns that group to its own height. */
  private shownGroups(): Array<{ id: string; s: HTMLElement }> {
    return this.dockIds().filter((id) => this.present(id)).map((id) => ({ id, s: this.section(id) })).filter((x): x is { id: string; s: HTMLElement } => !!x.s);
  }

  /** Each shown group at its set height, or its own; Layers always takes what is left. */
  private sizeGroups(): void {
    for (const { id, s } of this.shownGroups()) {
      const h = this.heights[id];
      const sized = id !== 'layers' && typeof h === 'number';
      s.classList.toggle('is-sized', sized);
      s.style.height = sized ? `${Math.round(h)}px` : '';
    }
  }

  private splitters(): void {
    for (const old of Array.from(this.dock.querySelectorAll(':scope > .cv-dock-split'))) old.remove();
    const shown = this.shownGroups();
    this.sizeGroups();
    for (let i = 0; i < shown.length - 1; i++) {
      const above = shown[i], below = shown[i + 1], target = above.id === 'layers' ? below : above, sign = above.id === 'layers' ? -1 : 1;
      const split = el(`<div class="cv-dock-split" role="separator" aria-orientation="horizontal" aria-label="Resize ${target.id}"></div>`);
      split.dataset.target = target.id;
      above.s.after(split);
      split.addEventListener('dblclick', () => { delete this.heights[target.id]; this.saveHeights(); this.sizeGroups(); });
      split.addEventListener('pointerdown', (e) => {
        if (e.button !== 0 || this.locked) return;
        e.preventDefault();
        split.setPointerCapture(e.pointerId);
        const y0 = e.clientY, h0 = target.s.getBoundingClientRect().height, head = target.s.querySelector('.cv-panel-head')?.getBoundingClientRect().height ?? 40;
        const layers = this.section('layers'), room = layers ? layers.getBoundingClientRect().height - 150 : 200;
        // Only the sizes change while dragging: rebuilding the splitters would drop this one and its pointer capture.
        const move = (ev: PointerEvent): void => { this.heights[target.id] = Math.max(head + 24, Math.min(h0 + Math.max(0, room), h0 + sign * (ev.clientY - y0))); this.sizeGroups(); };
        const up = (): void => { split.removeEventListener('pointermove', move); split.removeEventListener('pointerup', up); split.removeEventListener('pointercancel', up); this.saveHeights(); };
        split.addEventListener('pointermove', move); split.addEventListener('pointerup', up); split.addEventListener('pointercancel', up);
      });
    }
  }

  private save(): void {
    try { localStorage.setItem(KEY, JSON.stringify(this.state)); } catch { /* this session only */ }
    this.apply();
    this.onChange?.();
  }

  /** The drop index is the gap nearest the pointer among the shown sections; the marker sits in that gap. */
  private move(e: PointerEvent): void {
    const d = this.drag;
    if (!d) return;
    // A drag whose button was let go elsewhere is over: hovering a header must not revive it (the stray marker).
    if ((e.buttons & 1) === 0) { this.drag = null; setHidden(this.marker, true); return; }
    if (!d.moving && Math.abs(e.clientY - d.startY) < 5) return;
    d.moving = true;
    const at = this.gapAt(e.clientX, e.clientY);
    if (!at) return;
    const { col, index: best } = at;
    // What is dragged, following the pointer; the marker says what a release does.
    if (!this.ghost) { this.ghost = el('<div class="cv-drag-ghost"></div>'); this.ghost.textContent = ALL_PANELS.find((p) => p.id === d.id)?.label ?? d.id; document.body.appendChild(this.ghost); }
    this.ghost.style.left = `${e.clientX + 12}px`; this.ghost.style.top = `${e.clientY + 10}px`;
    d.target = best;
    d.col = col;
    this.placeMarker(at, 'Move here');
  }

  private ghost: HTMLElement | null = null;
  /** The column and the gap among its shown panels nearest a point: the side column's when over it (and open), else the
   *  dock's; null outside both. Group drags and tab drags (a new group) use the same. */
  gapAt(x: number, y: number): { col: 'dock' | 'side'; index: number; host: HTMLElement; y: number } | null {
    const sr = this.side && !this.side.hidden && !this.side.classList.contains('is-collapsed') ? this.side.getBoundingClientRect() : null;
    const dr = this.dock.getBoundingClientRect();
    const col: 'dock' | 'side' | null = sr && x >= sr.left && x <= sr.right ? 'side' : x >= dr.left && x <= dr.right ? 'dock' : null;
    if (!col) return null;
    const host = col === 'side' ? this.side as HTMLElement : this.dock;
    const shown = (col === 'side' ? this.state.side : this.dockIds()).filter((id) => this.present(id)).map((id) => this.section(id)).filter((s): s is HTMLElement => !!s);
    const box = host.getBoundingClientRect();
    let best = 0, bestDist = Infinity, gy = box.top + 2;
    for (let i = 0; i <= shown.length && shown.length; i++) {
      const gap = i < shown.length ? shown[i].getBoundingClientRect().top : shown[shown.length - 1].getBoundingClientRect().bottom;
      const dist = Math.abs(y - gap);
      if (dist < bestDist) { bestDist = dist; best = i; gy = gap; }
    }
    return { col, index: best, host, y: gy };
  }
  private placeMarker(at: { host: HTMLElement; y: number }, label: string): void {
    if (this.marker.parentElement !== at.host) at.host.appendChild(this.marker);
    this.marker.dataset.label = label;
    this.marker.style.top = `${Math.round(at.y - at.host.getBoundingClientRect().top + at.host.scrollTop - 1)}px`;
    setHidden(this.marker, false);
  }
  /** A tab dragged over a gap: where a new group would go. */
  showGap(x: number, y: number, label: string): void { const at = this.gapAt(x, y); if (at) this.placeMarker(at, label); else setHidden(this.marker, true); }
  hideGap(): void { if (!this.drag) setHidden(this.marker, true); }
  /** A new group into a column at a gap (index among the shown panels). */
  insertPanel(id: string, col: 'dock' | 'side', index: number): void {
    const list = col === 'side' ? this.state.side : this.dockIds();
    const before = list.filter((x) => this.present(x))[index] ?? null;
    const side = this.state.side.filter((x) => x !== id), order = this.state.order.filter((x) => x !== id);
    const into = col === 'side' ? side : order, at = before ? into.indexOf(before) : into.length;
    into.splice(at < 0 ? into.length : at, 0, id);
    this.state = { ...this.state, side, order };
    this.save();
  }
  removePanel(id: string): void { this.state = { ...this.state, side: this.state.side.filter((x) => x !== id), order: this.state.order.filter((x) => x !== id), hidden: this.state.hidden.filter((x) => x !== id) }; this.save(); }

  private readonly docMove = (e: PointerEvent): void => { if (this.drag) this.move(e); else document.removeEventListener('pointermove', this.docMove, true); };

  private drop(): void {
    const d = this.drag;
    this.drag = null;
    this.ghost?.remove();
    this.ghost = null;
    document.removeEventListener('pointermove', this.docMove, true);
    setHidden(this.marker, true);
    if (!d || !d.moving || d.target < 0) return;
    this.suppressClick = true;
    // Into the column it was dropped in, before the panel it was dropped above (or at the end).
    const list = d.col === 'side' ? this.state.side : this.dockIds();
    const shown = list.filter((id) => this.present(id));
    const before = shown[d.target] ?? null;
    if (before === d.id) return;
    const side = this.state.side.filter((id) => id !== d.id), order = this.state.order.filter((id) => id !== d.id);
    const into = d.col === 'side' ? side : order;
    const at = before ? into.indexOf(before) : into.length;
    into.splice(at < 0 ? into.length : at, 0, d.id);
    this.state = { ...this.state, side, order };
    if (this.marker.parentElement !== this.dock) this.dock.appendChild(this.marker);
    this.save();
  }
}
