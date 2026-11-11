import { api } from '../engine/client';
import { editor } from '../editor/editor';
import { el, isMac, q, setClass, setDisabled, setHidden, setStyleVar, setText } from '../ui/dom';
import { icon } from '../ui/icons';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

type XTerm = import('@xterm/xterm').Terminal;
type Fit = import('@xterm/addon-fit').FitAddon;
interface Preset { id: string; label: string; available: boolean; install: string | null }
interface Session { id: string; preset: string; title?: string; alive?: boolean }
interface Info { token: string; presets: Preset[]; sessions: Session[]; cwd: string }
interface Started { id: string; preset: string; title: string; reused?: boolean }
interface Tab { id: string; preset: string; title: string; host: HTMLElement; node: HTMLElement; term: XTerm; fit: Fit; ws: WebSocket | null; exited: boolean }

export interface ChatView {
  readonly root: HTMLElement;
  onShow(): void;
  send(text: string, context: string): Promise<void>;
  isBusy(): boolean;
  onHandoff: (session: string) => void;
  onSuggest: (text: string) => void;
}

const HEIGHT_KEY = 'formdesign.dock.height';
const H_MIN = 140;
const CHAT = 'chat';
const ASK_CLAUDE = 'Ask Claude Code to work on your document';
const ASK_CHAT = 'Ask Claude about your document';

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

const isToggle = (e: KeyboardEvent): boolean => (isMac ? e.metaKey : e.ctrlKey) && !e.altKey && !e.shiftKey && e.code === 'KeyJ';
const setPlaceholder = (input: HTMLInputElement, text: string): void => { if (input.placeholder !== text) input.placeholder = text; };
const setLabel = (node: HTMLElement, text: string): void => {
  if (node.title !== text) node.title = text;
  if (node.getAttribute('aria-label') !== text) node.setAttribute('aria-label', text);
};

export function documentContext(ws: Workspace): string {
  const parts: string[] = [];
  const doc = editor.active === null ? null : editor.docs[editor.active];
  if (doc) parts.push(`open document: ${(doc as { name?: string }).name ?? 'untitled'}`);
  parts.push(`page ${ws.viewport.currentPage() + 1} is in view`);
  const sel = editor.selection?.items ?? [];
  if (sel.length) parts.push(`selected item ids: ${sel.slice(0, 40).join(', ')}`);
  return parts.join('; ');
}

class TerminalDock {
  private readonly tabsEl: HTMLElement;
  private readonly stage: HTMLElement;
  private readonly empty: HTMLElement;
  private readonly ask: HTMLInputElement;
  private readonly askSend: HTMLElement;
  private readonly restartBtn: HTMLButtonElement;
  private readonly tabs: Tab[] = [];
  private readonly pending = new Set<string>();
  private info: Info | null = null;
  private active = '';
  private chat: ChatView | null = null;
  private chatTab: HTMLElement | null = null;
  private fitQueued = false;
  private starting = false;
  private makeChat: (() => ChatView) | null = null;

  constructor(private readonly ws: Workspace, private readonly root: HTMLElement) {
    this.tabsEl = q(root, '.fd-dock-tabs');
    this.stage = q(root, '.fd-dock-stage');
    this.empty = q(root, '.fd-dock-empty');
    this.ask = q<HTMLInputElement>(root, '.fd-dock-ask-input');
    this.askSend = q(root, '.fd-dock-ask-send');
    this.restartBtn = q<HTMLButtonElement>(root, '.fd-dock-restart');
    q(root, '.fd-dock-new').addEventListener('click', () => void this.open());
    q(root, '.fd-dock-chat').addEventListener('click', () => void this.showChat());
    q(root, '.fd-dock-claude').addEventListener('click', () => void this.claude());
    this.restartBtn.addEventListener('click', () => void this.restart());
    q(root, '.fd-dock-hide').addEventListener('click', () => this.hide());
    q(root, '.fd-dock-ask').addEventListener('submit', (e) => { e.preventDefault(); void this.send(); });
    root.addEventListener('keydown', (e) => {
      e.stopPropagation();
      if (isToggle(e)) { e.preventDefault(); this.hide(); }
    });
    new ResizeObserver(() => this.queueFit()).observe(this.stage);
    document.addEventListener('nm:ui-settings-changed', () => { const t = theme(); for (const tab of this.tabs) tab.term.options.theme = t; });
    this.sizer();
    this.syncAsk();
  }

  register(make: () => ChatView): void { this.makeChat = make; }

  isOpen(): boolean { return !this.root.hidden; }

  hide(): void {
    if (!this.isOpen()) return;
    if (this.root.contains(document.activeElement)) (document.activeElement as HTMLElement).blur();
    this.setOpen(false);
  }

  async show(shell = true): Promise<boolean> {
    if (this.isOpen()) return false;
    this.setOpen(true);
    return this.ensure(shell);
  }

  private setOpen(open: boolean): void {
    if (this.isOpen() === open) return;
    setHidden(this.root, !open);
    setClass(this.ws.root, 'is-dock-open', open);
    window.dispatchEvent(new CustomEvent('fd-dock', { detail: { open } }));
  }

  private sizer(): void {
    try {
      const h = Number(localStorage.getItem(HEIGHT_KEY));
      if (h) setStyleVar(this.root, '--fd-dock-h', `${Math.round(Math.max(H_MIN, h))}px`);
    } catch { }
    const handle = q(this.root, '.fd-dock-rsz');
    handle.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      const start = e.clientY;
      const from = this.root.getBoundingClientRect().height;
      handle.setPointerCapture(e.pointerId);
      setClass(handle, 'is-dragging', true);
      const move = (ev: PointerEvent): void => {
        const h = Math.round(Math.max(H_MIN, Math.min(window.innerHeight * 0.7, from - (ev.clientY - start))));
        setStyleVar(this.root, '--fd-dock-h', `${h}px`);
      };
      const up = (): void => {
        handle.removeEventListener('pointermove', move);
        handle.removeEventListener('pointerup', up);
        handle.removeEventListener('pointercancel', up);
        setClass(handle, 'is-dragging', false);
        try { localStorage.setItem(HEIGHT_KEY, String(Math.round(this.root.getBoundingClientRect().height))); } catch { }
      };
      handle.addEventListener('pointermove', move);
      handle.addEventListener('pointerup', up);
      handle.addEventListener('pointercancel', up);
    });
  }

  private async refresh(): Promise<boolean> {
    try { this.info = await api<Info>('/term'); return true; } catch (err) {
      toast(`Terminals need the engine: ${(err as Error).message}`, 'error');
      return false;
    }
  }

  async ensure(shell: boolean): Promise<boolean> {
    let started = false;
    this.starting = true;
    try {
      if (await this.refresh() && this.info) {
        for (const s of this.info.sessions) if (!this.has(s.id)) await this.attach(s, false);
      }
      if (shell && !this.tabs.length && !this.chatTab) { await this.open(); started = true; }
      else if (!this.active) {
        const first = this.tabs[0]?.id ?? (this.chatTab ? CHAT : '');
        if (first) this.select(first);
      } else this.queueFit();
    } finally {
      this.starting = false;
      this.syncEmpty();
    }
    this.focus();
    return started;
  }

  private has(id: string): boolean { return this.pending.has(id) || this.tabs.some((t) => t.id === id); }

  focus(): void {
    if (this.active === CHAT) this.ask.focus();
    else this.tabs.find((t) => t.id === this.active)?.term.focus();
  }

  async open(): Promise<void> {
    await this.start({ preset: 'shell' });
  }

  async claude(prompt?: string, resume?: string): Promise<void> {
    if (!this.info && !(await this.refresh())) return;
    const p = this.info?.presets.find((x) => x.id === 'claude');
    if (p && !p.available) { toast(`Claude Code is not installed. Install it with: ${p.install ?? 'npm install -g @anthropic-ai/claude-code'}`, 'error'); return; }
    await this.start({ preset: 'claude', prompt: prompt || undefined, resume: resume || undefined, reuse: !resume });
  }

  private async start(body: Record<string, unknown>): Promise<void> {
    if (!this.info && !(await this.refresh())) return;
    const size = this.size();
    try {
      const r = await api<Started>('/term', { json: { cols: size.cols, rows: size.rows, ...body } });
      const have = this.tabs.find((t) => t.id === r.id);
      if (have) {
        this.retitle(have, r.title);
        this.select(r.id);
      } else await this.attach({ id: r.id, preset: r.preset, title: r.title }, true);
    } catch (err) { toast((err as Error).message, 'error'); }
  }

  private size(): { cols: number; rows: number } {
    const t = this.tabs.find((x) => x.id === this.active);
    return t ? { cols: t.term.cols, rows: t.term.rows } : { cols: 100, rows: 20 };
  }

  private async attach(s: Session, show: boolean): Promise<void> {
    if (this.has(s.id)) return;
    this.pending.add(s.id);
    try {
      const { Terminal, FitAddon } = await load();
      const host = el('<div class="fd-term" hidden></div>');
      host.dataset.id = s.id;
      const term = new Terminal({ theme: theme(), fontFamily: cssVar('--font-mono', 'ui-monospace, monospace'), fontSize: 12, lineHeight: 1.2, cursorBlink: true, scrollback: 5000, allowProposedApi: false });
      term.attachCustomKeyEventHandler((e) => !isToggle(e));
      const fit = new FitAddon();
      term.loadAddon(fit);
      const claude = s.preset === 'claude';
      const node = el(`<button type="button" class="fd-dock-tab${claude ? ' is-claude' : ''}" role="tab" aria-selected="false">${icon(claude ? 'agent' : 'terminal', 13)}<span class="fd-dock-tab-title"></span><span class="fd-dock-tab-x" role="button" aria-label="Close this tab" title="Close this tab">${icon('close', 11)}</span></button>`);
      node.addEventListener('click', (e) => {
        if ((e.target as HTMLElement).closest('.fd-dock-tab-x')) void this.close(s.id);
        else this.select(s.id);
      });
      this.stage.appendChild(host);
      this.tabsEl.appendChild(node);
      const tab: Tab = { id: s.id, preset: s.preset, title: s.title || (claude ? 'Claude Code' : 'Terminal'), host, node, term, fit, ws: null, exited: s.alive === false };
      this.tabs.push(tab);
      term.open(host);
      term.onData((d) => { if (tab.ws?.readyState === WebSocket.OPEN) tab.ws.send(new TextEncoder().encode(d)); });
      term.onResize(({ cols, rows }) => { if (tab.ws?.readyState === WebSocket.OPEN) tab.ws.send(JSON.stringify({ resize: [cols, rows] })); });
      this.connect(tab);
      this.paintTabs();
      this.syncEmpty();
      if (show || !this.active) this.select(s.id);
    } finally { this.pending.delete(s.id); }
  }

  private connect(tab: Tab): void {
    const proto = location.protocol === 'https:' ? 'wss' : 'ws';
    const ws = new WebSocket(`${proto}://${location.host}/api/term/${encodeURIComponent(tab.id)}/ws?token=${encodeURIComponent(this.info?.token ?? '')}`);
    ws.binaryType = 'arraybuffer';
    ws.onopen = () => { ws.send(JSON.stringify({ resize: [tab.term.cols, tab.term.rows] })); };
    ws.onmessage = (e) => {
      const data = new Uint8Array(e.data as ArrayBuffer);
      tab.term.write(data);
      if (!tab.exited && /Restart runs it again\]/.test(new TextDecoder().decode(data.slice(-120)))) { tab.exited = true; this.paintTabs(); }
    };
    ws.onclose = () => { if (tab.ws === ws) tab.ws = null; };
    tab.ws = ws;
  }

  private retitle(tab: Tab, title: string): void {
    if (tab.title === title) return;
    tab.title = title;
    this.paintTabs();
  }

  private paintTabs(): void {
    for (const t of this.tabs) {
      const same = this.tabs.filter((x) => x.title === t.title);
      setText(q(t.node, '.fd-dock-tab-title'), same.length > 1 ? `${t.title} ${same.indexOf(t) + 1}` : t.title);
      setClass(t.node, 'is-exited', t.exited);
    }
  }

  private syncEmpty(): void {
    setHidden(this.empty, this.starting || this.tabs.length > 0 || !!this.chatTab);
  }

  private syncAsk(): void {
    const chat = this.active === CHAT;
    setPlaceholder(this.ask, chat ? ASK_CHAT : ASK_CLAUDE);
    setLabel(this.askSend, chat ? 'Send to Claude' : 'Send to Claude Code');
    if (this.ask.getAttribute('aria-label') !== (chat ? 'Ask Claude' : 'Ask Claude Code')) this.ask.setAttribute('aria-label', chat ? 'Ask Claude' : 'Ask Claude Code');
  }

  private select(id: string): void {
    this.active = id;
    for (const t of this.tabs) {
      const on = t.id === id;
      setHidden(t.host, !on);
      setClass(t.node, 'is-on', on);
      const sel = String(on);
      if (t.node.getAttribute('aria-selected') !== sel) t.node.setAttribute('aria-selected', sel);
    }
    if (this.chat) setHidden(this.chat.root, id !== CHAT);
    if (this.chatTab) {
      const on = id === CHAT;
      setClass(this.chatTab, 'is-on', on);
      const sel = String(on);
      if (this.chatTab.getAttribute('aria-selected') !== sel) this.chatTab.setAttribute('aria-selected', sel);
    }
    this.syncAsk();
    setDisabled(this.restartBtn, id === CHAT || !id);
    this.queueFit();
    if (this.isOpen()) this.focus();
  }

  async showChat(): Promise<void> {
    if (!this.chat) {
      if (!this.makeChat) return;
      const chat = this.makeChat();
      chat.onHandoff = (session) => void this.claude(undefined, session);
      chat.onSuggest = (text) => { this.ask.value = text; this.ask.focus(); };
      setHidden(chat.root, true);
      this.stage.appendChild(chat.root);
      this.chat = chat;
    }
    if (!this.chatTab) {
      const tab = el(`<button type="button" class="fd-dock-tab is-chat" role="tab" aria-selected="false">${icon('agentChat', 13)}<span class="fd-dock-tab-title">Chat</span><span class="fd-dock-tab-x" role="button" aria-label="Close the Chat tab" title="Close the Chat tab">${icon('close', 11)}</span></button>`);
      tab.addEventListener('click', (e) => {
        if ((e.target as HTMLElement).closest('.fd-dock-tab-x')) this.closeChat();
        else this.select(CHAT);
      });
      this.tabsEl.prepend(tab);
      this.chatTab = tab;
    }
    this.syncEmpty();
    this.select(CHAT);
    this.chat.onShow();
  }

  private closeChat(): void {
    this.chatTab?.remove();
    this.chatTab = null;
    if (this.chat) setHidden(this.chat.root, true);
    if (this.active === CHAT) {
      this.active = '';
      const next = this.tabs[this.tabs.length - 1]?.id;
      if (next) this.select(next);
      else this.select('');
    }
    this.syncEmpty();
  }

  private async send(): Promise<void> {
    const text = this.ask.value.trim();
    if (!text) return;
    const extra = documentContext(this.ws);
    if (this.active === CHAT && this.chat) {
      if (this.chat.isBusy()) return;
      this.ask.value = '';
      await this.chat.send(text, extra);
      return;
    }
    this.ask.value = '';
    await this.claude(extra ? `${text} (${extra})` : text);
  }

  private async restart(): Promise<void> {
    const t = this.tabs.find((x) => x.id === this.active);
    if (!t) return;
    try {
      const r = await api<Started>(`/term/${encodeURIComponent(t.id)}/restart`, { json: {} });
      this.retitle(t, r.title);
      t.exited = false;
      this.paintTabs();
      if (!t.ws || t.ws.readyState > WebSocket.OPEN) { t.term.reset(); this.connect(t); }
      else if (t.ws.readyState === WebSocket.OPEN) t.ws.send(JSON.stringify({ resize: [t.term.cols, t.term.rows] }));
      t.term.focus();
    } catch (err) { toast((err as Error).message, 'error'); }
  }

  private async close(id: string): Promise<void> {
    const i = this.tabs.findIndex((t) => t.id === id);
    if (i < 0) return;
    const [t] = this.tabs.splice(i, 1);
    t.ws?.close();
    t.term.dispose();
    t.host.remove();
    t.node.remove();
    this.paintTabs();
    try { await api(`/term/${encodeURIComponent(id)}`, { method: 'DELETE' }); } catch { }
    if (this.active === id) {
      this.active = '';
      const next = this.tabs[Math.max(0, i - 1)]?.id ?? (this.chatTab ? CHAT : '');
      this.select(next);
    }
    this.syncEmpty();
  }

  private queueFit(): void {
    if (this.fitQueued) return;
    this.fitQueued = true;
    requestAnimationFrame(() => {
      this.fitQueued = false;
      const t = this.tabs.find((x) => x.id === this.active);
      if (!t || t.host.hidden || this.stage.clientWidth < 20 || this.stage.clientHeight < 20) return;
      const before = `${t.term.cols}x${t.term.rows}`;
      t.fit.fit();
      if (`${t.term.cols}x${t.term.rows}` !== before && t.ws?.readyState === WebSocket.OPEN) t.ws.send(JSON.stringify({ resize: [t.term.cols, t.term.rows] }));
    });
  }
}

let dock: TerminalDock | null = null;

export function registerChat(make: () => ChatView): void { dock?.register(make); }

export async function openChat(): Promise<void> {
  if (!dock) return;
  await dock.show(false);
  await dock.showChat();
}

export function installTerminal(ws: Workspace): void {
  const host = ws.root.querySelector<HTMLElement>('.fd-dock');
  if (!host) return;
  const d = new TerminalDock(ws, host);
  dock = d;
  ws.addCommands(
    { id: 'ui.terminal.toggle', label: 'Show Terminal', keys: ['Mod+J'], checked: () => d.isOpen(), run: () => { if (d.isOpen()) d.hide(); else void d.show(); } },
    { id: 'ui.terminal.new', label: 'New Terminal', run: async () => { const made = await d.show(); if (!made) await d.open(); } },
    { id: 'ui.terminal.claude', label: 'Claude Code', keys: ['Shift+Mod+J'], run: async () => { await d.show(false); await d.claude(); } },
  );
  ws.addMenuItems({ menu: 'Window', items: ['ui.terminal.toggle', 'ui.terminal.new', 'ui.terminal.claude', 'ui.claude'], at: 'end' });
  ws.addMenuItems({ menu: 'Help', items: ['ui.claude', 'ui.terminal.claude'], at: 'end' });
}
