import { el, setHidden, setText } from './dom';
import { icon } from './icons';

/** A panel group as the target's (Phase 21, UI_SPEC.md section 6): a black header strip whose tabs are chips (the active
 *  one filled, the others plain text), with − at the right to minimise the group to its header (remembered on this
 *  device); the body shows the active tab's content. The header carries .cv-panel-head, so the dock moves the group by
 *  it as it moves the other panels. */
export interface GroupTab { id: string; label: string; body: HTMLElement }

const KEY = 'formvis.groups';
function loadState(): Record<string, { tab?: string; min?: boolean }> {
  try { return JSON.parse(localStorage.getItem(KEY) ?? '{}') as Record<string, { tab?: string; min?: boolean }>; } catch { return {}; }
}
function saveState(id: string, patch: { tab?: string; min?: boolean }): void {
  try { const s = loadState(); s[id] = { ...s[id], ...patch }; localStorage.setItem(KEY, JSON.stringify(s)); } catch { /* this session only */ }
}

const HOMES = 'formvis.tabHomes';

export class PanelGroup {
  /** Every group by id: a tab dragged onto another group's header moves there (the owner's walkthrough). */
  static readonly all = new Map<string, PanelGroup>();
  /** A group holds at most this many tabs (the owner's rule); a full one refuses more. */
  static readonly MAX_TABS = 7;
  get full(): boolean { return this.tabs.size >= PanelGroup.MAX_TABS; }
  /** Called after tabs move (the app re-lays out the columns). */
  static onMoved: (() => void) | null = null;
  /** A tab released over a column's gap, not a header: the app makes it a new group there (true if it did). */
  static onDetach: ((tid: string, from: PanelGroup, x: number, y: number) => boolean) | null = null;
  /** While a tab is dragged over a column's gap: where a new group would go (null: nowhere). */
  static onGapHint: ((x: number | null, y: number) => void) | null = null;
  get size(): number { return this.tabs.size; }
  labelOf(tid: string): string { return this.tabs.get(tid)?.btn.textContent ?? tid; }
  /** A group made by detaching, gone once empty. */
  remove(): void { PanelGroup.all.delete(this.id); this.root.remove(); }
  readonly root: HTMLElement;
  private readonly tabs = new Map<string, { btn: HTMLButtonElement; body: HTMLElement }>();
  private active = '';
  onTab: ((id: string) => void) | null = null;
  /** The panel menu (the target's •••): the app opens it with these command ids (PanelGroup.menuHost). */
  static menuHost: ((anchor: HTMLElement, ids: string[]) => void) | null = null;
  private menu: (() => string[]) | null = null;
  setMenu(items: () => string[]): void { this.menu = items; setHidden(this.root.querySelector('.cv-group-more') as HTMLElement, false); }

  constructor(host: HTMLElement, readonly id: string, list: readonly GroupTab[], label: string) {
    this.root = el(`<section class="cv-${id} cv-group" aria-label="${label}"><div class="cv-panel-head cv-group-head"><span class="cv-group-grip" title="Drag to move the group" aria-hidden="true">${icon('grip', 14)}</span><div class="cv-group-tabs" role="tablist"></div><span></span><button class="cv-group-more" type="button" data-cmd="panel-menu" title="Panel menu" aria-label="Panel menu" hidden>${icon('more3', 16)}</button><button class="cv-group-min" type="button" data-cmd="minimise" title="Minimise" aria-label="Minimise">${icon('minus', 12)}</button></div><div class="cv-group-body"></div></section>`);
    const strip = this.root.querySelector('.cv-group-tabs') as HTMLElement, body = this.root.querySelector('.cv-group-body') as HTMLElement;
    const more = this.root.querySelector('.cv-group-more') as HTMLElement;
    more.addEventListener('click', () => { const ids = this.menu?.() ?? []; if (ids.length) PanelGroup.menuHost?.(more, ids); });
    this.root.dataset.group = id;
    PanelGroup.all.set(id, this);
    for (const t of list) { t.body.classList.add('cv-group-page'); this.attach(t.id, t.label, t.body); }
    const saved = loadState()[id] ?? {};
    if (list.length) this.show(saved.tab && this.tabs.has(saved.tab) ? saved.tab : list[0].id, false);
    else this.root.classList.add('is-empty');   // a group made by detaching a tab, until its tab arrives
    this.setMinimised(!!saved.min, false);
    (this.root.querySelector('.cv-group-min') as HTMLElement).addEventListener('click', () => this.setMinimised(!this.root.classList.contains('is-min')));
    host.appendChild(this.root);
    // Tabs fit in steps (1.49.6, the owner's walkthrough: tabs were shortened to Chann..., Pa...): normal, then compact,
    // then tight lettering, and only then are inactive tabs shortened; again whenever the width or the tabs change.
    new ResizeObserver(() => this.fitTabs()).observe(strip);
  }

  fitTabs(): void {
    const strip = this.root.querySelector('.cv-group-tabs') as HTMLElement;
    if (!strip.clientWidth) return;
    for (const step of ['', 'is-compact', 'is-tight']) {
      strip.classList.remove('is-compact', 'is-tight');
      if (step) strip.classList.add(step);
      const need = [...strip.children].reduce((w, c) => w + (c as HTMLElement).scrollWidth, 0) + Math.max(0, strip.children.length - 1) * 2;
      if (need <= strip.clientWidth + 0.5) return;
    }
  }

  get current(): string { return this.active; }

  /** A tab's button and page in this group: its click shows it here; dragged onto another group's header, it moves. */
  /** Adds a tab (its page as it is), before the tab named, or last (the Layers group's Channels and Paths). */
  addTab(tid: string, label: string, page: HTMLElement, before?: string): void {
    page.classList.add('cv-group-page');
    this.attach(tid, label, page);
    const b = before ? this.tabs.get(before)?.btn : null, t = this.tabs.get(tid);
    if (b && t) b.before(t.btn);
    setHidden(page, this.active !== tid);
    if (!this.active || loadState()[this.id]?.tab === tid) this.show(tid, false);   // the tab last left, even added late
  }
  has(tid: string): boolean { return this.tabs.has(tid); }

  private attach(tid: string, label: string, page: HTMLElement): void {
    const strip = this.root.querySelector('.cv-group-tabs') as HTMLElement, body = this.root.querySelector('.cv-group-body') as HTMLElement;
    const btn = el<HTMLButtonElement>('<button class="cv-group-tab" type="button" role="tab"></button>');
    setText(btn, label);
    btn.title = label;   // the whole name, if the tab is shortened
    btn.dataset.tab = tid;
    btn.addEventListener('click', (e) => { if (PanelGroup.justDragged) { e.preventDefault(); PanelGroup.justDragged = false; return; } this.show(tid); });
    btn.addEventListener('pointerdown', (e) => this.tabDrag(e, tid, btn));
    strip.appendChild(btn);
    body.appendChild(page);
    this.tabs.set(tid, { btn, body: page });
    this.root.classList.remove('is-empty');
    requestAnimationFrame(() => this.fitTabs());
  }

  private static justDragged = false;
  /** Dragging a tab (in a group of several: a single tab's group moves whole, by the dock) onto another group's header. */
  private tabDrag(e: PointerEvent, tid: string, btn: HTMLButtonElement): void {
    if (e.button !== 0 || this.tabs.size < 2) return;
    const x0 = e.clientX, y0 = e.clientY;
    let ghost: HTMLElement | null = null, target: PanelGroup | null = null;
    const over = (ev: PointerEvent): PanelGroup | null => {
      const head = (document.elementFromPoint(ev.clientX, ev.clientY) as HTMLElement | null)?.closest('.cv-group-head');
      const g = head ? PanelGroup.all.get((head.closest('.cv-group') as HTMLElement | null)?.dataset.group ?? '') ?? null : null;
      return g && g !== this ? g : null;
    };
    const move = (ev: PointerEvent): void => {
      if (!ghost && Math.hypot(ev.clientX - x0, ev.clientY - y0) < 6) return;
      if (!ghost) { ghost = el('<div class="cv-tab-ghost"></div>'); setText(ghost, btn.textContent ?? ''); document.body.appendChild(ghost); }
      ghost.style.left = `${ev.clientX + 8}px`; ghost.style.top = `${ev.clientY + 8}px`;
      const t = over(ev);
      // A full group (7 tabs) says so and takes nothing.
      if (t !== target) { target?.root.classList.remove('is-tab-target', 'is-tab-full'); target = t; target?.root.classList.add(t?.full ? 'is-tab-full' : 'is-tab-target'); }
      PanelGroup.onGapHint?.(t ? null : ev.clientX, ev.clientY);   // over a gap: New group here
    };
    const up = (ev: PointerEvent): void => {
      PanelGroup.onGapHint?.(null, 0);
      document.removeEventListener('pointermove', move, true);
      document.removeEventListener('pointerup', up, true);
      ghost?.remove();
      target?.root.classList.remove('is-tab-target', 'is-tab-full');
      if (ghost) PanelGroup.justDragged = true;
      if (ghost && target) { if (!target.full) { PanelGroup.moveTab(tid, this, target); PanelGroup.saveHomes(); } }
      else if (ghost && PanelGroup.onDetach?.(tid, this, ev.clientX, ev.clientY)) PanelGroup.saveHomes();
    };
    document.addEventListener('pointermove', move, true);
    document.addEventListener('pointerup', up, true);
  }

  /** Moves a tab (its page, kept as it is) from one group to another; an emptied group is hidden. */
  static moveTab(tid: string, from: PanelGroup, to: PanelGroup): void {
    const t = from.tabs.get(tid);
    if (!t || from === to || to.full) return;   // never past seven tabs, dragged or restored
    const label = t.btn.textContent ?? tid;
    t.btn.remove();
    from.tabs.delete(tid);
    requestAnimationFrame(() => from.fitTabs());
    to.attach(tid, label, t.body);
    to.show(tid);
    if (from.tabs.size === 0) from.root.classList.add('is-empty');
    else if (from.active === tid) from.show(from.tabs.keys().next().value as string);
    PanelGroup.onMoved?.();
  }

  /** Where each tab lives, remembered; restored once every group exists. */
  static saveHomes(): void {
    const homes: Record<string, string> = {};
    for (const [gid, g] of PanelGroup.all) for (const tid of g.tabs.keys()) homes[tid] = gid;
    try { localStorage.setItem(HOMES, JSON.stringify(homes)); } catch { /* this session only */ }
  }
  static restoreHomes(): void {
    let homes: Record<string, string> = {};
    try { homes = JSON.parse(localStorage.getItem(HOMES) ?? '{}') as Record<string, string>; } catch { return; }
    for (const [tid, gid] of Object.entries(homes)) {
      const to = PanelGroup.all.get(gid), from = [...PanelGroup.all.values()].find((g) => g.tabs.has(tid));
      if (to && from && from !== to) PanelGroup.moveTab(tid, from, to);
    }
  }
  /** Window > Reset Panels: each group on its first tab and open. */
  static resetStates(): void {
    try { localStorage.removeItem(KEY); } catch { /* nothing kept */ }
    for (const g of PanelGroup.all.values()) { const first = g.tabs.keys().next().value as string | undefined; if (first) g.show(first, false); g.setMinimised(false, false); }
  }

  /** Window > Reset Panels: every tab back in the group it began in. */
  static resetHomes(defaults: Record<string, string>): void {
    try { localStorage.removeItem(HOMES); } catch { /* nothing kept */ }
    for (const [tid, gid] of Object.entries(defaults)) {
      const to = PanelGroup.all.get(gid), from = [...PanelGroup.all.values()].find((g) => g.tabs.has(tid));
      if (to && from && from !== to) PanelGroup.moveTab(tid, from, to);
    }
  }

  show(id: string, remember = true): void {
    if (!this.tabs.has(id)) return;
    this.active = id;
    for (const [tid, t] of this.tabs) {
      const on = tid === id;
      t.btn.classList.toggle('is-active', on);
      t.btn.setAttribute('aria-selected', String(on));
      setHidden(t.body, !on);
    }
    if (remember) saveState(this.id, { tab: id });
    this.onTab?.(id);
  }

  setMinimised(min: boolean, remember = true): void {
    this.root.classList.toggle('is-min', min);
    setHidden(this.root.querySelector('.cv-group-body') as HTMLElement, min);
    if (remember) saveState(this.id, { min });
  }
}
