import { api } from '../engine/client';
import { el, patchList, q, setClass, setHidden, setText } from '../ui/dom';
import { icon } from '../ui/icons';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

type XTerm = import('@xterm/xterm').Terminal;
type Fit = import('@xterm/addon-fit').FitAddon;
interface Preset { id: string; label: string; available: boolean; install: string | null }
interface Info { token: string; presets: Preset[]; sessions: Array<{ id: string; preset: string; exited: number | null }>; cwd: string }
interface Tab { id: string; preset: string; label: string; screen: HTMLElement; term: XTerm; fit: Fit; ws: WebSocket | null; exited: boolean }

let lib: Promise<{ Terminal: typeof import('@xterm/xterm').Terminal; FitAddon: typeof import('@xterm/addon-fit').FitAddon }> | null = null;
const load = () => (lib ??= Promise.all([import('@xterm/xterm'), import('@xterm/addon-fit'), import('@xterm/xterm/css/xterm.css')]).then(([x, f]) => ({ Terminal: x.Terminal, FitAddon: f.FitAddon })));

const cssVar = (name: string, fallback: string): string => getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
const theme = () => ({
  background: cssVar('--surface-0', '#0b0b0e'),
  foreground: cssVar('--text-primary', '#e8e8ea'),
  cursor: cssVar('--accent-color', '#f97316'),
  cursorAccent: cssVar('--surface-0', '#0b0b0e'),
  selectionBackground: cssVar('--accent-color-dim', '#f9731633'),
  scrollbarSliderBackground: 'transparent',
  scrollbarSliderHoverBackground: 'transparent',
  scrollbarSliderActiveBackground: 'transparent',
});

class TerminalPanel {
  readonly root = el(`<div class="ft-term">
    <div class="ft-bar">
      <div class="ft-tabs" role="tablist" aria-label="Terminals"></div>
      <select class="cv-select ft-preset" aria-label="Start" title="What the new terminal starts"></select>
      <button class="cv-icon-btn ft-new" type="button" title="New terminal" aria-label="New terminal">${icon('add', 16)}</button>
    </div>
    <div class="ft-screens"><p class="cv-note ft-empty">Starting a terminal in the FormDesign folder. The formdes command and its MCP server are ready in it.</p></div>
  </div>`);
  private readonly tabsHost = q(this.root, '.ft-tabs');
  private readonly screens = q(this.root, '.ft-screens');
  private readonly empty = q(this.root, '.ft-empty');
  private readonly preset = q<HTMLSelectElement>(this.root, '.ft-preset');
  private tabs: Tab[] = [];
  private active = '';
  private info: Info | null = null;
  private started = false;
  private fitQueued = false;

  constructor() {
    q(this.root, '.ft-new').addEventListener('click', () => void this.create(this.preset.value || 'shell'));
    new ResizeObserver(() => this.queueFit()).observe(this.screens);
    document.addEventListener('nm:ui-settings-changed', () => { const t = theme(); for (const tab of this.tabs) tab.term.options.theme = t; });
  }

  get isStarted(): boolean { return this.started; }

  async onShow(): Promise<void> {
    this.queueFit();
    if (this.started) return;
    this.started = true;
    try { this.info = await api<Info>('/term'); } catch (err) { setText(this.empty, `Terminals need the engine: ${(err as Error).message}`); this.started = false; return; }
    this.fillPresets();
    if (this.info.sessions.length) for (const s of this.info.sessions) await this.attach(s.id, s.preset);
    else await this.create('shell');
  }

  private fillPresets(): void {
    const list = this.info?.presets ?? [];
    if (this.preset.options.length === list.length) return;
    this.preset.replaceChildren(...list.map((p) => new Option(p.available ? p.label : `${p.label} (not installed)`, p.id)));
    this.preset.value = 'shell';
  }

  async create(preset: string, extra: { resume?: string; prompt?: string } = {}): Promise<void> {
    const p = this.info?.presets.find((x) => x.id === preset);
    if (p && !p.available) { toast(`${p.label} is not installed. Install it with: ${p.install}`, 'error'); return; }
    try {
      const r = await api<{ id: string; preset: string }>('/term', { method: 'POST', json: { preset, cols: 100, rows: 30, ...extra } });
      await this.attach(r.id, r.preset);
    } catch (err) { toast((err as Error).message, 'error'); }
  }

  private async attach(id: string, preset: string): Promise<void> {
    const { Terminal, FitAddon } = await load();
    const screen = el('<div class="ft-screen" hidden></div>');
    this.screens.appendChild(screen);
    const term = new Terminal({ theme: theme(), fontFamily: "'JetBrains Mono', ui-monospace, monospace", fontSize: 12.5, lineHeight: 1.15, cursorBlink: true, scrollback: 10000, allowProposedApi: false });
    const fit = new FitAddon();
    term.loadAddon(fit);
    term.open(screen);
    const label = this.info?.presets.find((x) => x.id === preset)?.label ?? preset;
    const tab: Tab = { id, preset, label, screen, term, fit, ws: null, exited: false };
    this.tabs.push(tab);
    this.connect(tab);
    term.onData((d) => { if (tab.ws?.readyState === WebSocket.OPEN) tab.ws.send(new TextEncoder().encode(d)); });
    term.onResize(({ cols, rows }) => { if (tab.ws?.readyState === WebSocket.OPEN) tab.ws.send(JSON.stringify({ resize: [cols, rows] })); });
    this.select(id);
  }

  private connect(tab: Tab): void {
    const proto = location.protocol === 'https:' ? 'wss' : 'ws';
    const ws = new WebSocket(`${proto}://${location.host}/api/term/${tab.id}/ws?token=${encodeURIComponent(this.info?.token ?? '')}`);
    ws.binaryType = 'arraybuffer';
    ws.onopen = () => { ws.send(JSON.stringify({ resize: [tab.term.cols, tab.term.rows] })); };
    ws.onmessage = (e) => {
      const data = new Uint8Array(e.data as ArrayBuffer);
      tab.term.write(data);
      if (!tab.exited && /\[process exited with code -?\d+\]/.test(new TextDecoder().decode(data.slice(-64)))) { tab.exited = true; this.renderTabs(); }
    };
    ws.onclose = () => { tab.ws = null; };
    tab.ws = ws;
  }

  private select(id: string): void {
    this.active = id;
    for (const t of this.tabs) setHidden(t.screen, t.id !== id);
    setHidden(this.empty, this.tabs.length > 0);
    this.renderTabs();
    this.queueFit();
    this.tabs.find((t) => t.id === id)?.term.focus();
  }

  private async close(id: string): Promise<void> {
    const i = this.tabs.findIndex((t) => t.id === id);
    if (i < 0) return;
    const [t] = this.tabs.splice(i, 1);
    t.ws?.close();
    t.term.dispose();
    t.screen.remove();
    try { await api(`/term/${id}`, { method: 'DELETE' }); } catch { }
    if (this.active === id) this.select(this.tabs[Math.max(0, i - 1)]?.id ?? '');
    else this.renderTabs();
  }

  private renderTabs(): void {
    patchList<Tab>(this.tabsHost, this.tabs, {
      key: (t) => t.id,
      create: (t) => {
        const n = el(`<div class="ft-tab" role="tab"><span class="ft-tab-name"></span><button class="ft-tab-close" type="button" aria-label="Close terminal" title="Close terminal">${icon('close', 12)}</button></div>`);
        n.addEventListener('click', (e) => {
          const tid = n.dataset.key as string;
          if ((e.target as HTMLElement).closest('.ft-tab-close')) { void this.close(tid); return; }
          if (tid !== this.active) this.select(tid);
        });
        this.fillTab(n, t);
        return n;
      },
      update: (n, t) => this.fillTab(n, t),
    });
  }

  private fillTab(n: HTMLElement, t: Tab): void {
    const n2 = this.tabs.filter((x) => x.preset === t.preset);
    setText(q(n, '.ft-tab-name'), n2.length > 1 ? `${t.label} ${n2.indexOf(t) + 1}` : t.label);
    setClass(n, 'is-active', t.id === this.active);
    setClass(n, 'is-exited', t.exited);
    if (n.getAttribute('aria-selected') !== String(t.id === this.active)) n.setAttribute('aria-selected', String(t.id === this.active));
  }

  private queueFit(): void {
    if (this.fitQueued) return;
    this.fitQueued = true;
    requestAnimationFrame(() => {
      this.fitQueued = false;
      const t = this.tabs.find((x) => x.id === this.active);
      if (t && this.screens.clientWidth > 0 && this.screens.clientHeight > 0) t.fit.fit();
    });
  }
}

export function installTerminal(ws: Workspace): void {
  let panel: TerminalPanel | null = null;
  ws.registerPanel({ id: 'terminal', label: 'Terminal', group: 'terminal', column: 'left', order: 10, create: () => (panel = new TerminalPanel()).root, onShow: () => void panel?.onShow() });
  ws.addCommands({ id: 'ui.terminal.new', label: 'New Terminal', run: () => { const was = panel?.isStarted ?? false; ws.showPanel('terminal'); if (was) void panel?.create('shell'); } });
  claudeTerminal = async (resume?: string) => {
    ws.showPanel('terminal');
    for (let i = 0; i < 40 && !panel; i += 1) await new Promise((r) => setTimeout(r, 50));
    if (!panel) return;
    if (!panel.isStarted) await panel.onShow();
    await panel.create('claude', resume ? { resume } : {});
  };
  ws.addCommands({ id: 'ui.terminal.claude', label: 'Claude Code in Terminal', keys: ['Shift+Mod+J'], run: () => void claudeTerminal?.() });
  ws.addMenuItems({ menu: 'Window', items: ['ui.terminal.new', 'ui.terminal.claude'], at: 'end' });
}

export let claudeTerminal: ((resume?: string) => Promise<void>) | null = null;
