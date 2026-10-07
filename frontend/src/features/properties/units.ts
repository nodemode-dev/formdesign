export type Unit = 'points' | 'picas' | 'inches' | 'inchesDecimal' | 'millimeters' | 'centimeters' | 'ciceros' | 'agates' | 'pixels' | 'q' | 'ha';

const PT_PER_MM = 72 / 25.4;
const PT_PER_DIDOT = 0.376065 * PT_PER_MM;
const PT_PER_CICERO = 12 * PT_PER_DIDOT;

const PER: Record<Unit, number> = {
  points: 1, pixels: 1, picas: 12, inches: 72, inchesDecimal: 72, millimeters: PT_PER_MM, centimeters: 72 / 2.54,
  ciceros: PT_PER_CICERO, agates: 72 / 14, q: 0.25 * PT_PER_MM, ha: 0.25 * PT_PER_MM,
};

const SUFFIX: Record<Unit, string> = {
  points: ' pt', picas: 'p', inches: ' in', inchesDecimal: ' in', millimeters: ' mm', centimeters: ' cm', ciceros: 'c',
  agates: ' ag', pixels: ' px', q: ' Q', ha: ' H',
};

const STEP: Record<Unit, number> = {
  points: 1, pixels: 1, picas: 1, inches: 4.5, inchesDecimal: 4.5, millimeters: PT_PER_MM, centimeters: PT_PER_MM,
  ciceros: PT_PER_DIDOT, agates: 72 / 14, q: 0.25 * PT_PER_MM, ha: 0.25 * PT_PER_MM,
};

export const LABELS: Record<Unit, string> = {
  points: 'Points', picas: 'Picas', inches: 'Inches', inchesDecimal: 'Inches Decimal', millimeters: 'Millimeters',
  centimeters: 'Centimeters', ciceros: 'Ciceros', agates: 'Agates', pixels: 'Pixels', q: 'Q', ha: 'Ha',
};

export function asUnit(v: unknown): Unit {
  const s = String(v ?? '');
  return (s in PER ? s : 'points') as Unit;
}

export function unitStep(u: Unit): number { return STEP[u]; }

function trim(v: number, decimals: number): string {
  let s = v.toFixed(decimals);
  if (s.includes('.')) s = s.replace(/0+$/, '').replace(/\.$/, '');
  return s === '-0' ? '0' : s;
}

export function formatMeasure(pt: number, unit: Unit, decimals = 3): string {
  if (unit === 'picas' || unit === 'ciceros') {
    const [big, small, sep] = unit === 'picas' ? [12, 1, 'p'] : [PT_PER_CICERO, PT_PER_DIDOT, 'c'];
    const neg = pt < 0;
    const a = Math.abs(pt);
    let whole = Math.floor(a / big + 1e-9);
    let rest = (a - whole * big) / small;
    if (rest > 12 - 1e-6) { whole += 1; rest = 0; }
    if (Math.abs(rest) < 1e-9) rest = 0;
    return `${neg && (whole || rest) ? '-' : ''}${whole}${sep}${trim(rest, decimals)}`;
  }
  return `${trim(pt / PER[unit], decimals)}${SUFFIX[unit]}`;
}

function parseSingle(tok: string, def: Unit, base: number | null): number | null {
  if (!tok) return null;
  if (tok.endsWith('%')) {
    const n = Number(tok.slice(0, -1));
    return Number.isFinite(n) && base !== null ? (n / 100) * base : null;
  }
  for (const [sep, big, small] of [['p', 12, 1], ['c', PT_PER_CICERO, PT_PER_DIDOT]] as const) {
    const pos = tok.indexOf(sep);
    if (pos < 0) continue;
    const a = tok.slice(0, pos), b = tok.slice(pos + 1);
    const ok = (s: string) => s === '' || /^\d*\.?\d+$|^\d+\.$/.test(s);
    if (ok(a) && ok(b) && !(a === '' && b === '')) return (a ? Number(a) : 0) * big + (b ? Number(b) : 0) * small;
  }
  const m = /^(\d*\.?\d+|\d+\.)(.*)$/.exec(tok);
  if (!m) return null;
  const n = Number(m[1]);
  const suf = m[2];
  const map: Record<string, Unit> = {
    '': def, pt: 'points', pts: 'points', point: 'points', points: 'points', px: 'pixels', pixel: 'pixels', pixels: 'pixels',
    in: 'inches', inch: 'inches', inches: 'inches', '"': 'inches', i: 'inches', mm: 'millimeters', millimeter: 'millimeters',
    millimeters: 'millimeters', cm: 'centimeters', centimeter: 'centimeters', centimeters: 'centimeters', ag: 'agates',
    agate: 'agates', agates: 'agates', q: 'q', h: 'ha', ha: 'ha',
  };
  const u = map[suf];
  return u ? n * PER[u] : null;
}

function parseProduct(term: string, def: Unit, base: number | null): number | null {
  const parts = term.split(/([*/])/);
  let val = parseSingle(parts[0], def, base);
  if (val === null) return null;
  for (let i = 1; i < parts.length; i += 2) {
    const x = Number(parts[i + 1]);
    if (!parts[i + 1] || !Number.isFinite(x)) return null;
    if (parts[i] === '*') val *= x;
    else { if (x === 0) return null; val /= x; }
  }
  return val;
}

export function parseMeasure(input: string, def: Unit, base: number | null = null): number | null {
  const s = input.replace(/\s+/g, '').toLowerCase();
  if (!s) return null;
  const terms: Array<[number, string]> = [];
  let sign = 1, cur = '';
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if ((ch === '+' || ch === '-') && cur && !/[*/]$/.test(cur)) { terms.push([sign, cur]); cur = ''; sign = ch === '-' ? -1 : 1; }
    else if ((ch === '+' || ch === '-') && !cur) sign *= ch === '-' ? -1 : 1;
    else cur += ch;
  }
  if (!cur) return null;
  terms.push([sign, cur]);
  let total = 0;
  for (const [sg, t] of terms) {
    const v = parseProduct(t, def, base);
    if (v === null) return null;
    total += sg * v;
  }
  return Number.isFinite(total) ? total : null;
}

export function parseNumber(input: string, suffix = ''): number | null {
  let s = input.trim().toLowerCase().replace(/\s+/g, '');
  if (suffix) s = s.replace(new RegExp(`${suffix.trim().toLowerCase().replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}$`), '');
  s = s.replace(/[%°]$/, '').replace(/pt$/, '');
  if (!s) return null;
  if (!/^[-+*/.\d()]+$/.test(s)) return null;
  const terms = s.match(/[-+]?[^-+]+/g);
  if (!terms) return null;
  let total = 0;
  for (const t of terms) {
    const parts = t.split(/([*/])/);
    let v = Number(parts[0]);
    if (!Number.isFinite(v)) return null;
    for (let i = 1; i < parts.length; i += 2) {
      const x = Number(parts[i + 1]);
      if (!Number.isFinite(x)) return null;
      if (parts[i] === '*') v *= x; else { if (!x) return null; v /= x; }
    }
    total += v;
  }
  return Number.isFinite(total) ? total : null;
}

export function fmtNum(v: number, decimals = 2): string { return trim(v, decimals); }
