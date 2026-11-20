import type { DockAdapter } from './adapter';
import { el, q, setHidden, setText } from './dom';
import { contain } from './keys';
import { strings, type Say, type StringKey } from './strings';

export interface TourStep { target: string | HTMLElement; title: StringKey; body: StringKey; cmd?: string }

const find = (target: string | HTMLElement): Element | null => (typeof target === 'string' ? document.querySelector(target) : target);

const GAP = 12;
const EDGE = 8;
const PAD = 4;

const shown = (node: Element | null): node is HTMLElement => {
  if (!(node instanceof HTMLElement) || !node.isConnected) return false;
  const r = node.getBoundingClientRect();
  return r.width > 0 && r.height > 0;
};

export class Tour {
  private root: HTMLElement | null = null;
  private ring!: HTMLElement;
  private card!: HTMLElement;
  private count!: HTMLElement;
  private title!: HTMLElement;
  private body!: HTMLElement;
  private cmdRow!: HTMLElement;
  private cmd!: HTMLElement;
  private back!: HTMLButtonElement;
  private next!: HTMLButtonElement;
  private readonly s: Say;
  private steps: TourStep[] = [];
  private at = -1;
  private target: HTMLElement | null = null;
  private returnTo: HTMLElement | null = null;
  private done: (() => void) | null = null;
  private readonly watch = new ResizeObserver(() => this.place());
  private readonly onMove = (): void => this.place();
  private readonly onKey = (e: KeyboardEvent): void => {
    if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); this.end(true); }
  };

  constructor(private readonly a: DockAdapter) {
    this.s = strings(a);
  }

  isRunning(): boolean { return this.at >= 0; }

  start(steps: TourStep[], done: () => void): void {
    if (this.isRunning()) this.end(false);
    this.build();
    this.steps = steps;
    this.done = done;
    this.at = -1;
    if (!this.go(0, 1)) { this.done = null; return; }
    const active = document.activeElement;
    this.returnTo = active instanceof HTMLElement && active !== document.body ? active : null;
    window.addEventListener('resize', this.onMove);
    window.addEventListener('scroll', this.onMove, true);
    document.addEventListener('keydown', this.onKey, true);
    setHidden(this.root as HTMLElement, false);
    this.place();
    this.card.focus({ preventScroll: true });
  }

  private build(): void {
    if (this.root) return;
    const s = this.s;
    const c = this.a.classes;
    this.root = el(`<div class="ft-tour" hidden>
    <div class="ft-tour-ring"></div>
    <div class="ft-tour-card" role="dialog" aria-labelledby="ft-tour-title" aria-describedby="ft-tour-body" tabindex="-1">
      <p class="ft-tour-count"></p>
      <h2 class="ft-tour-title" id="ft-tour-title"></h2>
      <p class="ft-tour-body" id="ft-tour-body"></p>
      <div class="ft-tour-cmd" hidden><code class="ft-tour-cmd-text ft-notranslate" translate="no"></code><button type="button" class="${c.button} ft-tour-copy">${s('copy')}</button></div>
      <div class="ft-tour-nav">
        <button type="button" class="${c.button} ft-tour-x">${s('tourClose')}</button>
        <button type="button" class="${c.button} ft-tour-back">${s('tourBack')}</button>
        <button type="button" class="${c.button} ${c.primaryButton} ft-tour-next">${s('tourNext')}</button>
      </div>
    </div>
  </div>`);
    this.ring = q(this.root, '.ft-tour-ring');
    this.card = q(this.root, '.ft-tour-card');
    this.count = q(this.root, '.ft-tour-count');
    this.title = q(this.root, '.ft-tour-title');
    this.body = q(this.root, '.ft-tour-body');
    this.cmdRow = q(this.root, '.ft-tour-cmd');
    this.cmd = q(this.cmdRow, 'code');
    this.back = q<HTMLButtonElement>(this.root, '.ft-tour-back');
    this.next = q<HTMLButtonElement>(this.root, '.ft-tour-next');
    contain(this.card, this.a);
    q(this.root, '.ft-tour-copy').addEventListener('click', () => { void navigator.clipboard?.writeText(this.cmd.textContent ?? '').catch(() => undefined); });
    q(this.root, '.ft-tour-x').addEventListener('click', () => this.end(true));
    this.back.addEventListener('click', () => { this.go(this.at - 1, -1); this.place(); });
    this.next.addEventListener('click', () => {
      if (!this.go(this.at + 1, 1)) { this.end(true); return; }
      this.place();
    });
    document.body.append(this.root);
  }

  private go(from: number, dir: 1 | -1): boolean {
    for (let i = from; i >= 0 && i < this.steps.length; i += dir) {
      const at = this.steps[i];
      const node = at ? find(at.target) : null;
      if (!shown(node)) continue;
      this.at = i;
      this.watch.disconnect();
      this.target = node;
      this.watch.observe(node);
      this.fill();
      return true;
    }
    return false;
  }

  private fill(): void {
    const step = this.steps[this.at];
    if (!step) return;
    const before = this.steps.slice(0, this.at).filter((x) => shown(find(x.target))).length;
    const after = this.steps.slice(this.at + 1).filter((x) => shown(find(x.target))).length;
    setText(this.count, this.s('tourCount').replace('{n}', String(before + 1)).replace('{total}', String(before + 1 + after)));
    setText(this.title, this.s(step.title));
    setText(this.body, this.s(step.body));
    setText(this.cmd, step.cmd ?? '');
    setHidden(this.cmdRow, !step.cmd);
    const first = before === 0;
    const last = after === 0;
    setHidden(this.back, first);
    setText(this.next, last ? this.s('tourDone') : this.s('tourNext'));
  }

  private place(): void {
    if (!this.isRunning() || !this.root) return;
    if (!shown(this.target)) {
      if (!this.go(this.at + 1, 1) && !this.go(this.at - 1, -1)) { this.end(true); return; }
    }
    const r = (this.target as HTMLElement).getBoundingClientRect();
    const vw = window.innerWidth, vh = window.innerHeight;
    const box = { left: Math.max(0, r.left - PAD), top: Math.max(0, r.top - PAD), right: Math.min(vw, r.right + PAD), bottom: Math.min(vh, r.bottom + PAD) };
    this.put(this.ring, box.left, box.top, box.right - box.left, box.bottom - box.top);
    const cw = this.card.offsetWidth || 320, ch = this.card.offsetHeight || 160;
    const clampX = (x: number): number => Math.min(Math.max(EDGE, x), vw - cw - EDGE);
    const clampY = (y: number): number => Math.min(Math.max(EDGE, y), vh - ch - EDGE);
    const midX = clampX((box.left + box.right) / 2 - cw / 2), midY = clampY((box.top + box.bottom) / 2 - ch / 2);
    const spots: Array<[boolean, number, number]> = [
      [box.bottom + GAP + ch <= vh - EDGE, midX, box.bottom + GAP],
      [box.top - GAP - ch >= EDGE, midX, box.top - GAP - ch],
      [box.left - GAP - cw >= EDGE, box.left - GAP - cw, midY],
      [box.right + GAP + cw <= vw - EDGE, box.right + GAP, midY],
    ];
    const spot = spots.find((x) => x[0]);
    this.put(this.card, spot ? spot[1] : clampX(box.left + EDGE), spot ? spot[2] : clampY(box.top + EDGE), 0, 0);
  }

  private put(node: HTMLElement, x: number, y: number, w: number, h: number): void {
    const st = node.style;
    const tx = `translate(${Math.round(x)}px, ${Math.round(y)}px)`;
    if (st.transform !== tx) st.transform = tx;
    if (w > 0 && st.width !== `${Math.round(w)}px`) st.width = `${Math.round(w)}px`;
    if (h > 0 && st.height !== `${Math.round(h)}px`) st.height = `${Math.round(h)}px`;
  }

  end(finished: boolean): void {
    if (!this.isRunning()) return;
    this.at = -1;
    this.target = null;
    this.watch.disconnect();
    window.removeEventListener('resize', this.onMove);
    window.removeEventListener('scroll', this.onMove, true);
    document.removeEventListener('keydown', this.onKey, true);
    if (this.root) setHidden(this.root, true);
    const back = this.returnTo;
    this.returnTo = null;
    if (back?.isConnected) back.focus({ preventScroll: true });
    const done = this.done;
    this.done = null;
    if (finished) done?.();
  }
}
