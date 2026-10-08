import '../styles/tabs.css';
import { editor } from '../editor/editor';
import type { ChangeEvent } from '../engine/client';
import { el, q, setClass, setHidden, setText, setValue } from '../ui/dom';
import type { Workspace } from '../ui/workspace';
import { fmt, parseLength, readUnits } from './document/units';

type Align = 'left' | 'center' | 'right' | 'char';
interface Stop { position: number; align: Align; leader?: string; alignOn?: string }

const ICON: Record<Align, string> = {
  left: '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M5 3v10h8"/></svg>',
  center: '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M8 3v10M3 13h10"/></svg>',
  right: '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M11 3v10H3"/></svg>',
  char: '<svg viewBox="0 0 16 16" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M8 3v10M3 13h10"/><circle cx="11" cy="9" r="0.9" fill="currentColor" stroke="none"/></svg>',
};
const NAMES: Record<Align, string> = { left: 'Left-justified tab', center: 'Centre-justified tab', right: 'Right-justified tab', char: 'Align to decimal (or another character)' };
const MARK: Record<Align, string> = {
  left: '<svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor"><path d="M2 0h2v8h7v2H2z"/></svg>',
  center: '<svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor"><path d="M5 0h2v8h4v2H1V8h4z"/></svg>',
  right: '<svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor"><path d="M8 0h2v10H1V8h7z"/></svg>',
  char: '<svg viewBox="0 0 12 12" width="12" height="12" fill="currentColor"><path d="M5 0h2v8h4v2H1V8h4z"/><circle cx="9" cy="5" r="1.2"/></svg>',
};

export class TabsPanel {
  readonly root = el(`<div class="ft-panel">
    <div class="cv-seg ft-align" role="radiogroup" aria-label="Tab alignment"></div>
    <div class="ft-fields">
      <div class="fp-num ft-num"><span class="fp-num-lab ft-lab">X</span><input class="fp-num-in ft-x" type="text" spellcheck="false" autocomplete="off" aria-label="Tab position"></div>
      <div class="fp-num ft-num"><span class="fp-num-lab ft-lab ft-wide">Leader</span><input class="fp-num-in ft-leader" type="text" maxlength="8" spellcheck="false" aria-label="Leader"></div>
      <div class="fp-num ft-num"><span class="fp-num-lab ft-lab ft-wide">On</span><input class="fp-num-in ft-on" type="text" maxlength="1" spellcheck="false" aria-label="Align on"></div>
    </div>
    <div class="ft-ruler" role="slider" aria-label="Tab stops"><div class="ft-scale"></div><div class="ft-stops"></div></div>
    <div class="ft-foot"><span class="ft-hint">Click the ruler to add a stop. Drag a stop down off the ruler to remove it.</span><button class="cv-btn ft-clear" type="button">Clear All</button></div>
    <div class="ft-empty" hidden>Select text or a text frame to set its tabs.</div>
  </div>`);
  private readonly ruler = q(this.root, '.ft-ruler');
  private readonly stopsBox = q(this.root, '.ft-stops');
  private readonly x = q<HTMLInputElement>(this.root, '.ft-x');
  private readonly leader = q<HTMLInputElement>(this.root, '.ft-leader');
  private readonly on = q<HTMLInputElement>(this.root, '.ft-on');
  private stops: Stop[] = [];
  private current = -1;
  private align: Align = 'left';
  private width = 432;
  private scaleKey = '';
  private drag: { i: number; x0: number; y0: number; moved: boolean; off: boolean } | null = null;
  private timer = 0;
  private visible = false;
  private hasText = false;

  constructor(private readonly ws: Workspace) {
    const seg = q(this.root, '.ft-align');
    for (const a of ['left', 'center', 'right', 'char'] as Align[]) {
      const b = el<HTMLButtonElement>(`<button type="button" role="radio" data-align="${a}">${ICON[a]}</button>`);
      b.title = NAMES[a];
      b.setAttribute('aria-label', NAMES[a]);
      b.addEventListener('click', () => this.setAlign(a));
      seg.appendChild(b);
    }
    this.x.addEventListener('change', () => this.fromX());
    this.x.addEventListener('keydown', (e) => { if (e.key === 'Enter') this.fromX(); });
    this.leader.addEventListener('change', () => this.edit({ leader: this.leader.value }));
    this.on.addEventListener('change', () => this.edit({ alignOn: this.on.value }));
    q(this.root, '.ft-clear').addEventListener('click', () => { this.stops = []; this.current = -1; this.render(); void this.commit(); });
    this.ruler.addEventListener('pointerdown', (e) => this.down(e));
    this.ruler.addEventListener('pointermove', (e) => this.move(e));
    this.ruler.addEventListener('pointerup', (e) => this.up(e));
    this.ruler.addEventListener('pointercancel', () => { this.drag = null; this.render(); });
    new ResizeObserver(() => this.render()).observe(this.ruler);
    editor.on('selection', () => this.schedule(60));
    editor.on('change', (e: ChangeEvent) => this.schedule(e?.docChanged ? 120 : 400));
    window.addEventListener('formdes:units', () => { this.scaleKey = ''; this.render(); });
    this.render();
  }

  onShow(): void { this.visible = true; this.schedule(0); }

  private schedule(ms: number): void {
    if (!this.visible) return;
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => void this.load(), ms);
  }

  private async load(): Promise<void> {
    if (this.drag) return;
    const a = editor.hasDoc ? await editor.exec<{ para?: { tabs?: Stop[]; leftIndent?: number } }>('type.selectionAttrs', {}, true).catch(() => undefined) : undefined;
    this.hasText = !!a?.para;
    const tabs = (a?.para?.tabs ?? []).map((t) => ({ position: t.position, align: (t.align ?? 'left') as Align, leader: t.leader ?? '', alignOn: t.alignOn ?? '' }));
    const same = tabs.length === this.stops.length && tabs.every((t, i) => t.position === this.stops[i].position && t.align === this.stops[i].align && t.leader === this.stops[i].leader && t.alignOn === this.stops[i].alignOn);
    if (!same) { this.stops = tabs; if (this.current >= tabs.length) this.current = tabs.length - 1; }
    this.width = await this.frameWidth();
    this.render();
  }

  private async frameWidth(): Promise<number> {
    const sel = editor.selection;
    const t = sel?.text as { frame?: number } | undefined;
    const id = sel?.items?.[0] ?? t?.frame;
    if (id == null) return Math.max(432, ...this.stops.map((s) => s.position + 36));
    const ins = await editor.inspect().catch(() => null) as { spreads?: Array<{ items?: Array<{ id: number; bounds?: number[] }> }> } | null;
    for (const sp of ins?.spreads ?? []) for (const it of sp.items ?? []) if (it.id === id && Array.isArray(it.bounds)) return Math.max(36, it.bounds[2] - it.bounds[0]);
    return 432;
  }

  private px(pos: number): number { return (pos / this.width) * this.ruler.clientWidth; }
  private pos(px: number): number { return Math.max(0, Math.min(this.width, (px / Math.max(1, this.ruler.clientWidth)) * this.width)); }

  private render(): void {
    const u = readUnits();
    const off = !this.hasText;
    setHidden(q(this.root, '.ft-empty'), !off);
    setClass(this.root, 'is-off', off);
    for (const b of this.root.querySelectorAll<HTMLButtonElement>('.ft-align button')) {
      const on = b.dataset.align === (this.current >= 0 ? this.stops[this.current].align : this.align);
      setClass(b, 'is-on', on);
      b.setAttribute('aria-checked', String(on));
    }
    const cur = this.current >= 0 ? this.stops[this.current] : null;
    setValue(this.x, cur ? fmt(cur.position, u) : '');
    setValue(this.leader, cur?.leader ?? '');
    setValue(this.on, cur?.alignOn ?? '');
    this.leader.disabled = this.on.disabled = this.x.disabled = !cur;
    const w = this.ruler.clientWidth;
    const key = `${w}/${this.width}/${u}`;
    if (w && key !== this.scaleKey) {
      this.scaleKey = key;
      const per = { pt: 1, pc: 12, in: 72, mm: 72 / 25.4, px: 1 }[u];
      const steps = u === 'in' ? [0.25, 0.5, 1, 2] : u === 'pc' ? [1, 2, 3, 6, 12] : [5, 10, 20, 50, 100, 200];
      const step = steps.find((s) => (s * per / this.width) * w >= 34) ?? steps[steps.length - 1];
      const marks: HTMLElement[] = [];
      for (let v = 0; v * per <= this.width + 0.01; v += step) {
        const m = el('<span class="ft-tick"></span>');
        m.style.transform = `translateX(${Math.round(this.px(v * per))}px)`;
        m.textContent = String(Math.round(v * 100) / 100);
        marks.push(m);
      }
      q(this.root, '.ft-scale').replaceChildren(...marks);
    }
    while (this.stopsBox.children.length > this.stops.length) this.stopsBox.lastElementChild?.remove();
    while (this.stopsBox.children.length < this.stops.length) {
      const m = el('<span class="ft-stop" role="button" tabindex="-1"></span>');
      this.stopsBox.appendChild(m);
    }
    this.stops.forEach((s, i) => {
      const m = this.stopsBox.children[i] as HTMLElement;
      if (m.dataset.align !== s.align) { m.dataset.align = s.align; m.innerHTML = MARK[s.align]; }
      m.dataset.i = String(i);
      const t = `translateX(${Math.round(this.px(s.position)) - 6}px)`;
      if (m.style.transform !== t) m.style.transform = t;
      setClass(m, 'is-current', i === this.current);
      setClass(m, 'is-off', !!this.drag?.off && this.drag.i === i);
      m.title = `${NAMES[s.align]} at ${fmt(s.position, u)} ${u}`;
    });
  }

  private down(e: PointerEvent): void {
    if (!this.hasText || e.button !== 0) return;
    this.ruler.setPointerCapture(e.pointerId);
    const r = this.ruler.getBoundingClientRect();
    const hit = (e.target as HTMLElement).closest<HTMLElement>('.ft-stop');
    let i = hit ? Number(hit.dataset.i) : -1;
    if (i < 0) {
      const align = this.current >= 0 ? this.stops[this.current].align : this.align;
      const added: Stop = { position: Math.round(this.pos(e.clientX - r.left) * 100) / 100, align, leader: '', alignOn: align === 'char' ? '.' : '' };
      this.stops.push(added);
      this.stops.sort((a, b) => a.position - b.position);
      i = this.stops.indexOf(added);
      this.drag = { i, x0: e.clientX, y0: e.clientY, moved: true, off: false };
    } else this.drag = { i, x0: e.clientX, y0: e.clientY, moved: false, off: false };
    this.current = i;
    this.render();
  }

  private move(e: PointerEvent): void {
    const d = this.drag;
    if (!d) return;
    const r = this.ruler.getBoundingClientRect();
    if (Math.abs(e.clientX - d.x0) > 2) d.moved = true;
    d.off = e.clientY - r.bottom > 24;
    if (d.moved) this.stops[d.i].position = Math.round(this.pos(e.clientX - r.left) * 100) / 100;
    this.render();
  }

  private up(_e: PointerEvent): void {
    const d = this.drag;
    this.drag = null;
    if (!d) return;
    if (d.off) { this.stops.splice(d.i, 1); this.current = -1; }
    else {
      const keep = this.stops[d.i];
      this.stops.sort((a, b) => a.position - b.position);
      this.current = this.stops.indexOf(keep);
    }
    this.render();
    if (d.moved || d.off) void this.commit();
  }

  private setAlign(a: Align): void {
    this.align = a;
    if (this.current >= 0) this.edit({ align: a, alignOn: a === 'char' ? this.stops[this.current].alignOn || '.' : '' });
    else this.render();
  }

  private fromX(): void {
    if (this.current < 0) return;
    const v = parseLength(this.x.value, readUnits());
    if (v == null) { this.render(); return; }
    const keep = this.stops[this.current];
    keep.position = Math.max(0, v);
    this.stops.sort((a, b) => a.position - b.position);
    this.current = this.stops.indexOf(keep);
    this.render();
    void this.commit();
  }

  private edit(patch: Partial<Stop>): void {
    if (this.current < 0) return;
    Object.assign(this.stops[this.current], patch);
    this.render();
    void this.commit();
  }

  private async commit(): Promise<void> {
    const tabs = this.stops.map((s) => ({ position: s.position, align: s.align, ...(s.leader ? { leader: s.leader } : {}), ...(s.alignOn ? { alignOn: s.alignOn } : {}) }));
    await editor.exec('type.para', { attrs: { tabs } });
    this.ws.viewport.refreshOverlays();
  }
}

export function installTabs(ws: Workspace): void {
  let panel: TabsPanel | null = null;
  ws.registerPanel({
    id: 'tabs', label: 'Tabs', group: 'typeextra', column: 'side', order: 20,
    create: () => (panel = new TabsPanel(ws)).root,
    onShow: () => panel?.onShow(),
  });
  ws.addCommands({ id: 'ui.tabs', label: 'Tabs', keys: ['Shift+Mod+T'], run: () => ws.showPanel('tabs') });
  ws.addMenuItems({ menu: 'Type', items: ['ui.tabs'], at: 'end' });
}
