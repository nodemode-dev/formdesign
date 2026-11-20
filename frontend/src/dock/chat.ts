import type { ChatMode, DockAdapter, DockChatEngine, DockChatEvent, DockChatPane, DockChatStatus, DockOwnChat, DockPaneHidden } from './adapter';
import { el, q, setClass, setDisabled, setHidden, setText } from './dom';
import { dockIcon as icon } from './icons';
import { lead as appLead, strings, type Say, type StringKey } from './strings';

const MODES: [ChatMode, StringKey][] = [['ask', 'modeAsk'], ['auto-edits', 'modeAuto'], ['plan', 'modePlan']];

function toolName(name: string): string {
  return name.replace(/^mcp__/, '').replace(/__/g, ' ');
}

export type ChatHost = Omit<DockAdapter<string>, 'chat'> & { chat: DockChatEngine };

export interface DockChatView {
  readonly root: HTMLElement;
  load(): Promise<void>;
  release(): void;
  isBusy(): boolean;
  send(text: string): Promise<boolean>;
  focus?(): void;
  visible?(on: boolean, why: DockPaneHidden): void;
  askTarget?(): HTMLElement | undefined;
}

export class OwnChat implements DockChatView {
  readonly root: HTMLElement;
  private readonly own: DockOwnChat;
  private on = false;

  constructor(pane: DockChatPane) {
    this.root = el('<div class="ft-chat is-own" hidden></div>');
    this.own = pane.pane(this.root) ?? {};
  }

  load(): Promise<void> { return Promise.resolve(); }
  release(): void { this.own.closed?.(); }
  isBusy(): boolean { return this.own.busy?.() === true; }
  async send(text: string): Promise<boolean> {
    if (!this.own.send || this.isBusy()) return false;
    return (await this.own.send(text)) !== false;
  }
  focus(): void { this.own.focus?.(); }
  askTarget(): HTMLElement | undefined { return this.own.askTarget; }
  visible(on: boolean, why: DockPaneHidden): void {
    if (on === this.on) return;
    this.on = on;
    if (on) this.own.shown?.(); else this.own.hidden?.(why);
  }
}

export class ChatPane implements DockChatView {
  private readonly s: Say;
  readonly root: HTMLElement;
  onFail: (err: unknown) => void = () => undefined;
  onHandoff: (session: string) => void = () => undefined;
  onBash: () => void = () => undefined;
  private readonly log: HTMLElement;
  private readonly empty: HTMLElement;
  private readonly emptyText: HTMLElement;
  private readonly install: HTMLElement;
  private readonly statusEl: HTMLElement;
  private readonly mode: HTMLSelectElement;
  private readonly stopBtn: HTMLButtonElement;
  private readonly blocks = new Map<string, { node: HTMLElement; text: Text; raw: string }>();
  private readonly tools = new Map<string, HTMLElement>();
  private readonly asks = new Map<string, HTMLElement>();
  private live: { off(): void } | null = null;
  private info: DockChatStatus | null = null;
  private state = 'idle';
  private running = false;
  private offline = false;
  private wanted = false;
  private session: string | null = null;

  constructor(private readonly a: ChatHost) {
    const s = strings(a);
    const c = a.classes;
    this.s = s;
    this.root = el(`<div class="ft-chat" hidden>
<div class="ft-chat-bar"><span class="ft-chat-status ${c.note}"></span><span class="ft-chat-meter ${c.note}" translate="no"></span><span class="ft-dock-spacer"></span>
<select class="${c.select} ft-chat-mode" aria-label="${s('modeLabel')}"></select>
<button type="button" class="${c.button} ft-chat-btn ft-chat-stop" disabled>${s('stop')}</button>
<button type="button" class="${c.button} ft-chat-btn ft-chat-handoff" title="${s('handoffTitle')}">${s('handoff')}</button>
<button type="button" class="${c.button} ft-chat-btn ft-chat-new">${s('newChat')}</button></div>
<div class="ft-chat-log" role="log" aria-live="polite"><div class="ft-chat-empty"><p class="${c.note} ft-chat-empty-text"></p><div class="ft-chat-install" hidden><code class="ft-chat-install-cmd"></code><button type="button" class="${c.button} ft-chat-copy">${s('copy')}</button></div></div></div>
</div>`);
    this.log = q(this.root, '.ft-chat-log');
    this.empty = q(this.root, '.ft-chat-empty');
    this.emptyText = q(this.root, '.ft-chat-empty-text');
    this.install = q(this.root, '.ft-chat-install');
    this.statusEl = q(this.root, '.ft-chat-status');
    this.mode = q<HTMLSelectElement>(this.root, '.ft-chat-mode');
    this.stopBtn = q<HTMLButtonElement>(this.root, '.ft-chat-stop');
    const offered = a.chat.modes ?? [];
    for (const [value, key] of MODES) {
      if (!offered.includes(value)) continue;
      const o = document.createElement('option');
      o.value = value;
      o.textContent = s(key);
      this.mode.append(o);
    }
    setHidden(this.mode, offered.length === 0);
    setHidden(this.stopBtn, !a.chat.stop);
    q(this.root, '.ft-chat-new').addEventListener('click', () => void this.begin(true));
    q(this.root, '.ft-chat-handoff').addEventListener('click', () => void this.handoff());
    q(this.root, '.ft-chat-copy').addEventListener('click', () => { void navigator.clipboard?.writeText(this.info?.install ?? '').catch(() => undefined); });
    this.stopBtn.addEventListener('click', () => { void a.chat.stop?.().catch(() => undefined); });
    this.mode.addEventListener('change', () => { if (this.running) void this.begin(false); });
  }

  private get meter(): HTMLElement { return q(this.root, '.ft-chat-meter'); }

  isBusy(): boolean { return this.state === 'busy'; }

  async load(): Promise<void> {
    this.wanted = true;
    this.connect();
    await this.refresh();
  }

  release(): void {
    this.wanted = false;
    const live = this.live;
    this.live = null;
    live?.off();
  }

  async send(text: string): Promise<boolean> {
    if (!text || this.state === 'busy') return false;
    if (!this.running && !(await this.begin(false))) return false;
    let went = true;
    try { await this.a.chat.send(text); } catch (err) {
      went = false;
      this.note('note', this.a.errors.message(err) ?? this.s('sendFailed'));
    }
    this.paint();
    return went;
  }

  private async refresh(): Promise<void> {
    this.info = await this.a.chat.status();
    this.offline = this.info === null;
    if (this.info) {
      this.running = this.info.running;
      this.state = this.info.state;
      this.session = this.info.session ?? this.session;
      if (this.info.mode && this.mode.value !== this.info.mode && this.info.running) this.mode.value = this.info.mode;
    }
    this.paint();
    this.root.classList.add('is-loaded');
  }

  private paint(): void {
    const s = this.s;
    const i = this.info;
    let status: string;
    let lead: string;
    if (this.offline || !i) {
      status = this.a.words.start ? s('offlineStart') : s('offline');
      lead = status;
    } else if (i.refused !== undefined) {
      status = i.refused || s('refused');
      lead = status;
    } else if (!i.installed) {
      status = s('missing');
      lead = s('notInstalled');
    } else {
      const parts = [`Claude Code ${i.version ?? ''}`.trim()];
      if (i.signedIn === true) parts.push(s('signedIn'));
      else if (i.signedIn === false) parts.push(s('notSignedIn'));
      parts.push(this.running ? (this.state === 'busy' ? s('working') : s('ready')) : s('idle'));
      status = parts.join(' · ');
      lead = i.lead ?? appLead(this.a);
    }
    setText(this.statusEl, status);
    this.statusEl.title = status;
    setText(this.emptyText, lead);
    const missing = !!i && !i.installed;
    setHidden(this.install, !missing);
    if (missing) setText(q(this.install, '.ft-chat-install-cmd'), i.install);
    const usable = !!i && i.installed && !this.offline;
    setDisabled(this.stopBtn, !this.running);
    setDisabled(this.mode, !usable);
    setClass(this.root, 'is-busy', this.state === 'busy');
  }

  private connect(): void {
    if (this.live) return;
    const live = { off: (): void => undefined };
    this.live = live;
    live.off = this.a.chat.subscribe({
      open: () => { if (this.live === live) this.wipe(); },
      event: (ev) => { if (this.live === live) this.onEvent(ev); },
      close: () => {
        if (this.live !== live) return;
        this.live = null;
        if (this.wanted) window.setTimeout(() => { if (this.wanted && !this.live) { this.connect(); void this.refresh(); } }, 2000);
      },
    });
  }

  private wipe(): void {
    this.log.replaceChildren(this.empty);
    setHidden(this.empty, false);
    this.blocks.clear();
    this.tools.clear();
    this.asks.clear();
  }

  private async begin(fresh: boolean): Promise<boolean> {
    try {
      await this.a.chat.start?.(this.a.chat.modes?.length ? this.mode.value as ChatMode : null, fresh);
      if (fresh) this.wipe();
      return true;
    } catch (err) {
      this.note('note', this.a.errors.message(err) ?? this.s('notStarted'));
      return false;
    }
  }

  private async handoff(): Promise<void> {
    const st = await this.a.chat.status();
    const session = st?.session ?? this.session;
    if (!session) { this.onFail(new Error(this.s('handoffFirst'))); return; }
    if ((st?.state ?? this.state) === 'busy') { this.onFail(new Error(this.s('handoffBusy'))); return; }
    this.onHandoff(session);
  }

  private add(cls: string): HTMLElement {
    const node = el(/is-note|is-error|ft-chat-ask/.test(cls) ? `<div class="${cls}"></div>` : `<div class="${cls} ft-notranslate" translate="no"></div>`);
    setHidden(this.empty, true);
    const stick = this.log.scrollHeight - this.log.scrollTop - this.log.clientHeight < 40;
    this.log.append(node);
    if (stick) this.log.scrollTop = this.log.scrollHeight;
    return node;
  }

  private note(kind: 'user' | 'note' | 'error', text: string): void {
    setText(this.add(`ft-chat-msg is-${kind}`), text);
  }

  private grow(node: HTMLElement): void {
    if (this.log.scrollHeight - this.log.scrollTop - this.log.clientHeight < 80 + node.clientHeight) this.log.scrollTop = this.log.scrollHeight;
  }

  private onEvent(ev: DockChatEvent): void {
    switch (ev.type) {
      case 'state':
        this.running = ev.running;
        this.state = ev.state;
        this.session = ev.session ?? this.session;
        if (ev.running && ev.mode && this.mode.value !== ev.mode) this.mode.value = ev.mode;
        this.paint();
        break;
      case 'user': {
        const node = this.add('ft-chat-msg is-user');
        setText(node, ev.text);
        if (ev.note) { const note = el('<span class="ft-chat-msg-note"></span>'); setText(note, ev.note); node.append(note); }
        break;
      }
      case 'reset': this.wipe(); break;
      case 'meter':
        setText(this.meter, ev.text);
        setClass(this.meter, 'is-high', ev.high === true);
        break;
      case 'block':
        if (!this.blocks.has(ev.id)) {
          const node = this.add(`ft-chat-msg is-${ev.kind === 'thinking' ? 'thinking' : 'assistant'}`);
          const text = document.createTextNode('');
          node.append(text);
          this.blocks.set(ev.id, { node, text, raw: '' });
        }
        break;
      case 'delta': {
        const block = this.blocks.get(ev.id);
        if (!block) break;
        block.raw += ev.text;
        const render = this.a.chat.renderText;
        if (render && block.node.classList.contains('is-assistant')) render(block.node, block.raw); else block.text.appendData(ev.text);
        this.grow(block.node);
        break;
      }
      case 'tool': this.tool(ev.id, ev.name, ev.summary); break;
      case 'tool_result': {
        const card = this.tools.get(ev.id);
        if (!card) break;
        const chip = q(card, '.ft-chat-tool');
        const out = q(card, '.ft-chat-tool-out');
        setText(out, ev.text);
        setHidden(out, ev.text === '' || !ev.error);
        setClass(chip, 'is-running', false);
        setClass(chip, 'is-done', !ev.error);
        setClass(chip, 'is-error', ev.error);
        break;
      }
      case 'permission': this.question(ev.id, ev.tool, ev.summary); break;
      case 'permission_done': {
        const card = this.asks.get(ev.id);
        if (!card) break;
        card.classList.add('is-done');
        setText(q(card, '.ft-chat-ask-state'), ev.allowed ? this.s('allowed') : this.s('declined'));
        break;
      }
      case 'result':
        if (!ev.ok) this.note('error', ev.text || this.s('stoppedError'));
        if (ev.meta) setText(this.add('ft-chat-msg is-meta'), ev.meta);
        break;
      case 'notice': this.note('note', ev.text); break;
    }
  }

  private tool(id: string, name: string, summary: string): void {
    if (this.tools.has(id)) return;
    const card = this.add('ft-chat-tool-card');
    const chip = el(`<div class="ft-chat-tool is-running" role="button" tabindex="0"><span class="ft-chat-tool-mark">${icon('terminal', 11)}</span><span class="ft-chat-tool-name"></span><span class="ft-chat-tool-input"></span></div>`);
    const out = el('<pre class="ft-chat-tool-out" hidden></pre>');
    setText(q(chip, '.ft-chat-tool-name'), toolName(name));
    setText(q(chip, '.ft-chat-tool-input'), summary);
    chip.title = summary;
    const toggle = (): void => { if (out.textContent) setHidden(out, !out.hidden); };
    chip.addEventListener('click', toggle);
    chip.addEventListener('keydown', (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggle(); } });
    card.append(chip, out);
    this.tools.set(id, card);
    if (name === 'Bash') this.onBash();
  }

  private question(id: string, tool: string, summary: string): void {
    if (this.asks.has(id)) return;
    const c = this.a.classes;
    const card = this.add('ft-chat-ask');
    card.append(el('<div class="ft-chat-ask-text"></div>'), el('<code class="ft-chat-ask-sum ft-notranslate" translate="no"></code>'),
      el(`<div class="ft-chat-ask-row"><button type="button" class="${c.button} ${c.primaryButton} ft-chat-allow">${this.s('consentOnce')}</button><button type="button" class="${c.button} ft-chat-deny">${this.s('deny')}</button><span class="ft-chat-ask-state"></span></div>`));
    setText(q(card, '.ft-chat-ask-text'), this.s('asksToUse', { tool: toolName(tool) }));
    setText(q(card, '.ft-chat-ask-sum'), summary);
    q(card, '.ft-chat-allow').addEventListener('click', () => { void this.a.chat.answer?.(id, true).catch(() => undefined); });
    q(card, '.ft-chat-deny').addEventListener('click', () => { void this.a.chat.answer?.(id, false).catch(() => undefined); });
    this.asks.set(id, card);
  }
}
