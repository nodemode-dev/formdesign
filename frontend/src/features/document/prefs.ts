import { api } from '../../engine/client';
import { Dialog } from '../../ui/dialogs';
import { el, escapeHtml, q, setChecked, setClass, setText, setTitle, setValue } from '../../ui/dom';
import { readChoice, setAccent, setIntensity, setTheme, THEMES } from '../../ui/themes';
import { toast } from '../../ui/toast';
import { copyText, Seg } from './form';
import { docIcon } from './icons';
import { capabilities, repoDir } from './paths';
import { fmt, readUnits, saveUnits, UNIT_NAMES, UNITS, type Unit } from './units';

const REFS: Array<[string, number, number]> = [
  ['Letter page', 612, 792], ['A4 page', 595.28, 841.89], ['Trade 6 × 9 page', 432, 648],
  ['Half-inch margin', 36, 0], ['Standard bleed', 9, 0], ['Body text size', 10, 0],
];

type Page = 'appearance' | 'units' | 'agents';

const ACCENTS: Array<{ hex: string; name: string }> = [
  { hex: '#f97316', name: 'Orange' }, { hex: '#aba0d3', name: 'Lavender' }, { hex: '#3b82f6', name: 'Blue' },
  { hex: '#06b6d4', name: 'Cyan' }, { hex: '#10b981', name: 'Green' }, { hex: '#eab308', name: 'Gold' },
  { hex: '#ef4444', name: 'Red' }, { hex: '#ec4899', name: 'Pink' }, { hex: '#a855f7', name: 'Purple' },
];

export class PreferencesDialog {
  private readonly d = new Dialog('Preferences', 780);
  private page: Page = 'appearance';
  private readonly nav = el('<nav class="cv-prefs-nav fd-pr-nav" aria-label="Sections"></nav>');
  private readonly pages = el('<div class="cv-prefs-pages fd-pr-pages"></div>');
  private readonly themes = el('<div class="fd-pr-themes"></div>');
  private readonly accents = el('<div class="fd-pr-accents"></div>');
  private readonly custom = el<HTMLInputElement>('<input class="fd-pr-color" type="color" aria-label="Custom accent colour" title="Custom colour">');
  private readonly accentHex = el('<span class="fd-pr-hex"></span>');
  private readonly intensity = el<HTMLInputElement>('<input class="cv-slider fd-pr-slider" type="range" min="0" max="150" step="5" aria-label="Accent intensity">');
  private readonly intensityOut = el('<span class="fd-pr-pct"></span>');
  private readonly units = new Seg<Unit>(UNITS.map((u) => ({ value: u, label: u, title: UNIT_NAMES[u] })), 'pt');
  private readonly unitName = el('<p class="fd-f-note"></p>');
  private readonly endpoint = el('<code class="fd-pr-code"></code>');
  private readonly mcpState = el('<span class="fd-pr-state"></span>');
  private readonly agentsOn = el<HTMLInputElement>('<input type="checkbox" checked>');
  private readonly lines = new Map<string, HTMLElement>();
  private readonly refs = el('<div class="fd-pr-refs"></div>');
  private readonly done: HTMLButtonElement;

  constructor() {
    q<HTMLElement>(this.d.scrim, '.cv-dialog').classList.add('fd-prefs');
    const shell = el('<div class="cv-prefs fd-pr"></div>');
    shell.append(this.nav, this.pages);
    this.d.body.appendChild(shell);
    this.d.body.classList.add('fd-pr-body');
    const tabs: Array<{ id: Page; label: string; icon: string }> = [
      { id: 'appearance', label: 'Appearance', icon: 'palette' },
      { id: 'units', label: 'Units', icon: 'ruler' },
      { id: 'agents', label: 'Agents', icon: 'terminal' },
    ];
    for (const t of tabs) {
      const b = el<HTMLButtonElement>(`<button class="cv-prefs-tab fd-pr-tab" type="button">${docIcon(t.icon, 15)}<span class="fd-pr-tabname"></span></button>`);
      setText(q(b, '.fd-pr-tabname'), t.label);
      b.dataset.page = t.id;
      b.addEventListener('click', () => this.show(t.id));
      this.nav.appendChild(b);
    }
    this.pages.append(this.appearancePage(), this.unitsPage(), this.agentsPage());
    this.done = this.d.button('Done', 'primary', () => this.d.close());
    document.addEventListener('nm:ui-settings-changed', () => { if (this.d.isOpen) this.sync(); });
  }

  open(page?: Page): void {
    this.show(page ?? this.page);
    this.sync();
    this.d.open(this.done);
    void this.loadAgents();
  }

  private show(p: Page): void {
    this.page = p;
    for (const b of Array.from(this.nav.children) as HTMLElement[]) setClass(b, 'is-active', b.dataset.page === p);
    for (const pg of Array.from(this.pages.children) as HTMLElement[]) { const on = pg.dataset.page === p; if (pg.hidden === on) pg.hidden = !on; }
  }

  private section(title: string, note = ''): HTMLElement {
    const s = el(`<section class="fd-pr-sec"><h3 class="fd-pr-h"></h3>${note ? '<p class="fd-f-note"></p>' : ''}</section>`);
    setText(q(s, '.fd-pr-h'), title);
    if (note) setText(q(s, '.fd-f-note'), note);
    return s;
  }

  private appearancePage(): HTMLElement {
    const pg = el('<div class="cv-prefs-page fd-pr-page"></div>');
    pg.dataset.page = 'appearance';
    for (const [id, th] of Object.entries(THEMES)) {
      const v = th.vars;
      const b = el<HTMLButtonElement>(`<button class="fd-pr-theme" type="button"><span class="fd-pr-tprev"><span class="fd-pr-tbar"></span><span class="fd-pr-tline"></span><span class="fd-pr-tline is-short"></span><span class="fd-pr-tdot"></span></span><span class="fd-pr-tname">${escapeHtml(th.label)}</span></button>`);
      b.dataset.theme = id;
      const prev = q<HTMLElement>(b, '.fd-pr-tprev');
      prev.style.background = v['--bg-secondary'] ?? '';
      prev.style.borderColor = v['--border-color'] ?? '';
      q<HTMLElement>(b, '.fd-pr-tbar').style.background = v['--bg-tertiary'] ?? '';
      for (const l of Array.from(b.querySelectorAll<HTMLElement>('.fd-pr-tline'))) l.style.background = v['--text-secondary'] ?? '';
      b.addEventListener('click', () => setTheme(id));
      this.themes.appendChild(b);
    }
    for (const a of ACCENTS) {
      const b = el<HTMLButtonElement>('<button class="fd-pr-accent" type="button"></button>');
      b.style.background = a.hex;
      b.dataset.hex = a.hex;
      setTitle(b, a.name);
      b.setAttribute('aria-label', a.name);
      b.addEventListener('click', () => setAccent(a.hex));
      this.accents.appendChild(b);
    }
    const customWrap = el('<label class="fd-pr-custom" title="Custom colour"></label>');
    customWrap.appendChild(this.custom);
    this.accents.append(customWrap, this.accentHex);
    this.custom.addEventListener('input', () => setAccent(this.custom.value));
    this.intensity.addEventListener('input', () => { setIntensity(Number(this.intensity.value) / 100); setText(this.intensityOut, `${this.intensity.value}%`); });
    const theme = this.section('Theme', 'The colours of every surface. Studio Dark is the form family’s own.');
    theme.appendChild(this.themes);
    const accent = this.section('Accent', 'Marks the active tool, the selection and the chosen items.');
    accent.appendChild(this.accents);
    const inten = this.section('Accent intensity', 'How strongly the accent tints what it marks.');
    const r = el('<div class="fd-pr-range"></div>');
    r.append(this.intensity, this.intensityOut);
    inten.appendChild(r);
    pg.append(theme, accent, inten);
    return pg;
  }

  private unitsPage(): HTMLElement {
    const pg = el('<div class="cv-prefs-page fd-pr-page" hidden></div>');
    pg.dataset.page = 'units';
    const s = this.section('Measurements', 'The unit the Info panel reads in, and the one new print documents start in.');
    s.append(this.units.root, this.unitName);
    this.units.root.classList.add('fd-pr-units');
    this.units.onChange = (u) => { saveUnits(u); this.syncUnits(u); };
    const r = this.section('In this unit');
    for (const [name, w, h] of REFS) {
      const row = el('<div class="fd-pr-ref"><span class="fd-pr-refname"></span><span class="fd-pr-refval"></span></div>');
      setText(q(row, '.fd-pr-refname'), name);
      row.dataset.w = String(w);
      row.dataset.h = String(h);
      this.refs.appendChild(row);
    }
    r.appendChild(this.refs);
    pg.append(s, r);
    return pg;
  }

  private syncUnits(u: Unit): void {
    this.units.set(u);
    setText(this.unitName, `${UNIT_NAMES[u]}. Type another unit in any field (12mm, 0.5in, 3p6) and it is converted.`);
    for (const row of Array.from(this.refs.children) as HTMLElement[]) {
      const w = Number(row.dataset.w), h = Number(row.dataset.h);
      setText(q(row, '.fd-pr-refval'), h ? `${fmt(w, u)} × ${fmt(h, u)} ${u}` : `${fmt(w, u)} ${u}`);
    }
  }

  private agentsPage(): HTMLElement {
    const pg = el('<div class="cv-prefs-page fd-pr-page" hidden></div>');
    pg.dataset.page = 'agents';
    const mcp = this.section('MCP endpoint', 'Agents read and lay out documents through this endpoint while formdes runs.');
    const ep = el(`<div class="fd-pr-line"></div>`);
    ep.append(this.endpoint, this.copyBtn(() => this.endpoint.textContent ?? ''));
    mcp.append(ep, this.mcpState);
    const access = this.section('Agent access', 'When this is off, agents connected over MCP can still list their tools but every call is refused, until you turn it back on.');
    const toggle = el('<label class="cv-opt"><span>Let agents read and change documents</span></label>');
    toggle.prepend(this.agentsOn);
    this.agentsOn.addEventListener('change', () => void api('/mcp/settings', { json: { enabled: this.agentsOn.checked } })
      .then(() => toast(this.agentsOn.checked ? 'Agents can work in formdes' : 'Agent access is off'))
      .catch((e) => { toast((e as Error).message, 'error'); this.agentsOn.checked = !this.agentsOn.checked; }));
    access.appendChild(toggle);
    pg.append(access, mcp);
    const rows: Array<{ id: string; title: string; note: string }> = [
      { id: 'claude', title: 'Claude Code', note: 'Adds formdes as an MCP server over stdio.' },
      { id: 'codex', title: 'Codex', note: 'Run in the formdes folder: installs the MCP server and the skill.' },
      { id: 'cursor', title: 'Cursor', note: 'Run in the formdes folder: the same command sets up Cursor’s MCP config too.' },
    ];
    for (const r of rows) {
      const s = this.section(r.title, r.note);
      const code = el('<code class="fd-pr-code"></code>');
      const line = el('<div class="fd-pr-line"></div>');
      line.append(code, this.copyBtn(() => code.textContent ?? ''));
      s.appendChild(line);
      this.lines.set(r.id, code);
      pg.appendChild(s);
    }
    return pg;
  }

  private copyBtn(text: () => string): HTMLButtonElement {
    const b = el<HTMLButtonElement>(`<button class="cv-icon-btn fd-pr-copy" type="button" title="Copy" aria-label="Copy">${docIcon('copy', 15)}</button>`);
    b.addEventListener('click', () => void copyText(text()).then((ok) => {
      if (!ok) { toast('Could not copy', 'error'); return; }
      setClass(b, 'is-done', true);
      window.setTimeout(() => setClass(b, 'is-done', false), 1200);
      toast('Copied');
    }));
    return b;
  }

  private sync(): void {
    const c = readChoice();
    for (const b of Array.from(this.themes.children) as HTMLElement[]) setClass(b, 'is-on', b.dataset.theme === c.theme);
    let preset = false;
    for (const b of Array.from(this.accents.querySelectorAll<HTMLElement>('.fd-pr-accent'))) {
      const on = b.dataset.hex?.toLowerCase() === c.accentColor.toLowerCase();
      preset ||= on;
      setClass(b, 'is-on', on);
    }
    setClass(this.custom.parentElement as HTMLElement, 'is-on', !preset);
    setValue(this.custom, /^#[0-9a-f]{6}$/i.test(c.accentColor) ? c.accentColor : '#f97316');
    setText(this.accentHex, c.accentColor.toUpperCase());
    const pct = Math.round((c.intensity ?? 1) * 100);
    setValue(this.intensity, String(pct));
    setText(this.intensityOut, `${pct}%`);
    this.syncUnits(readUnits());
  }

  private async loadAgents(): Promise<void> {
    const c = await capabilities();
    const repo = repoDir(c) || 'path/to/formdes';
    setText(this.endpoint, c.mcp ?? `${location.origin}/mcp`);
    setText(this.lines.get('claude') ?? null, `claude mcp add formdes -- ${repo}/formdes mcp`);
    setText(this.lines.get('codex') ?? null, `cd ${repo} && ./formdes agents install`);
    setText(this.lines.get('cursor') ?? null, `cd ${repo} && ./formdes agents install`);
    try {
      const s = await api<{ tools?: number; calls?: number; enabled?: boolean }>('/mcp/status');
      setChecked(this.agentsOn, s.enabled !== false);
      setText(this.mcpState, `${s.tools ?? 0} tools · ${s.calls ?? 0} calls this session`);
      setClass(this.mcpState, 'is-live', true);
    } catch {
      setText(this.mcpState, 'The endpoint answers while the engine runs.');
      setClass(this.mcpState, 'is-live', false);
    }
  }
}
