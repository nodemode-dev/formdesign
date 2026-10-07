import { editor, type PageInfo, type Rect4, type SlotInfo } from '../../editor/editor';
import { PopupMenu, type MenuItem } from '../../ui/menu';
import { PanelGroup } from '../../ui/panel-group';
import type { Workspace } from '../../ui/workspace';

export interface InspectItem {
  id: number; kind: string; name: string; layer: number; hidden: boolean; locked: boolean; bounds: number[];
  story?: number; asset?: number; children?: InspectItem[];
}
export interface InspectLayer { id: number; name: string; color: number[]; visible: boolean; locked: boolean; printable: boolean }
export interface InspectPage { bounds: number[]; index: number; name: string; parent: string | null; side?: string; columns?: number }
export interface InspectSpread { index: number; items: InspectItem[]; pages: InspectPage[] }
export interface InspectParent { id: number; items: number; label: string }
export interface InspectStory { id: number; preview: string; frames: number[] }
export interface InspectData {
  activeLayer: number; layers: InspectLayer[]; parents: InspectParent[]; spreads: InspectSpread[]; stories: InspectStory[];
  settings: { facingPages: boolean; pageWidth: number; pageHeight: number; intent?: string }; pageCount: number;
}

export function intersects(a: number[], b: number[]): boolean { return a[0] < b[2] && a[2] > b[0] && a[1] < b[3] && a[3] > b[1]; }

export function spreadOf(slot: SlotInfo): { index: number; kind: string } {
  const s = slot.spread as unknown;
  if (typeof s === 'number') return { index: s, kind: 'doc' };
  const o = (s ?? {}) as { index?: number; kind?: string };
  return { index: Number(o.index ?? 0), kind: String(o.kind ?? 'doc') };
}

export function parentPrefix(label: string | null | undefined): string {
  if (!label) return '';
  const i = label.indexOf('-');
  return i > 0 ? label.slice(0, i) : label;
}

export function parentName(label: string): string {
  const i = label.indexOf('-');
  return i > 0 ? label.slice(i + 1) : label;
}

export function isShown(node: HTMLElement): boolean { return node.isConnected && node.getClientRects().length > 0; }

export function setStyle(node: HTMLElement, prop: string, value: string): void {
  if (node.style.getPropertyValue(prop) !== value) node.style.setProperty(prop, value);
}

export class Later {
  private t = 0;
  private due = 0;
  constructor(private readonly fn: () => void) {}
  run(ms: number): void {
    const at = performance.now() + ms;
    if (this.t && this.due <= at) return;
    window.clearTimeout(this.t);
    this.due = at;
    this.t = window.setTimeout(() => { this.t = 0; this.fn(); }, ms);
  }
}

export class Settle {
  private t = 0;
  private first = 0;
  constructor(private readonly fn: () => void, private readonly ms: number, private readonly max: number) {}
  poke(): void {
    const now = performance.now();
    if (!this.t) this.first = now;
    window.clearTimeout(this.t);
    const wait = Math.max(0, Math.min(this.ms, this.first + this.max - now));
    this.t = window.setTimeout(() => { this.t = 0; this.fn(); }, wait);
  }
}

export async function inspectNow(): Promise<InspectData | null> {
  const uid = editor.doc?.uid;
  if (uid === undefined) return null;
  const d = (await editor.inspect()) as InspectData | null;
  if (!d || editor.doc?.uid !== uid) return null;
  return d;
}

export function docPages(): PageInfo[] {
  const lay = editor.layout;
  return lay && !lay.editingParents ? lay.pages : [];
}

export function rangeText(abs: number[], pages: PageInfo[]): string {
  const sorted = [...new Set(abs)].filter((a) => a >= 0 && a < pages.length).sort((a, b) => a - b);
  const out: string[] = [];
  let i = 0;
  while (i < sorted.length) {
    let j = i;
    while (j + 1 < sorted.length && sorted[j + 1] === sorted[j] + 1) j++;
    const a = pages[sorted[i]]?.name ?? String(sorted[i] + 1), b = pages[sorted[j]]?.name ?? String(sorted[j] + 1);
    out.push(i === j ? a : `${a}-${b}`);
    i = j + 1;
  }
  return out.join(', ');
}

export function parsePages(text: string, pages: PageInfo[]): number[] | null {
  const find = (tok: string): number => {
    const t = tok.trim();
    if (!t) return -1;
    if (/^\+\d+$/.test(t)) { const n = Number(t.slice(1)) - 1; return n >= 0 && n < pages.length ? n : -1; }
    const hit = pages.find((p) => p.name.toLowerCase() === t.toLowerCase());
    if (hit) return hit.abs;
    return -1;
  };
  const out = new Set<number>();
  for (const part of text.split(/[,;]/)) {
    const p = part.trim();
    if (!p) continue;
    const whole = find(p);
    if (whole >= 0) { out.add(whole); continue; }
    const m = p.match(/^(.+?)\s*-\s*(.+)$/);
    if (!m) return null;
    const a = find(m[1]), b = find(m[2]);
    if (a < 0 || b < 0) return null;
    for (let k = Math.min(a, b); k <= Math.max(a, b); k++) out.add(k);
  }
  return out.size ? [...out].sort((x, y) => x - y) : null;
}

export function onceLayout(timeout = 1500): Promise<void> {
  return new Promise((resolve) => {
    let off: (() => void) | null = null;
    const t = window.setTimeout(() => { off?.(); resolve(); }, timeout);
    off = editor.on('layout', () => { window.clearTimeout(t); off?.(); resolve(); });
  });
}

export const PRESETS: Array<{ id: string; label: string; w: number; h: number }> = [
  { id: 'letter', label: 'Letter', w: 612, h: 792 },
  { id: 'legal', label: 'Legal', w: 612, h: 1008 },
  { id: 'tabloid', label: 'Tabloid', w: 792, h: 1224 },
  { id: 'a5', label: 'A5', w: 419.53, h: 595.28 },
  { id: 'a4', label: 'A4', w: 595.28, h: 841.89 },
  { id: 'a3', label: 'A3', w: 841.89, h: 1190.55 },
  { id: 'b5', label: 'B5', w: 498.9, h: 708.66 },
  { id: 'hd', label: '1920 × 1080', w: 1920, h: 1080 },
];

export function round(n: number): string { return String(Math.round(n * 100) / 100); }

export function sizeName(w: number, h: number): string {
  for (const p of PRESETS) {
    if (Math.abs(p.w - w) < 0.6 && Math.abs(p.h - h) < 0.6) return p.label;
    if (p.w !== p.h && Math.abs(p.h - w) < 0.6 && Math.abs(p.w - h) < 0.6) return `${p.label} Wide`;
  }
  return `${Math.round(w)} × ${Math.round(h)}`;
}

export function pageSize(r: Rect4): [number, number] { return [r[2] - r[0], r[3] - r[1]]; }

let popup: PopupMenu | null = null;
let escWired = false;

export function openMenu(ws: Workspace, items: MenuItem[], x: number, y: number): void {
  popup ??= new PopupMenu(ws.commands, null, true);
  if (!escWired) {
    escWired = true;
    window.addEventListener('keydown', (e) => { if (e.key === 'Escape' && popup?.isOpen) { e.stopPropagation(); popup.close(); } }, true);
    window.addEventListener('blur', () => popup?.close());
  }
  popup.close();
  popup.setItems(items);
  popup.openAt(x, y);
}

export function wireGroupMenus(ws: Workspace, menus: Record<string, () => MenuItem[]>): void {
  PanelGroup.menuHost ??= (anchor, ids) => { const r = anchor.getBoundingClientRect(); openMenu(ws, ids, r.right, r.bottom + 4); };
  const mine = Object.keys(menus);
  for (const g of PanelGroup.all.values()) {
    const count = mine.filter((id) => g.has(id)).length;
    if (!count || count !== g.size) continue;
    g.setMenu(() => {
      const fn = menus[g.current];
      return fn ? (fn().filter((x) => typeof x === 'string') as string[]) : [];
    });
  }
}

export function rgb(c: number[] | undefined): string {
  const [r, g, b] = c ?? [120, 120, 120];
  return `rgb(${r | 0}, ${g | 0}, ${b | 0})`;
}

export function words(text: string, n = 6): string {
  const clean = text.replace(/[-]/g, '').replace(/\s+/g, ' ').trim();
  const parts = clean.split(' ');
  return parts.length > n ? `${parts.slice(0, n).join(' ')}…` : clean;
}
