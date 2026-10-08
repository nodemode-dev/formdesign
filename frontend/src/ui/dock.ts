import { el, setClass, setHidden, setStyleVar } from './dom';
import { PanelGroup } from './panel-group';

export type DockColumn = 'side' | 'dock';
export interface DockState { side: string[]; dock: string[]; hidden: string[]; heights: Record<string, number>; custom: Array<{ id: string; label: string }>; width: number | null }
export interface PanelLayout { dock: DockState; groups: Record<string, { tab?: string; min?: boolean }>; homes: Record<string, string> }

const KEY = 'formdes.dock';
const W_MIN = 220, W_MAX = 560;
const CUSTOM_ID = /^g[0-9a-z]{1,16}$/;

const ids = (host: HTMLElement): string[] => Array.from(host.children).map((c) => (c as HTMLElement).dataset.group ?? '').filter(Boolean);
const nums = (o: unknown): Record<string, number> => Object.fromEntries(Object.entries(o && typeof o === 'object' ? o : {}).filter(([, v]) => typeof v === 'number' && Number.isFinite(v))) as Record<string, number>;
const strs = (a: unknown): string[] => (Array.isArray(a) ? a.filter((x): x is string => typeof x === 'string') : []);

export class Dock {
  readonly defaults: DockState;
  private state: DockState;
  private readonly marker = el('<div class="cv-dock-marker" hidden></div>');
  private readonly splits = new Map<string, HTMLElement>();
  private readonly wired = new WeakSet<HTMLElement>();
  private drag: { id: string; x0: number; y0: number; moving: boolean; at: { col: DockColumn; index: number } | null } | null = null;
  private ghost: HTMLElement | null = null;
  private suppressClick = false;
  onChange: (() => void) | null = null;

  constructor(private readonly root: HTMLElement, private readonly cols: Record<DockColumn, HTMLElement>, private readonly resizer: HTMLElement, private readonly makeGroup: (host: HTMLElement, id: string, label: string) => PanelGroup) {
    this.defaults = { side: ids(cols.side), dock: ids(cols.dock), hidden: [], heights: {}, custom: [], width: null };
    let raw: Partial<DockState> = {};
    try { raw = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<DockState>; } catch { raw = {}; }
    for (const c of Array.isArray(raw.custom) ? raw.custom : []) if (c && CUSTOM_ID.test(String(c.id)) && typeof c.label === 'string' && !PanelGroup.all.has(c.id)) makeGroup(cols.dock, c.id, c.label);
    this.state = this.merge(raw);
    this.wireResizer();
    window.addEventListener('pointerup', () => { if (this.drag) this.drop(); });
    this.apply();
  }

  private known(): string[] { return [...PanelGroup.all.keys()]; }

  private merge(raw: Partial<DockState>): DockState {
    const known = this.known(), has = (id: string) => known.includes(id);
    const side = strs(raw.side).filter(has), dock = strs(raw.dock).filter((id) => has(id) && !side.includes(id));
    for (const id of this.defaults.side) if (!side.includes(id) && !dock.includes(id)) side.splice(Math.min(this.defaults.side.indexOf(id), side.length), 0, id);
    for (const id of known) if (!side.includes(id) && !dock.includes(id)) { const at = this.defaults.dock.indexOf(id); dock.splice(at < 0 ? dock.length : Math.min(at, dock.length), 0, id); }
    const custom = (Array.isArray(raw.custom) ? raw.custom : []).filter((c) => c && has(c.id) && CUSTOM_ID.test(c.id)).map((c) => ({ id: c.id, label: String(c.label).slice(0, 80) }));
    const w = typeof raw.width === 'number' && raw.width >= W_MIN && raw.width <= W_MAX ? Math.round(raw.width) : null;
    return { side, dock, hidden: strs(raw.hidden).filter(has), heights: nums(raw.heights), custom, width: w };
  }

  get snapshot(): DockState { return JSON.parse(JSON.stringify(this.state)) as DockState; }

  setState(next: Partial<DockState>): void {
    for (const c of next.custom ?? []) if (CUSTOM_ID.test(c.id) && !PanelGroup.all.has(c.id)) this.makeGroup(this.cols.dock, c.id, c.label);
    this.state = this.merge(next);
    this.save();
  }

  isHidden(id: string): boolean { return this.state.hidden.includes(id); }
  show(id: string): void { if (this.isHidden(id)) { this.state.hidden = this.state.hidden.filter((h) => h !== id); this.save(); } }
  hide(id: string): void { if (!this.isHidden(id)) { this.state.hidden = [...this.state.hidden, id]; this.save(); } }
  refresh(): void { this.dropEmpty(); this.apply(); this.onChange?.(); }

  addCustom(id: string, label: string, col: DockColumn, index: number): void {
    this.state.custom = [...this.state.custom, { id, label }];
    this.insert(id, col, index);
  }

  private dropEmpty(): void {
    const keep = this.state.custom.filter((c) => { const g = PanelGroup.all.get(c.id); if (g && g.size > 0) return true; g?.remove(); return false; });
    if (keep.length === this.state.custom.length) return;
    const gone = new Set(this.state.custom.filter((c) => !keep.includes(c)).map((c) => c.id));
    this.state = { ...this.state, custom: keep, side: this.state.side.filter((id) => !gone.has(id)), dock: this.state.dock.filter((id) => !gone.has(id)) };
    this.persist();
  }

  private section(id: string): HTMLElement | null { return PanelGroup.all.get(id)?.root ?? null; }
  private present(id: string): boolean { const s = this.section(id); return !!s && !this.isHidden(id) && !s.classList.contains('is-empty'); }
  private shown(col: DockColumn): HTMLElement[] { return this.state[col].filter((id) => this.present(id)).map((id) => this.section(id) as HTMLElement); }

  private persist(): void { try { localStorage.setItem(KEY, JSON.stringify(this.state)); } catch { } }
  private save(): void { this.persist(); this.apply(); this.onChange?.(); }

  private apply(): void {
    for (const col of ['side', 'dock'] as DockColumn[]) {
      const host = this.cols[col], want = this.state[col].map((id) => this.section(id)).filter((s): s is HTMLElement => !!s);
      const now = Array.from(host.children).filter((c) => want.includes(c as HTMLElement));
      if (want.some((s, i) => now[i] !== s)) for (const s of want) host.appendChild(s);
    }
    for (const g of PanelGroup.all.values()) {
      setHidden(g.root, !this.present(g.id));
      this.wire(g);
      const h = this.state.heights[g.id], sized = typeof h === 'number';
      setClass(g.root, 'is-sized', sized);
      const px = sized ? `${Math.round(h)}px` : '';
      if (g.root.style.height !== px) g.root.style.height = px;
    }
    this.splitters();
    if (this.state.width) setStyleVar(this.root, '--cv-layers-w', `${this.state.width}px`);
    else if (this.root.style.getPropertyValue('--cv-layers-w')) this.root.style.removeProperty('--cv-layers-w');
  }

  private splitters(): void {
    const used = new Set<string>();
    for (const col of ['side', 'dock'] as DockColumn[]) {
      const shown = this.shown(col);
      for (let i = 0; i < shown.length - 1; i++) {
        const above = shown[i], id = above.dataset.group as string;
        used.add(id);
        let s = this.splits.get(id);
        if (!s) { s = this.splitter(id); this.splits.set(id, s); }
        if (above.nextElementSibling !== s) above.after(s);
      }
    }
    for (const [id, s] of this.splits) if (!used.has(id)) { s.remove(); this.splits.delete(id); }
  }

  private splitter(id: string): HTMLElement {
    const s = el('<div class="cv-dock-split" role="separator" aria-orientation="horizontal" aria-label="Resize the panel group"></div>');
    s.addEventListener('dblclick', () => { delete this.state.heights[id]; this.save(); });
    s.addEventListener('pointerdown', (e) => {
      const target = this.section(id);
      if (e.button !== 0 || !target || target.classList.contains('is-min')) return;
      e.preventDefault();
      s.setPointerCapture(e.pointerId);
      const y0 = e.clientY, h0 = target.getBoundingClientRect().height, head = target.querySelector('.cv-panel-head')?.getBoundingClientRect().height ?? 40;
      const host = target.parentElement as HTMLElement, room = host.getBoundingClientRect().bottom - target.getBoundingClientRect().top - 60;
      const move = (ev: PointerEvent): void => {
        this.state.heights[id] = Math.max(head + 60, Math.min(room, h0 + ev.clientY - y0));
        setClass(target, 'is-sized', true);
        target.style.height = `${Math.round(this.state.heights[id])}px`;
      };
      const up = (): void => { s.removeEventListener('pointermove', move); s.removeEventListener('pointerup', up); s.removeEventListener('pointercancel', up); this.persist(); };
      s.addEventListener('pointermove', move); s.addEventListener('pointerup', up); s.addEventListener('pointercancel', up);
    });
    return s;
  }

  private wireResizer(): void {
    this.resizer.addEventListener('dblclick', () => { this.state.width = null; this.save(); });
    this.resizer.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      this.resizer.setPointerCapture(e.pointerId);
      setClass(this.resizer, 'is-dragging', true);
      const right = this.cols.dock.getBoundingClientRect().right;
      const move = (ev: PointerEvent): void => {
        this.state.width = Math.round(Math.max(W_MIN, Math.min(W_MAX, right - ev.clientX - 3)));
        setStyleVar(this.root, '--cv-layers-w', `${this.state.width}px`);
      };
      const up = (): void => {
        setClass(this.resizer, 'is-dragging', false);
        this.resizer.removeEventListener('pointermove', move); this.resizer.removeEventListener('pointerup', up); this.resizer.removeEventListener('pointercancel', up);
        this.persist();
      };
      this.resizer.addEventListener('pointermove', move); this.resizer.addEventListener('pointerup', up); this.resizer.addEventListener('pointercancel', up);
    });
  }

  private wire(g: PanelGroup): void {
    const head = g.root.querySelector<HTMLElement>(':scope > .cv-panel-head');
    if (!head || this.wired.has(head)) return;
    this.wired.add(head);
    head.addEventListener('pointerdown', (e) => {
      const t = e.target as HTMLElement;
      if (e.button !== 0 || t.closest('button:not(.cv-group-tab), [data-cmd], input, select')) return;
      if (t.closest('.cv-group-tab') && g.size > 1) return;
      this.drag = { id: g.id, x0: e.clientX, y0: e.clientY, moving: false, at: null };
      document.addEventListener('pointermove', this.docMove, true);
    });
    head.addEventListener('click', (e) => { if (this.suppressClick) { e.stopPropagation(); e.preventDefault(); this.suppressClick = false; } }, true);
  }

  private readonly docMove = (e: PointerEvent): void => {
    const d = this.drag;
    if (!d) { document.removeEventListener('pointermove', this.docMove, true); return; }
    if ((e.buttons & 1) === 0) { this.drop(); return; }
    if (!d.moving && Math.hypot(e.clientX - d.x0, e.clientY - d.y0) < 6) return;
    d.moving = true;
    if (!this.ghost) { this.ghost = el('<div class="cv-drag-ghost"></div>'); this.ghost.textContent = PanelGroup.all.get(d.id)?.root.getAttribute('aria-label') ?? d.id; document.body.appendChild(this.ghost); }
    this.ghost.style.left = `${e.clientX + 12}px`;
    this.ghost.style.top = `${e.clientY + 10}px`;
    const at = this.gapAt(e.clientX, e.clientY);
    d.at = at ? { col: at.col, index: at.index } : null;
    if (at) this.placeMarker(at, 'Move here'); else setHidden(this.marker, true);
  };

  private drop(): void {
    const d = this.drag;
    this.drag = null;
    this.ghost?.remove();
    this.ghost = null;
    document.removeEventListener('pointermove', this.docMove, true);
    setHidden(this.marker, true);
    if (!d?.moving || !d.at) return;
    this.suppressClick = true;
    window.setTimeout(() => { this.suppressClick = false; }, 0);
    this.insert(d.id, d.at.col, d.at.index);
  }

  private insert(id: string, col: DockColumn, index: number): void {
    const before = this.state[col].filter((x) => this.present(x))[index] ?? null;
    if (before === id) return;
    const side = this.state.side.filter((x) => x !== id), dock = this.state.dock.filter((x) => x !== id);
    const into = col === 'side' ? side : dock, at = before ? into.indexOf(before) : into.length;
    into.splice(at < 0 ? into.length : at, 0, id);
    this.state = { ...this.state, side, dock };
    this.save();
  }

  gapAt(x: number, y: number): { col: DockColumn; index: number; host: HTMLElement; y: number } | null {
    let col: DockColumn | null = null;
    for (const c of ['side', 'dock'] as DockColumn[]) {
      const r = this.cols[c].getBoundingClientRect();
      if (r.width > 0 && x >= r.left && x <= r.right && y >= r.top && y <= r.bottom) col = c;
    }
    if (!col) return null;
    const host = this.cols[col], shown = this.shown(col);
    let index = 0, best = Infinity, gy = host.getBoundingClientRect().top + 2;
    for (let i = 0; i <= shown.length; i++) {
      const g = i < shown.length ? shown[i].getBoundingClientRect().top : (shown.length ? shown[shown.length - 1].getBoundingClientRect().bottom : gy);
      if (Math.abs(y - g) < best) { best = Math.abs(y - g); index = i; gy = g; }
    }
    return { col, index, host, y: gy };
  }

  private placeMarker(at: { host: HTMLElement; y: number }, label: string): void {
    if (this.marker.parentElement !== at.host) at.host.appendChild(this.marker);
    this.marker.dataset.label = label;
    this.marker.style.top = `${Math.round(at.y - at.host.getBoundingClientRect().top + at.host.scrollTop - 1)}px`;
    setHidden(this.marker, false);
  }

  showGap(x: number, y: number, label: string): void { const at = this.gapAt(x, y); if (at) this.placeMarker(at, label); else setHidden(this.marker, true); }
  hideGap(): void { if (!this.drag) setHidden(this.marker, true); }
}
