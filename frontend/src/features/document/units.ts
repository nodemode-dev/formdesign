export type Unit = 'pt' | 'pc' | 'in' | 'mm' | 'px';

export const UNITS: Unit[] = ['pt', 'pc', 'in', 'mm', 'px'];
export const UNIT_NAMES: Record<Unit, string> = { pt: 'Points', pc: 'Picas', in: 'Inches', mm: 'Millimeters', px: 'Pixels' };
export const CORE_UNITS: Record<Unit, string> = { pt: 'points', pc: 'picas', in: 'inches', mm: 'millimeters', px: 'pixels' };
const PER: Record<Unit, number> = { pt: 1, pc: 12, in: 72, mm: 72 / 25.4, px: 1 };
const DECIMALS: Record<Unit, number> = { pt: 2, pc: 3, in: 3, mm: 2, px: 1 };
const KEY = 'formdes.units';

export const toPt = (v: number, u: Unit): number => v * PER[u];
export const fromPt = (pt: number, u: Unit): number => pt / PER[u];

export function isUnit(s: unknown): s is Unit { return typeof s === 'string' && (UNITS as string[]).includes(s); }

export function fmt(pt: number, u: Unit): string {
  const v = fromPt(pt, u);
  const d = DECIMALS[u];
  return String(Number(v.toFixed(d)));
}

export function fmtWith(pt: number, u: Unit): string { return `${fmt(pt, u)} ${u}`; }

export function savedUnits(): Unit | null {
  try { const v = localStorage.getItem(KEY); return isUnit(v) ? v : null; } catch { return null; }
}

export function readUnits(): Unit { return savedUnits() ?? 'pt'; }

export function saveUnits(u: Unit): void {
  try { localStorage.setItem(KEY, u); } catch { }
  window.dispatchEvent(new CustomEvent('formdes:units', { detail: { units: u } }));
}

/** Reads a length typed in a field: a plain number is in `u`; a number with its own unit (12mm, 1.5in, 3p6) is converted. */
export function parseLength(text: string, u: Unit): number | null {
  const s = text.trim().toLowerCase().replace(',', '.');
  if (!s) return null;
  const picas = s.match(/^(-?\d*\.?\d*)p(\d*\.?\d*)$/);
  if (picas && (picas[1] || picas[2])) return toPt(Number(picas[1] || 0) * 12 + Number(picas[2] || 0), 'pt');
  const m = s.match(/^(-?\d*\.?\d+)\s*(pt|pc|in|"|mm|cm|px)?$/);
  if (!m) return null;
  const n = Number(m[1]);
  if (!Number.isFinite(n)) return null;
  switch (m[2]) {
    case undefined: return toPt(n, u);
    case '"': return toPt(n, 'in');
    case 'cm': return toPt(n * 10, 'mm');
    default: return toPt(n, m[2] as Unit);
  }
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(n < 10240 ? 1 : 0)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}
