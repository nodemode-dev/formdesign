import { api } from '../../engine/client';

export interface PackImage { id: string; name: string; size?: string; orientation?: string; role?: string; caption?: string; colors?: string[] }
export interface PackArticle { id: string; title: string; deck?: string; byline?: string; words?: number }
export interface PackBrief {
  text?: string; kinds?: string[]; medium?: string; tone?: string[]; colors?: string[];
  size?: { w: number; h: number; unit: string; hint?: string }; pages?: number; brand?: string; fields?: Record<string, string>;
}
export interface Pack {
  pack: string; title?: string; subtitle?: string; author?: string;
  facts?: { words?: number; articles?: number; images?: number; tables?: number; readingMinutes?: number };
  brief?: PackBrief; articles?: PackArticle[]; images?: PackImage[]; quotes?: string[]; palette?: string[]; warnings?: string[];
}

export interface TemplateCard {
  id: string; name: string; category: string; kinds: string[]; tags: string[]; tone: string[]; medium: string; description: string;
  size: [number, number]; unit: string; pages: number; facing: boolean; theme: string; slots: string[]; user: boolean; flows: string[];
}
export interface TemplateList { templates: TemplateCard[]; categories: Record<string, number>; total: number }
export interface Ranked { id: string; name: string; category: string; score: number; why: string[] }

export interface Report {
  template: string; filled?: string[]; kept?: string[]; placeholders?: string[]; emptied?: string[];
  fit?: Record<string, { trimmed?: string; shrunk?: number; overset?: boolean; error?: string }>;
  warnings?: string[]; addedPages?: number; preflight?: { errors: number; warnings: number; top?: string[] };
  unused?: { images?: string[]; articles?: string[] }; pages?: number; brand?: string; alternatives?: Array<{ id: string; score: number }>;
  next?: string[]; seconds?: number;
}
export interface MakeResult extends Partial<Report> { variations?: Report[] }

export interface Brand {
  name: string; palette?: Partial<Record<PaletteRole, string>>; fonts?: Partial<Record<FontRole, [string, string]>>;
  logo?: string; voice?: string; modified?: number;
}
export type PaletteRole = 'accent' | 'accent2' | 'ink' | 'paper' | 'dark' | 'light' | 'muted' | 'tint';
export type FontRole = 'display' | 'text' | 'accent' | 'mono';

export interface SlotRec { slot: string; kind: string; page: number; id: number; story?: number; text?: string; chars?: number; image?: string; frames?: number; bounds?: number[] }

export const TEMPLATE_COLOURS = '__template';

export function brandParam(v: string): unknown {
  if (!v) return undefined;
  if (v === TEMPLATE_COLOURS) return { name: 'Template colours' };
  return v;
}

let templateCache: Promise<TemplateList> | null = null;
export function templates(refresh = false): Promise<TemplateList> {
  if (!templateCache || refresh) templateCache = api<TemplateList>('/templates').catch((err) => { templateCache = null; throw err; });
  return templateCache;
}

export async function templateCard(id: string): Promise<TemplateCard | undefined> {
  try { return (await templates()).templates.find((t) => t.id === id); } catch { return undefined; }
}

export function thumbUrl(id: string, size: number, opts: { page?: number; brand?: string; v?: number } = {}): string {
  const q = new URLSearchParams({ size: String(size) });
  if (opts.page) q.set('page', String(opts.page));
  if (opts.brand) q.set('brand', opts.brand);
  if (opts.v) q.set('v', String(Math.round(opts.v)));
  return `/api/templates/${encodeURIComponent(id)}/thumb.png?${q.toString()}`;
}

export function packImageUrl(pack: string, ref: string): string {
  return `/api/intake/${encodeURIComponent(pack)}/image/${encodeURIComponent(ref)}`;
}

export function sizeLabel(size: [number, number], unit: string): string {
  const [w, h] = size;
  const trim = (n: number) => String(Math.round(n * 100) / 100);
  if (unit === 'px') return `${trim(w)} × ${trim(h)} px`;
  const quarter = (n: number) => Math.abs(n / 18 - Math.round(n / 18)) < 0.01;
  if (quarter(w) && quarter(h)) return `${trim(w / 72)} × ${trim(h / 72)} in`;
  return `${Math.round(w / 72 * 25.4)} × ${Math.round(h / 72 * 25.4)} mm`;
}

export function setRatio(node: HTMLElement, ratio: number): void {
  const r = (Number.isFinite(ratio) && ratio > 0 ? ratio : 0.77).toFixed(4);
  if (node.style.getPropertyValue('--ratio') !== r) node.style.setProperty('--ratio', r);
}

export function pagesLabel(n: number, facing?: boolean): string {
  return `${n} ${n === 1 ? 'page' : 'pages'}${facing && n > 1 ? ', facing' : ''}`;
}

export function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${Math.round(n / 1024)} KB`;
  return `${(n / 1024 / 1024).toFixed(n < 10 * 1024 * 1024 ? 1 : 0)} MB`;
}

export const CATEGORY_LABELS: Record<string, string> = {
  magazine: 'Magazine', newsletter: 'Newsletter', book: 'Book', report: 'Report', print: 'Print', stationery: 'Stationery',
  ad: 'Ad', web: 'Web', presentation: 'Presentation',
};

export function categoryLabel(c: string): string { return CATEGORY_LABELS[c] ?? c.charAt(0).toUpperCase() + c.slice(1); }

type Listener = () => void;
const brandListeners = new Set<Listener>();
let brandCache: Promise<Brand[]> | null = null;

export function brands(refresh = false): Promise<Brand[]> {
  if (!brandCache || refresh) brandCache = api<{ brands: Brand[] }>('/brands').then((r) => r.brands).catch(() => { brandCache = null; return []; });
  return brandCache;
}
export function onBrandsChanged(fn: Listener): void { brandListeners.add(fn); }
export function brandsChanged(): void { brandCache = null; for (const fn of brandListeners) fn(); }

export function syncBrandOptions(select: HTMLSelectElement, list: Brand[], fixed: Array<[string, string]>): void {
  const want: Array<[string, string]> = [...fixed, ...list.map((b) => [b.name, b.name] as [string, string])];
  const key = JSON.stringify(want);
  if (select.dataset.optKey === key) return;
  select.dataset.optKey = key;
  const keep = select.value;
  const opts = Array.from(select.options);
  want.forEach(([value, label], i) => {
    let o = opts[i];
    if (!o) { o = document.createElement('option'); select.appendChild(o); }
    if (o.getAttribute('value') !== value) o.setAttribute('value', value);
    if (o.textContent !== label) o.textContent = label;
  });
  for (const o of opts.slice(want.length)) o.remove();
  select.value = want.some(([v]) => v === keep) ? keep : want[0]?.[0] ?? '';
}
