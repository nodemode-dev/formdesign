import { editor, type PageInfo } from '../../editor/editor';
import type { ChangeEvent } from '../../engine/client';
import { el, q, setClass, setText } from '../../ui/dom';
import type { Workspace } from '../../ui/workspace';
import { fmt, readUnits, type Unit } from './units';

type Key = 'x' | 'y' | 'page' | 'w' | 'h' | 'size' | 'pages' | 'facing' | 'intent' | 'bleed' | 'stories' | 'items' | 'overset' | 'swatches' | 'layers';

const ROWS: Array<[Key, string] | '-'> = [
  ['x', 'X'], ['y', 'Y'], ['page', 'Page'], '-',
  ['w', 'W'], ['h', 'H'], '-',
  ['size', 'Size'], ['pages', 'Pages'], ['facing', 'Facing'], ['intent', 'Intent'], ['bleed', 'Bleed'],
  ['stories', 'Stories'], ['overset', 'Overset'], ['items', 'Objects'], ['swatches', 'Swatches'], ['layers', 'Layers'],
];

export class InfoPanel {
  readonly root = el('<div class="cv-info-body fd-info"></div>');
  private readonly vals = new Map<Key, HTMLElement>();
  private unit: Unit = readUnits();
  private pointer: { x: number; y: number } | null = null;
  private raf = 0;
  private factsTimer = 0;
  private sel: { w: number; h: number } | null = null;
  private facts: Partial<Record<Key, string>> = {};

  constructor(private readonly ws: Workspace) {
    for (const r of ROWS) {
      if (r === '-') { this.root.appendChild(el('<div class="fd-info-sep"></div>')); continue; }
      const n = el('<div class="cv-info-row"><span class="cv-info-key"></span><span class="cv-info-val"></span></div>');
      setText(q(n, '.cv-info-key'), r[1]);
      n.dataset.row = r[0];
      this.vals.set(r[0], q(n, '.cv-info-val'));
      this.root.appendChild(n);
    }
    const stage = q(ws.root, '.fd-stage');
    stage.addEventListener('pointermove', (e) => {
      const r = stage.getBoundingClientRect();
      this.pointer = { x: e.clientX - r.left, y: e.clientY - r.top };
      if (!this.raf) this.raf = requestAnimationFrame(() => { this.raf = 0; this.renderPointer(); });
    });
    editor.on('change', (e: ChangeEvent) => this.scheduleFacts(e?.docChanged ? 160 : 60));
    editor.on('selection', () => this.scheduleFacts(40));
    editor.on('docs', () => this.scheduleFacts(40));
    editor.on('layout', () => this.scheduleFacts(40));
    window.addEventListener('formdesign:units', () => { this.unit = readUnits(); this.renderPointer(); void this.loadFacts(); });
    this.render();
  }

  onShow(): void { this.unit = readUnits(); void this.loadFacts(); }

  private set(k: Key, v: string): void { setText(this.vals.get(k) ?? null, v); }

  private renderPointer(): void {
    const lay = editor.layout;
    const u = this.unit;
    if (!this.pointer || !lay || !editor.hasDoc) { this.set('x', ''); this.set('y', ''); this.set('page', ''); return; }
    const [cx, cy] = this.ws.viewport.toCanvas(this.pointer.x, this.pointer.y);
    let best: PageInfo | null = null, bd = Infinity;
    for (const p of lay.pages) {
      const r = p.rect;
      const dx = cx < r[0] ? r[0] - cx : cx > r[2] ? cx - r[2] : 0;
      const dy = cy < r[1] ? r[1] - cy : cy > r[3] ? cy - r[3] : 0;
      const d = dx * dx + dy * dy;
      if (d < bd) { bd = d; best = p; }
    }
    if (!best) return;
    this.set('x', `${fmt(cx - best.rect[0], u)} ${u}`);
    this.set('y', `${fmt(cy - best.rect[1], u)} ${u}`);
    this.set('page', bd === 0 ? best.name : `${best.name} (pasteboard)`);
  }

  private scheduleFacts(ms: number): void {
    window.clearTimeout(this.factsTimer);
    this.factsTimer = window.setTimeout(() => void this.loadFacts(), ms);
  }

  private async loadFacts(): Promise<void> {
    if (!editor.hasDoc) { this.sel = null; this.facts = {}; this.render(); return; }
    const ins = await editor.inspect();
    if (!ins) return;
    const u = this.unit;
    const ids = new Set<number>(editor.selection?.items ?? []);
    let box: [number, number, number, number] | null = null;
    let items = 0;
    for (const sp of ins.spreads ?? []) {
      for (const it of sp.items ?? []) {
        items++;
        if (!ids.has(it.id) || !Array.isArray(it.bounds)) continue;
        const b = it.bounds as number[];
        box = box ? [Math.min(box[0], b[0]), Math.min(box[1], b[1]), Math.max(box[2], b[2]), Math.max(box[3], b[3])] : [b[0], b[1], b[2], b[3]];
      }
    }
    this.sel = box ? { w: box[2] - box[0], h: box[3] - box[1] } : null;
    const st = ins.settings ?? {};
    const lay = editor.layout;
    const bleed = Array.isArray(lay?.bleed) ? Math.max(0, ...(lay?.bleed as number[])) : Number(lay?.bleed ?? 0);
    const stories = (ins.stories ?? []) as Array<{ overset?: boolean }>;
    const overset = stories.filter((s) => s.overset).length;
    this.facts = {
      size: st.pageWidth ? `${fmt(st.pageWidth, u)} × ${fmt(st.pageHeight, u)} ${u}` : '',
      pages: String(ins.pageCount ?? lay?.pages.length ?? ''),
      facing: st.facingPages ? 'Yes' : 'No',
      intent: st.intent ? String(st.intent).replace(/^./, (c: string) => c.toUpperCase()) : '',
      bleed: `${fmt(bleed, u)} ${u}`,
      stories: String(stories.length),
      overset: overset ? `${overset} ${overset === 1 ? 'story' : 'stories'}` : 'None',
      items: String(items),
      swatches: String(Math.max(0, (ins.swatches ?? []).filter((s: string) => !/^\[/.test(s)).length)),
      layers: String((ins.layers ?? []).length),
    };
    this.render();
  }

  private render(): void {
    const u = this.unit;
    this.set('w', this.sel ? `${fmt(this.sel.w, u)} ${u}` : '');
    this.set('h', this.sel ? `${fmt(this.sel.h, u)} ${u}` : '');
    for (const k of ['size', 'pages', 'facing', 'intent', 'bleed', 'stories', 'overset', 'items', 'swatches', 'layers'] as Key[]) this.set(k, this.facts[k] ?? '');
    setClass(this.vals.get('overset')?.parentElement ?? null, 'is-alert', !!this.facts.overset && this.facts.overset !== 'None');
    this.renderPointer();
  }
}
