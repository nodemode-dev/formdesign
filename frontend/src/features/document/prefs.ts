import { api } from '../../engine/client';
import { AiPage } from '../ai-settings';
import { confirmAction, Dialog } from '../../ui/dialogs';
import { el, q, setChecked, setClass, setHidden, setText, setTitle, setValue } from '../../ui/dom';
import { icon } from '../../ui/icons';
import { readChoice, setAccent, setIntensity, setTheme, themeSwatchesHtml, THEMES } from '../../ui/themes';
import { toast } from '../../ui/toast';
import { isLang, language, languages, setLanguage } from '../../ui/i18n';
import { copyText, Seg } from './form';
import { docIcon } from './icons';
import { capabilities, repoDir } from './paths';
import { fmt, readUnits, saveUnits, UNIT_NAMES, UNITS, type Unit } from './units';

const REFS: Array<[string, number, number]> = [
  ['Letter page', 612, 792], ['A4 page', 595.28, 841.89], ['Trade 6 × 9 page', 432, 648],
  ['Half-inch margin', 36, 0], ['Standard bleed', 9, 0], ['Body text size', 10, 0],
];

type Page = 'appearance' | 'units' | 'agents' | 'ai';

const SECTION_OF: Record<Page, string> = { appearance: 'Interface', units: 'Units & Rulers', agents: 'Plugins', ai: 'AI' };
const RAIL_ICONS: Record<string, string> = { General: 'pfGeneral', Interface: 'pfInterface', 'Units & Rulers': 'pfUnits', Plugins: 'pfPlugins', AI: 'pfAssistant' };
const RAIL_KEY = 'formdesign.prefsRailCollapsed';
const KEY_PREFIX = 'formdesign.';
const THEME_KEY = 'nodemode_ui_settings';

const ACCENTS: Array<{ hex: string; name: string }> = [
  { hex: '#f97316', name: 'Orange' }, { hex: '#aba0d3', name: 'Lavender' }, { hex: '#3b82f6', name: 'Blue' },
  { hex: '#06b6d4', name: 'Cyan' }, { hex: '#10b981', name: 'Green' }, { hex: '#eab308', name: 'Gold' },
  { hex: '#ef4444', name: 'Red' }, { hex: '#ec4899', name: 'Pink' }, { hex: '#a855f7', name: 'Purple' },
];

export class PreferencesDialog {
  private readonly d = new Dialog('Preferences', 780);
  private page: Page = 'appearance';
  private readonly nav = el('<nav class="cv-prefs-nav" role="tablist" aria-label="Sections"></nav>');
  private readonly pages = el('<div class="cv-prefs-pages"></div>');
  private readonly sections = new Map<string, HTMLElement>();
  private readonly themes = el('<div class="nm-theme-grid" role="group" aria-label="Theme"></div>');
  private readonly dataDir = el('<code class="fd-pr-code"></code>');
  private shell: { frame: HTMLElement; rail: HTMLElement; port: HTMLElement; shade: SVGPathElement; line: SVGPathElement } | null = null;
  private current = '';
  private spyRaf = 0;
  private wireRaf = 0;
  private spyMuted = 0;
  private wireD = '';
  private portY = -1;
  private readonly accents = el('<div class="fd-pr-accents"></div>');
  private readonly custom = el<HTMLInputElement>('<input class="fd-pr-color" type="color" aria-label="Custom accent colour" title="Custom colour">');
  private readonly accentHex = el('<span class="fd-pr-hex"></span>');
  private readonly intensity = el<HTMLInputElement>('<input class="cv-slider fd-pr-slider" type="range" min="0" max="150" step="5" aria-label="Accent intensity">');
  private readonly intensityOut = el('<span class="fd-pr-pct"></span>');
  private readonly units = new Seg<Unit>(UNITS.map((u) => ({ value: u, label: u, title: UNIT_NAMES[u] })), 'pt');
  private readonly unitName = el('<p class="cv-note"></p>');
  private readonly endpoint = el('<code class="fd-pr-code"></code>');
  private readonly mcpState = el('<span class="fd-pr-state"></span>');
  private readonly agentsOn = el<HTMLInputElement>('<input type="checkbox" checked>');
  private readonly lines = new Map<string, HTMLElement>();
  private readonly refs = el('<div class="fd-pr-refs"></div>');
  private readonly ai = new AiPage();
  private readonly done: HTMLButtonElement;

  constructor() {
    const frame = q<HTMLElement>(this.d.scrim, '.cv-dialog');
    frame.classList.add('cv-prefs-dlg');
    q<HTMLElement>(this.d.scrim, '.cv-dialog-head').prepend(el(`<span class="cv-prefs-headicon">${icon('settings', 16)}</span>`));
    const wrap = el('<div class="cv-prefs"></div>');
    wrap.appendChild(this.pages);
    this.d.body.appendChild(wrap);
    this.add('General', this.storageGroup(), this.resetGroup());
    this.add('Interface', ...this.interfaceGroups());
    this.add('Units & Rulers', ...this.unitsGroups());
    this.add('Plugins', ...this.pluginGroups());
    this.add('AI', this.ai.root);
    let collapsed = false;
    try { collapsed = localStorage.getItem(RAIL_KEY) === '1'; } catch { collapsed = false; }
    const fold = el<HTMLButtonElement>(`<button class="cv-prefs-fold" type="button">${icon('pfFold', 16)}</button>`);
    const paintRail = (): void => {
      setClass(frame, 'is-rail-collapsed', collapsed);
      setTitle(fold, collapsed ? 'Expand tabs' : 'Collapse tabs');
      fold.setAttribute('aria-label', fold.title);
      fold.setAttribute('aria-expanded', String(!collapsed));
    };
    fold.addEventListener('click', () => {
      collapsed = !collapsed;
      try { localStorage.setItem(RAIL_KEY, collapsed ? '1' : '0'); } catch { }
      paintRail();
      this.queueWire();
    });
    paintRail();
    const rail = el('<div class="cv-prefs-rail"></div>');
    rail.append(this.nav, fold);
    const wire = el<HTMLElement>('<svg class="cv-prefs-wire" aria-hidden="true"><path class="cv-prefs-wire-shade"></path><path class="cv-prefs-wire-line"></path></svg>') as unknown as SVGSVGElement;
    const port = el('<span class="cv-prefs-port" aria-hidden="true"></span>');
    frame.append(rail, wire, port);
    this.shell = { frame, rail, port, shade: q<SVGPathElement>(wire, '.cv-prefs-wire-shade'), line: q<SVGPathElement>(wire, '.cv-prefs-wire-line') };
    this.pages.addEventListener('scroll', () => this.queueSpy(), { passive: true });
    new ResizeObserver(() => this.queueWire()).observe(frame);
    window.addEventListener('resize', () => this.queueWire());
    this.done = this.d.button('Done', 'primary', () => this.d.close());
    document.addEventListener('nm:ui-settings-changed', () => { if (this.d.isOpen) this.sync(); });
  }

  open(page?: Page): void {
    if (page) this.page = page;
    this.sync();
    this.d.open(this.done);
    if (page) { this.markSection(SECTION_OF[page]); this.scrollToSection(SECTION_OF[page], 'auto'); }
    else this.markSection(this.current || 'General');
    this.queueWire();
    void this.loadAgents();
    void this.ai.load();
    void this.loadStorage();
  }

  private add(name: string, ...content: HTMLElement[]): void {
    const ic = RAIL_ICONS[name] ?? 'pfGeneral';
    const tab = el<HTMLButtonElement>(`<button class="cv-prefs-railtab" type="button" role="tab"><span class="cv-prefs-tabicon">${icon(ic, 18)}</span><span class="cv-prefs-tablabel"></span></button>`);
    tab.dataset.section = name;
    setText(q(tab, '.cv-prefs-tablabel'), name);
    tab.title = name;
    tab.addEventListener('click', () => this.showSection(name, 'smooth'));
    this.nav.appendChild(tab);
    const page = el(`<section class="cv-prefs-page cv-prefs-sec" role="tabpanel"><button class="cv-prefs-sechead" type="button" aria-expanded="true"><span class="cv-prefs-secicon">${icon(ic, 16)}</span><span class="cv-prefs-seclabel"></span><span class="cv-prefs-chev">${icon('chevronDown', 16)}</span></button><div class="cv-prefs-secbody"></div></section>`);
    page.dataset.section = name;
    setText(q(page, '.cv-prefs-seclabel'), name);
    q(page, '.cv-prefs-sechead').addEventListener('click', () => this.foldSection(name));
    q(page, '.cv-prefs-secbody').append(...content);
    this.pages.appendChild(page);
    this.sections.set(name, page);
  }

  private showSection(name: string, scroll: ScrollBehavior | null = null): void {
    if (!this.sections.has(name)) return;
    this.markSection(name);
    if (scroll) this.scrollToSection(name, scroll);
  }

  private markSection(name: string): void {
    if (this.current === name) return;
    this.current = name;
    for (const [n, page] of this.sections) setClass(page, 'is-current', n === name);
    for (const tab of Array.from(this.nav.children) as HTMLElement[]) { const on = tab.dataset.section === name; setClass(tab, 'is-active', on); tab.setAttribute('aria-selected', String(on)); }
    this.queueWire();
  }

  private scrollToSection(name: string, behavior: ScrollBehavior): void {
    const sec = this.sections.get(name);
    if (!sec) return;
    if (sec.classList.contains('is-folded')) this.foldSection(name);
    const top = sec.getBoundingClientRect().top - this.pages.getBoundingClientRect().top + this.pages.scrollTop - 12;
    this.spyMuted = performance.now() + 700;
    this.pages.scrollTo({ top, behavior });
  }

  private foldSection(name: string): void {
    const sec = this.sections.get(name);
    if (!sec) return;
    const folded = !sec.classList.contains('is-folded');
    setClass(sec, 'is-folded', folded);
    setHidden(q(sec, '.cv-prefs-secbody'), folded);
    q(sec, '.cv-prefs-sechead').setAttribute('aria-expanded', String(!folded));
    this.queueWire();
    this.queueSpy();
  }

  private queueSpy(): void {
    if (this.spyRaf) return;
    this.spyRaf = requestAnimationFrame(() => {
      this.spyRaf = 0;
      const pages = this.pages;
      if (!pages.clientHeight || performance.now() < this.spyMuted) return;
      const edge = pages.getBoundingClientRect().top + 24;
      const names = Array.from(this.sections.keys());
      let best = '', bestD = Infinity;
      if (pages.scrollHeight > pages.clientHeight + 4 && pages.scrollTop + pages.clientHeight >= pages.scrollHeight - 2) best = names[names.length - 1] ?? '';
      else for (const [n, sec] of this.sections) {
        const top = sec.getBoundingClientRect().top, d = top <= edge ? edge - top : top - edge + 100000;
        if (d < bestD) { bestD = d; best = n; }
      }
      if (best) this.markSection(best);
    });
  }

  private queueWire(): void {
    if (this.wireRaf) return;
    this.wireRaf = requestAnimationFrame(() => { this.wireRaf = 0; this.drawWire(); });
  }

  private drawWire(): void {
    const s = this.shell;
    if (!s || !this.d.isOpen) return;
    const F = s.frame.getBoundingClientRect();
    const tab = this.nav.querySelector<HTMLElement>('.cv-prefs-railtab.is-active');
    const head = s.frame.querySelector<HTMLElement>('.cv-dialog-head');
    if (!F.width || !tab || !head) return;
    const N = this.nav.getBoundingClientRect();
    let T = tab.getBoundingClientRect();
    if (T.top < N.top || T.bottom > N.bottom) {
      this.nav.scrollTop += T.top < N.top ? T.top - N.top - 6 : T.bottom - N.bottom + 6;
      T = tab.getBoundingClientRect();
    }
    const R = s.rail.getBoundingClientRect(), H = head.getBoundingClientRect();
    const portY = Math.round(H.bottom - F.top), sx = Math.round(T.right - F.left), ty = Math.round(T.top - F.top + T.height / 2);
    const rr = R.right - F.left, trunk = Math.round(rr - rr * 0.45), dy = portY - ty, r = 10, dir = dy > 0 ? 1 : -1;
    const d = Math.abs(dy) < 2 * r ? `M ${sx} ${ty} H ${trunk} V ${portY} H -7` : `M ${sx} ${ty} H ${trunk - r} a ${r} ${r} 0 0 ${dir > 0 ? 1 : 0} ${r} ${dir * r} V ${portY - dir * r} a ${r} ${r} 0 0 ${dir > 0 ? 0 : 1} ${r} ${dir * r} H -7`;
    if (d !== this.wireD) { this.wireD = d; s.shade.setAttribute('d', d); s.line.setAttribute('d', d); }
    if (portY !== this.portY) { this.portY = portY; s.port.style.transform = `translateY(${portY}px)`; }
  }

  private group(title: string, note = ''): HTMLElement {
    const g = el('<section class="cv-prefs-group"><h3 class="cv-prefs-label"></h3></section>');
    setText(q(g, '.cv-prefs-label'), title);
    if (note) g.appendChild(this.note(note));
    return g;
  }

  private note(text: string): HTMLElement {
    const n = el('<p class="cv-note"></p>');
    setText(n, text);
    return n;
  }

  private storageGroup(): HTMLElement {
    const g = this.group('Settings Storage');
    const line = el('<div class="fd-pr-line"></div>');
    line.append(this.dataDir, this.copyBtn(() => this.dataDir.textContent ?? ''));
    g.append(line, this.note('Documents, templates and the engine’s stores live in this folder. The window’s own preferences stay in the browser or app storage.'));
    return g;
  }

  private resetGroup(): HTMLElement {
    const g = this.group('Reset');
    const b = el<HTMLButtonElement>('<button class="cv-btn cv-btn-danger" type="button">Reset All Preferences...</button>');
    b.addEventListener('click', () => void confirmAction('Reset all preferences?', 'Every preference, the panel arrangement and the theme go back to how FormDesign comes; your documents are kept. FormDesign reloads.', 'Reset', true).then((ok) => {
      if (!ok) return;
      try {
        for (const k of Object.keys(localStorage)) if (k.startsWith(KEY_PREFIX) || k === THEME_KEY) localStorage.removeItem(k);
      } catch { }
      location.reload();
    }));
    g.append(b);
    return g;
  }

  private async loadStorage(): Promise<void> {
    const c = await capabilities();
    setText(this.dataDir, c.dataDir ?? 'Starts with the engine');
  }

  private interfaceGroups(): HTMLElement[] {
    const tpl = document.createElement('template');
    tpl.innerHTML = themeSwatchesHtml();
    this.themes.append(tpl.content);
    this.themes.addEventListener('click', (e) => {
      const id = (e.target as HTMLElement).closest<HTMLElement>('[data-theme-id]')?.dataset.themeId;
      if (id) setTheme(id);
    });
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
    const theme = this.group('Theme', 'The colours of every surface. Studio Dark is the form family’s own.');
    theme.appendChild(this.themes);
    const accent = this.group('Accent', 'Marks the active tool, the selection and the chosen items.');
    accent.appendChild(this.accents);
    const inten = this.group('Accent intensity', 'How strongly the accent tints what it marks.');
    const r = el('<div class="fd-pr-range"></div>');
    r.append(this.intensity, this.intensityOut);
    inten.appendChild(r);
    const lang = this.group('Language', 'The language of menus, panels and dialogs. Document text is never changed.');
    const pick = el<HTMLSelectElement>('<select class="cv-select fd-pr-lang" aria-label="Language" translate="no" data-no-i18n></select>');
    for (const [code, name] of languages()) pick.appendChild(new Option(name, code));
    pick.value = language();
    pick.addEventListener('change', () => { if (isLang(pick.value)) void setLanguage(pick.value); });
    lang.appendChild(pick);
    return [theme, accent, inten, lang];
  }

  private unitsGroups(): HTMLElement[] {
    const s = this.group('Measurements', 'The unit the Info panel reads in, and the one new print documents start in.');
    s.append(this.units.root, this.unitName);
    this.units.root.classList.add('fd-pr-units');
    this.units.onChange = (u) => { saveUnits(u); this.syncUnits(u); };
    const r = this.group('In this unit');
    for (const [name, w, h] of REFS) {
      const row = el('<div class="fd-pr-ref"><span class="fd-pr-refname"></span><span class="fd-pr-refval"></span></div>');
      setText(q(row, '.fd-pr-refname'), name);
      row.dataset.w = String(w);
      row.dataset.h = String(h);
      this.refs.appendChild(row);
    }
    r.appendChild(this.refs);
    return [s, r];
  }

  private syncUnits(u: Unit): void {
    this.units.set(u);
    setText(this.unitName, `${UNIT_NAMES[u]}. Type another unit in any field (12mm, 0.5in, 3p6) and it is converted.`);
    for (const row of Array.from(this.refs.children) as HTMLElement[]) {
      const w = Number(row.dataset.w), h = Number(row.dataset.h);
      setText(q(row, '.fd-pr-refval'), h ? `${fmt(w, u)} × ${fmt(h, u)} ${u}` : `${fmt(w, u)} ${u}`);
    }
  }

  private pluginGroups(): HTMLElement[] {
    const out: HTMLElement[] = [];
    const mcp = this.group('MCP endpoint', 'Agents read and lay out documents through this endpoint while FormDesign runs.');
    const ep = el(`<div class="fd-pr-line"></div>`);
    ep.append(this.endpoint, this.copyBtn(() => this.endpoint.textContent ?? ''));
    mcp.append(ep, this.mcpState);
    const access = this.group('Agent access', 'When this is off, agents connected over MCP can still list their tools but every call is refused, until you turn it back on.');
    const toggle = el('<label class="cv-opt"><span>Let agents read and change documents</span></label>');
    toggle.prepend(this.agentsOn);
    this.agentsOn.addEventListener('change', () => void api('/mcp/settings', { json: { enabled: this.agentsOn.checked } })
      .then(() => toast(this.agentsOn.checked ? 'Agents can work in FormDesign' : 'Agent access is off'))
      .catch((e) => { toast((e as Error).message, 'error'); this.agentsOn.checked = !this.agentsOn.checked; }));
    access.appendChild(toggle);
    out.push(this.claudeSection(), access, mcp);
    const rows: Array<{ id: string; title: string; note: string }> = [
      { id: 'claude', title: 'Claude Code', note: 'Adds FormDesign as an MCP server over stdio.' },
      { id: 'codex', title: 'Codex', note: 'Run in the FormDesign folder: installs the MCP server and the skill.' },
      { id: 'cursor', title: 'Cursor', note: 'Run in the FormDesign folder: the same command sets up Cursor’s MCP config too.' },
    ];
    for (const r of rows) {
      const s = this.group(r.title, r.note);
      const code = el('<code class="fd-pr-code"></code>');
      const line = el('<div class="fd-pr-line"></div>');
      line.append(code, this.copyBtn(() => code.textContent ?? ''));
      s.appendChild(line);
      this.lines.set(r.id, code);
      out.push(s);
    }
    return out;
  }

  private claudeSection(): HTMLElement {
    const s = this.group('Claude Code in FormDesign', 'The Claude Code tab and the Chat tab use your own Claude Code sign-in, connected to FormDesign’s tools. Leave a field empty for the default.');
    const state = el('<p class="cv-note fd-pr-claude-state">Checking for Claude Code…</p>');
    const fields: Array<[string, string, string]> = [
      ['claudeFolder', 'Starts in folder', 'Agent folder in your FormDesign data folder'],
      ['claudeBin', 'Claude Code program', 'found automatically'],
      ['claudeFirstMessage', 'Start with message', 'nothing; Claude Code waits for you'],
    ];
    const found = (r: { installed?: boolean; version?: string | null; folder?: string; install?: string; binNote?: string }): string => {
      const text = r.installed ? `Claude Code ${r.version ?? ''} is installed. It starts in ${r.folder ?? ''}.` : String(r.install ?? 'Claude Code is not installed: install it from claude.com/claude-code and run claude once in a terminal to sign in.');
      return r.binNote ? `${text} ${r.binNote}` : text;
    };
    const inputs = new Map<string, HTMLInputElement>();
    for (const [key, label, ph] of fields) {
      const row = el('<label class="fd-pr-field"><span></span></label>');
      setText(q(row, 'span'), label);
      const input = el<HTMLInputElement>('<input class="cv-input" type="text" spellcheck="false">');
      input.placeholder = ph;
      input.addEventListener('change', () => void api('/claude/prefs', { json: { [key]: input.value.trim() } })
        .then(() => api<Parameters<typeof found>[0]>('/claude/status'))
        .then((r) => { setText(state, found(r)); toast('Saved'); })
        .catch((e) => toast((e as Error).message, 'error')));
      row.appendChild(input);
      inputs.set(key, input);
      s.appendChild(row);
    }
    s.insertBefore(state, s.querySelector('.fd-pr-field'));
    void api<Record<string, string | boolean | null>>('/claude/status').then((r) => {
      for (const [key, input] of inputs) input.value = String(r[key] ?? '');
      setText(state, found(r as Parameters<typeof found>[0]));
    }).catch(() => setText(state, 'The engine is not answering'));
    return s;
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
    for (const b of Array.from(this.themes.children) as HTMLElement[]) {
      const id = b.dataset.themeId ?? '';
      const on = id === c.theme;
      setClass(b, 'is-active', on);
      const edge = on ? c.accentColor : THEMES[id]?.vars['--border-color'] ?? '';
      if (b.style.borderColor !== edge) b.style.borderColor = edge;
    }
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
    const repo = repoDir(c) || 'path/to/formdesign';
    setText(this.endpoint, c.mcp ?? `${location.origin}/mcp`);
    const win = repo.includes('\\');
    const install = win ? `cd /d "${repo}" && formdesign.bat agents install` : `cd "${repo}" && ./formdesign agents install`;
    setText(this.lines.get('claude') ?? null, `claude mcp add formdesign -- "${win ? `${repo}\\formdesign.bat` : `${repo}/formdesign`}" mcp`);
    setText(this.lines.get('codex') ?? null, install);
    setText(this.lines.get('cursor') ?? null, install);
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
