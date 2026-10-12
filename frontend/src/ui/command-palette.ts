import { editor } from '../editor/editor';
import type { Commands } from './commands';
import { Dialog } from './dialogs';
import { el, patchList, q, setClass, setHidden, setHtml, setText } from './dom';
import { icon } from './icons';
import type { MenuDef, MenuItem } from './menu';
import { toolIcon } from './tool-icons';

export function fuzzyScore(query: string, text: string): number | null {
  const qs = Array.from(query.toLowerCase()), raw = Array.from(text), t = raw.map((c) => c.toLowerCase()), m = qs.length, n = t.length;
  if (!m) return 0;
  if (m > n) return null;
  const opens = (j: number): boolean => j === 0 || /[\s>\-/(.,:]/.test(raw[j - 1]) || (raw[j] !== t[j] && raw[j - 1] === t[j - 1] && /[a-z]/.test(t[j - 1]));
  let prev = new Float64Array(n).fill(-Infinity), row = new Float64Array(n);
  for (let i = 0; i < m; i++) {
    let earlier = -Infinity;
    for (let j = 0; j < n; j++) {
      if (j >= 2) earlier = Math.max(earlier, prev[j - 2]);
      if (t[j] !== qs[i]) { row[j] = -Infinity; continue; }
      const own = 1 + (opens(j) ? 5 : 0) - j * 0.01, before = i === 0 ? 0 : Math.max(j >= 1 ? prev[j - 1] + 7 : -Infinity, earlier);
      row[j] = before === -Infinity ? -Infinity : before + own;
    }
    [prev, row] = [row, prev];
  }
  let best = -Infinity;
  for (let j = 0; j < n; j++) best = Math.max(best, prev[j]);
  return best === -Infinity ? null : Math.round(best) - Math.floor(n / 8);
}

function pathsOf(menus: MenuDef[]): Map<string, string> {
  const out = new Map<string, string>();
  const walk = (items: MenuItem[], path: string): void => {
    for (const it of items) {
      if (it === '-') continue;
      if (typeof it === 'object') walk(it.items, `${path} > ${it.label}`);
      else if (!out.has(it)) out.set(it, path);
    }
  };
  for (const m of menus) walk(typeof m.items === 'function' ? m.items() : m.items, m.label);
  return out;
}

interface Hit { key: string; label: string; meta: string; svg: string; enabled: boolean; score: number; run: () => void }

const SHOWN = 12;

export class CommandPalette extends Dialog {
  private readonly input = el<HTMLInputElement>('<input class="cv-input" type="search" placeholder="Search commands, panels and tools" aria-label="Search commands, panels and tools" spellcheck="false" autocomplete="off">');
  private readonly list = el('<div class="cv-palette-list" role="listbox" aria-label="Commands and tools"></div>');
  private readonly empty = el('<p class="cv-note cv-palette-empty" hidden>No command, panel or tool matches.</p>');
  private hits: Hit[] = [];
  private selected = 0;
  private paths = new Map<string, string>();
  private recent: string[] = [];

  constructor(private readonly commands: Commands, private readonly menus: () => MenuDef[]) {
    super('Search', 600);
    const box = el('<div class="cv-palette-box"></div>');
    box.append(this.list, this.empty);
    this.body.append(this.input, box);
    this.input.addEventListener('input', () => { this.selected = 0; this.render(); });
    this.input.addEventListener('keydown', (e) => {
      if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        this.selected = Math.max(0, Math.min(this.hits.length - 1, this.selected + (e.key === 'ArrowDown' ? 1 : -1)));
        this.mark();
      } else if (e.key === 'Enter') { e.preventDefault(); this.runHit(this.selected); }
    });
    this.list.addEventListener('mousedown', (e) => e.preventDefault());
    this.list.addEventListener('click', (e) => {
      const row = (e.target as HTMLElement).closest<HTMLElement>('.cv-palette-row');
      if (row) this.runHit(Number(row.dataset.key));
    });
  }

  show(): void {
    if (this.isOpen) return;
    this.paths = pathsOf(this.menus());
    this.input.value = '';
    this.selected = 0;
    this.render();
    this.open(this.input);
  }

  private runHit(i: number): void {
    const h = this.hits[i];
    if (!h?.enabled) return;
    this.recent = [h.key, ...this.recent.filter((k) => k !== h.key)].slice(0, SHOWN);
    this.close();
    h.run();
  }

  private render(): void {
    const query = this.input.value.trim(), hits: Hit[] = [];
    const first = (key: string): number => { const i = this.recent.indexOf(key); return query || i < 0 ? 0 : 1000 - i; };
    for (const c of this.commands.all()) {
      if (c.id === 'ui.search') continue;
      const label = this.commands.label(c).replace(/(\.\.\.|…)$/, ''), path = this.paths.get(c.id) ?? '';
      if (!label) continue;
      const own = fuzzyScore(query, label), wide = own === null ? fuzzyScore(query, `${label} ${path}`) : null;
      if (own === null && wide === null) continue;
      const score = own ?? (wide as number) - 10;
      const svg = c.id.startsWith('ui.panel.') ? icon('panelNav', 16) : '';
      hits.push({ key: c.id, label: c.id.startsWith('ui.panel.') ? `${label} panel` : label, meta: [path, this.commands.keyLabel(c)].filter(Boolean).join(' · '), svg, enabled: this.commands.enabled(c), score: score + first(c.id), run: () => { this.commands.run(c.id); } });
    }
    for (const t of editor.tools.flat()) {
      const key = `tool:${t.id}`, score = fuzzyScore(query, `${t.label} tool`);
      if (score === null) continue;
      hits.push({ key, label: /tool$/i.test(t.label) ? t.label : `${t.label} Tool`, meta: ['Tool', t.shortcut ?? ''].filter(Boolean).join(' · '), svg: toolIcon(t.icon, 16), enabled: editor.hasDoc, score: score + (query ? 2 : 0) + first(key), run: () => void editor.setTool(t.id) });
    }
    hits.sort((a, b) => b.score - a.score || a.label.localeCompare(b.label));
    this.hits = hits.slice(0, SHOWN);
    this.selected = Math.max(0, Math.min(this.selected, this.hits.length - 1));
    patchList(this.list, this.hits, {
      key: (h) => String(this.hits.indexOf(h)),
      create: (h) => { const n = el<HTMLButtonElement>('<button class="cv-list-row cv-palette-row" type="button" role="option" tabindex="-1"><span class="cv-palette-icon"></span><span class="cv-list-name"></span><span class="cv-list-meta"></span></button>'); this.fill(n, h); return n; },
      update: (n, h) => this.fill(n, h),
    });
    setHidden(this.empty, this.hits.length > 0);
    this.mark();
  }

  private fill(n: HTMLElement, h: Hit): void {
    setText(q(n, '.cv-list-name'), h.label);
    setText(q(n, '.cv-list-meta'), h.meta);
    setClass(n, 'is-off', !h.enabled);
    if (n.getAttribute('aria-disabled') !== String(!h.enabled)) n.setAttribute('aria-disabled', String(!h.enabled));
    setHtml(q(n, '.cv-palette-icon'), h.svg, h.key);
  }

  private mark(): void {
    Array.from(this.list.children).forEach((n, i) => {
      setClass(n, 'is-selected', i === this.selected);
      if (n.getAttribute('aria-selected') !== String(i === this.selected)) n.setAttribute('aria-selected', String(i === this.selected));
    });
  }
}
