import type { DockAdapter, DockSizes, DockStorage } from './adapter';
import { moveLegacy, readSeen, readState, type DockSide, type DockState } from './state';

export interface PlacementHooks { changed(): void; resizing(): void; settled(): void }

export interface Placement {
  readonly hosted: boolean;
  start(root: HTMLElement, grip: HTMLElement, hooks: PlacementHooks): void;
  isOpen(): boolean;
  setOpen(on: boolean): void;
  side(): DockSide;
  storedSide(): DockSide;
  canRight(): boolean;
  setSide(side: DockSide): void;
  keep(tab: string | null): void;
  savedTab(): string;
  seen(): boolean;
  markSeen(): void;
  shown(on: boolean): void;
}

const store = (): Storage | null => { try { return window.localStorage; } catch { return null; } };

class OwnPlacement implements Placement {
  readonly hosted = false;
  private state: DockState;
  private hooks: PlacementHooks | null = null;

  constructor(private readonly keyed: DockStorage, private readonly z: DockSizes, private readonly right: (() => boolean) | undefined) {
    const s = store();
    const defaults = { h: z.defaultHeight, w: z.defaultWidth };
    if (s) moveLegacy(s, keyed.key, keyed.legacy, defaults);
    this.state = s ? readState(s, keyed.key, keyed.legacy, defaults) : { open: false, side: 'bottom', h: defaults.h, w: defaults.w, tab: '' };
  }

  start(_root: HTMLElement, grip: HTMLElement, hooks: PlacementHooks): void {
    this.hooks = hooks;
    this.state.open = this.isOpen();
    this.apply();
    window.addEventListener('resize', () => { if (this.apply()) hooks.changed(); });
    this.drag(grip);
  }

  isOpen(): boolean {
    const at = document.documentElement.getAttribute('data-ft-dock');
    return at === null ? this.state.open : at === 'open';
  }

  setOpen(on: boolean): void {
    this.state.open = on;
    this.apply();
  }

  storedSide(): DockSide { return this.state.side; }

  side(): DockSide { return this.state.side === 'right' && this.canRight() ? 'right' : 'bottom'; }

  canRight(): boolean {
    if (this.right && !this.right()) return false;
    return this.z.keepWidth === undefined && !this.z.maxWidth ? true : this.maxWidth() >= this.z.minWidth;
  }

  setSide(side: DockSide): void {
    this.state.side = side;
    this.apply();
  }

  keep(tab: string | null): void {
    const s = store();
    if (!s) return;
    if (tab) this.state.tab = tab;
    const seen = readSeen(s, this.keyed.key);
    try { s.setItem(this.keyed.key, JSON.stringify(seen ? { ...this.state, seen } : this.state)); } catch { return; }
  }

  savedTab(): string { return this.state.tab; }

  seen(): boolean {
    const s = store();
    return s ? readSeen(s, this.keyed.key) : true;
  }

  markSeen(): void {
    const s = store();
    if (!s) return;
    try { s.setItem(this.keyed.key, JSON.stringify({ ...this.state, seen: true })); } catch { return; }
  }

  shown(): void { return; }

  private maxWidth(): number {
    return Math.min(window.innerWidth * this.z.maxWidthShare, window.innerWidth - (this.z.keepWidth ?? 0), this.z.maxWidth ? this.z.maxWidth() : Infinity);
  }

  private maxHeight(): number {
    return Math.min(window.innerHeight * this.z.maxHeightShare, window.innerHeight - (this.z.keepHeight ?? 0), this.z.maxHeight ? this.z.maxHeight() : Infinity);
  }

  private width(w: number): number { return Math.round(Math.max(this.z.minWidth, Math.min(this.maxWidth(), w))); }

  private height(h: number): number { return Math.round(Math.max(this.z.minHeight, Math.min(this.maxHeight(), h))); }

  private apply(): boolean {
    const html = document.documentElement;
    const want: Array<[string, string]> = [
      ['data-ft-dock', this.state.open ? 'open' : 'closed'], ['data-ft-dock-side', this.side()],
      ['data-ft-dock-h', String(this.height(this.state.h))], ['data-ft-dock-w', String(this.width(this.state.w))],
    ];
    let changed = false;
    for (const [name, value] of want) if (html.getAttribute(name) !== value) { html.setAttribute(name, value); changed = true; }
    const sizes: Array<[string, string]> = [['--ft-dock-size', `${this.height(this.state.h)}px`], ['--ft-dock-wsize', `${this.width(this.state.w)}px`]];
    for (const [name, value] of sizes) if (html.style.getPropertyValue(name) !== value) html.style.setProperty(name, value);
    return changed;
  }

  private drag(grip: HTMLElement): void {
    grip.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      grip.setPointerCapture(e.pointerId);
      grip.classList.add('is-dragging');
      const right = this.side() === 'right';
      const from = right ? this.width(this.state.w) : this.height(this.state.h);
      const at = right ? e.clientX : e.clientY;
      const move = (ev: PointerEvent): void => {
        const size = from + at - (right ? ev.clientX : ev.clientY);
        if (right) this.state.w = this.width(size); else this.state.h = this.height(size);
        if (this.apply()) this.hooks?.resizing();
      };
      const up = (): void => {
        grip.removeEventListener('pointermove', move);
        grip.removeEventListener('pointerup', up);
        grip.removeEventListener('pointercancel', up);
        grip.classList.remove('is-dragging');
        this.hooks?.settled();
      };
      grip.addEventListener('pointermove', move);
      grip.addEventListener('pointerup', up);
      grip.addEventListener('pointercancel', up);
    });
  }
}

class HostedPlacement implements Placement {
  readonly hosted = true;
  private on = false;
  private tab = '';

  start(root: HTMLElement): void { root.classList.add('is-hosted'); }
  isOpen(): boolean { return this.on; }
  setOpen(): void { return; }
  side(): DockSide { return 'bottom'; }
  storedSide(): DockSide { return 'bottom'; }
  canRight(): boolean { return false; }
  setSide(): void { return; }
  keep(tab: string | null): void { if (tab) this.tab = tab; }
  savedTab(): string { return this.tab; }
  seen(): boolean { return true; }
  markSeen(): void { return; }
  shown(on: boolean): void { this.on = on; }
}

export function placementFor(a: Pick<DockAdapter, 'placement'>): Placement {
  const p = a.placement;
  return p.mode === 'hosted' ? new HostedPlacement() : new OwnPlacement(p.storage, p.sizes, p.canDockRight);
}
