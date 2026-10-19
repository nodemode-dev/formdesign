import { api, engine } from '../engine/client';
import { editor } from '../editor/editor';
import { Dialog } from '../ui/dialogs';
import { el, q, setClass, setHidden, setText } from '../ui/dom';
import { icon } from '../ui/icons';
import { MarkdownStream } from '../ui/markdown';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';
import { openChat, registerChat, type ChatView } from './terminal';

interface Part { kind: 'text' | 'tool'; text?: string; id?: string; name?: string; input?: string; status?: string; images?: string[] }
interface Meta { seconds?: number; turns?: number | null; costUsd?: number | null; inputTokens?: number; outputTokens?: number }
interface Message { role: 'user' | 'assistant'; text?: string; note?: string; parts?: Part[]; error?: string; meta?: Meta }
interface Usage { input: number; output: number; costUsd: number; context: number; window: number; model: string }
interface State { busy: boolean; messages: Message[]; turn: number; session: string; usage?: Usage }
interface Status { installed: boolean; path: string | null; version: string | null; folder: string; install: string }
type Ev = { type: 'claude'; kind: string; [k: string]: unknown };

const SUGGESTIONS = ['Describe this page and what could be better', 'Check every page for overset text and fix it', 'Make a one-page flyer from the selected text'];

function tokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(n % 1_000_000 ? 1 : 0)}M`;
  if (n >= 1000) return `${(n / 1000).toFixed(n >= 100_000 ? 0 : 1)}k`;
  return String(n);
}

class AdoptDialog extends Dialog {
  private readonly pick = el<HTMLSelectElement>('<select class="cv-select fc-adopt-pick" aria-label="Conversation"></select>');
  private readonly titles = new Map<string, string>();
  private choose: ((session: string, title: string) => void) | null = null;

  constructor() {
    super('Continue from Claude Code', 460);
    const lead = el('<p class="cv-note fc-adopt-lead"></p>');
    setText(lead, 'Brings a conversation from a Claude Code tab into this chat: its messages show here and your next message continues it.');
    this.body.appendChild(lead);
    this.field('Conversation', this.pick);
    this.button('Cancel', 'ghost', () => this.close());
    this.button('Continue Here', 'primary', () => {
      const id = this.pick.value;
      if (!id) return;
      this.close();
      this.choose?.(id, this.titles.get(id) ?? '');
    });
  }

  ask(sessions: Array<{ id: string; title: string; modified: number; current: boolean }>, choose: (session: string, title: string) => void): void {
    this.choose = choose;
    this.titles.clear();
    const when = (t: number): string => new Date(t * 1000).toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit' });
    this.pick.replaceChildren(...sessions.map((s) => {
      this.titles.set(s.id, s.title);
      return new Option(`${when(s.modified)} · ${s.title || 'untitled'}${s.current ? ' (this chat)' : ''}`, s.id);
    }));
    this.pick.value = sessions[0]?.id ?? '';
    this.open();
  }
}

class ClaudePanel implements ChatView {
  readonly root = el(`<div class="fc-chat">
    <div class="fc-bar"><span class="fc-status"></span><span class="fc-usage" title="Memory: how much of Claude’s working memory this conversation uses. Reply: tokens written so far. Chat: everything sent and written in this conversation, and what it has cost."></span>
      <span class="fc-btns">
        <button class="cv-btn fc-stop" type="button" hidden>Stop</button>
        <button class="cv-btn fc-adopt" type="button" title="Bring a conversation you had in a Claude Code tab into this chat">From Claude Code…</button>
        <button class="cv-btn fc-handoff" type="button" title="Continue this conversation in a Claude Code tab">Open in Claude Code</button>
        <button class="cv-btn fc-new" type="button" title="Start a new conversation">New Chat</button>
      </span></div>
    <div class="fc-setup" hidden><p class="fc-setup-text"></p><button class="cv-btn fc-recheck" type="button">Check again</button></div>
    <div class="fc-log" role="log" aria-live="polite"><div class="fc-hint"><p>Ask Claude about the open document in the line below. It uses your own Claude Code sign-in and FormDesign&rsquo;s tools; every change appears in the window as it happens and can be undone.</p><div class="fc-chips"></div></div></div>
  </div>`);
  onHandoff: (session: string) => void = () => undefined;
  onSuggest: (text: string) => void = () => undefined;
  private readonly log = q(this.root, '.fc-log');
  private readonly hint = q(this.root, '.fc-hint');
  private readonly meter = q(this.root, '.fc-usage');
  private readonly chips = new Map<string, HTMLElement>();
  private reply: HTMLElement | null = null;
  private md: MarkdownStream | null = null;
  private usage: Usage = { input: 0, output: 0, costUsd: 0, context: 0, window: 0, model: '' };
  private busy = false;
  private loaded = false;
  private ready = false;
  private dialog: AdoptDialog | null = null;

  constructor() {
    for (const s of SUGGESTIONS) {
      const b = el<HTMLButtonElement>('<button class="cv-btn fc-chip" type="button"></button>');
      setText(b, s);
      b.addEventListener('click', () => this.onSuggest(s));
      q(this.hint, '.fc-chips').appendChild(b);
    }
    q(this.root, '.fc-stop').addEventListener('click', () => void this.stop());
    q(this.root, '.fc-new').addEventListener('click', () => void api('/claude/chat/reset', { json: {} }).catch((e) => toast((e as Error).message, 'error')));
    q(this.root, '.fc-handoff').addEventListener('click', () => void this.handoff());
    q(this.root, '.fc-adopt').addEventListener('click', () => void this.adopt());
    q(this.root, '.fc-recheck').addEventListener('click', () => void this.status());
    engine.on('claude', (m) => this.event(m as Ev));
  }

  isBusy(): boolean { return this.busy; }

  onShow(): void {
    void this.status();
    void this.load();
  }

  private async status(): Promise<void> {
    let s: Status;
    try { s = await api<Status>('/claude/status'); } catch (e) { setText(q(this.root, '.fc-status'), (e as Error).message); return; }
    this.ready = s.installed;
    setHidden(q(this.root, '.fc-setup'), s.installed);
    setText(q(this.root, '.fc-setup-text'), s.install);
    if (!this.busy) setText(q(this.root, '.fc-status'), s.installed ? `Claude Code ${s.version ?? ''}`.trim() : 'Claude Code not found');
  }

  private async load(): Promise<void> {
    if (this.loaded) return;
    this.loaded = true;
    try {
      const s = await api<State>('/claude/chat');
      this.replay(s);
    } catch { this.loaded = false; }
  }

  private replay(s: State): void {
    for (const m of s.messages) {
      if (m.role === 'user') { this.user(m.text ?? '', m.note ?? ''); continue; }
      this.begin();
      for (const p of m.parts ?? []) {
        if (p.kind === 'text') this.delta(p.text ?? '');
        else this.tool(p);
      }
      this.close(m.error ?? '', m.meta ?? {});
    }
    if (s.usage) this.showUsage(s.usage, -1);
    if (s.busy) { this.begin(); this.setBusy(true); }
  }

  async send(text: string, context: string): Promise<void> {
    if (!text || this.busy) return;
    if (!this.ready) { await this.status(); if (!this.ready) return; }
    const sel = editor.selection?.items ?? [];
    try {
      await api('/claude/chat', { json: { text, context, note: sel.length ? `${sel.length} selected` : '' } });
    } catch (e) { toast((e as Error).message, 'error'); }
  }

  private async stop(): Promise<void> {
    try { await api('/claude/chat/stop', { json: {} }); } catch (e) { toast((e as Error).message, 'error'); }
  }

  private event(e: Ev): void {
    switch (e.kind) {
      case 'start': this.user(String(e.text ?? ''), String(e.note ?? '')); this.begin(); break;
      case 'delta': this.delta(String(e.text ?? '')); break;
      case 'tool': this.tool({ kind: 'tool', id: String(e.id ?? ''), name: String(e.name ?? ''), input: String(e.input ?? ''), status: 'running' }); break;
      case 'toolDone': {
        const c = this.chips.get(String(e.id ?? ''));
        if (!c) break;
        this.mark(c, String(e.status ?? 'done'));
        if (Array.isArray(e.images) && e.images.length) this.images(c, e.images as string[]);
        break;
      }
      case 'usage': this.showUsage({ ...this.usage, context: Number(e.context ?? 0), window: Number(e.window ?? 0) || this.usage.window, model: String(e.model ?? this.usage.model) }, Number(e.output ?? 0)); break;
      case 'done': this.close(String(e.error ?? ''), (e.meta ?? {}) as Meta); if (e.usage) this.showUsage(e.usage as Usage, -1); break;
      case 'reset': this.clear(); this.showUsage({ input: 0, output: 0, costUsd: 0, context: 0, window: 0, model: '' }, -1); break;
      case 'adopted': this.clear(); this.loaded = false; void this.load(); break;
      default: break;
    }
  }

  private clear(): void {
    this.log.replaceChildren(this.hint);
    setHidden(this.hint, false);
    this.reply = null;
    this.md = null;
    this.chips.clear();
    this.setBusy(false);
  }

  private setBusy(on: boolean): void {
    this.busy = on;
    setHidden(q(this.root, '.fc-stop'), !on);
    setClass(this.root, 'is-busy', on);
    if (on) setText(q(this.root, '.fc-status'), 'Claude is working…');
    else void this.status();
  }

  private scroll(): void {
    const l = this.log;
    if (l.scrollHeight - l.scrollTop - l.clientHeight < 80) l.scrollTop = l.scrollHeight;
  }

  private user(text: string, note = ''): void {
    setHidden(this.hint, true);
    const n = el('<div class="fc-msg is-user"><div class="fc-said"></div></div>');
    setText(q(n, '.fc-said'), text);
    if (note) { const m = el('<div class="fc-note"></div>'); setText(m, note); n.appendChild(m); }
    this.log.appendChild(n);
    this.scroll();
  }

  private begin(): void {
    setHidden(this.hint, true);
    this.endText();
    this.reply = el('<div class="fc-msg is-claude"></div>');
    this.log.appendChild(this.reply);
    this.setBusy(true);
    this.scroll();
  }

  private delta(text: string): void {
    if (!this.reply) this.begin();
    if (!this.md) { this.md = new MarkdownStream('fc-text fd-md'); this.reply?.appendChild(this.md.root); }
    this.md.push(text);
    this.scroll();
  }

  private endText(): void {
    this.md?.end();
    this.md = null;
  }

  private tool(p: Part): void {
    if (!this.reply) this.begin();
    this.endText();
    const chip = el(`<div class="fc-tool"><span class="fc-tool-mark">${icon('terminal', 11)}</span><span class="fc-tool-name"></span><span class="fc-tool-input"></span></div>`);
    setText(q(chip, '.fc-tool-name'), p.name ?? '');
    setText(q(chip, '.fc-tool-input'), p.input ?? '');
    chip.title = p.input ?? '';
    this.mark(chip, p.status ?? 'running');
    if (p.id) this.chips.set(p.id, chip);
    this.reply?.appendChild(chip);
    if (p.images?.length) this.images(chip, p.images);
    this.scroll();
  }

  private images(after: HTMLElement, names: string[]): void {
    const row = el('<div class="fc-images"></div>');
    for (const name of names.slice(0, 6)) {
      const img = el<HTMLImageElement>('<img alt="What Claude looked at" decoding="async" draggable="false" width="160" height="120">');
      img.src = `/api/claude/image/${encodeURIComponent(name)}`;
      row.appendChild(img);
    }
    after.after(row);
    this.scroll();
  }

  private mark(chip: HTMLElement, s: string): void {
    for (const k of ['running', 'done', 'error']) setClass(chip, `is-${k}`, s === k);
  }

  private close(error: string, meta: Meta): void {
    if (!this.reply) this.begin();
    this.endText();
    for (const c of Array.from(this.reply?.querySelectorAll<HTMLElement>('.fc-tool.is-running') ?? [])) this.mark(c, 'done');
    if (error) { const n = el('<div class="fc-error"></div>'); setText(n, error); this.reply?.appendChild(n); }
    if (meta.seconds !== undefined) {
      const n = el('<div class="fc-meta"></div>');
      const used = meta.inputTokens || meta.outputTokens ? ` · ${tokens((meta.inputTokens ?? 0) + (meta.outputTokens ?? 0))} tokens` : '';
      setText(n, `${meta.seconds} s${meta.turns ? ` · ${meta.turns} steps` : ''}${used}${meta.costUsd ? ` · $${meta.costUsd.toFixed(3)}` : ''}`);
      this.reply?.appendChild(n);
    }
    this.reply = null;
    this.setBusy(false);
    this.scroll();
  }

  private showUsage(u: Usage, reply: number): void {
    this.usage = u;
    if (!u.context && !u.input && reply < 0) { setText(this.meter, ''); return; }
    const parts: string[] = [];
    if (u.context) parts.push(`Memory ${tokens(u.context)}${u.window ? ` of ${tokens(u.window)}` : ''}`);
    if (reply >= 0) parts.push(`Reply ${tokens(reply)}`);
    if (u.input || u.output) parts.push(`Chat ${tokens(u.input + u.output)}${u.costUsd ? ` · $${u.costUsd.toFixed(u.costUsd < 1 ? 3 : 2)}` : ''}`);
    setText(this.meter, parts.join(' · '));
    setClass(this.meter, 'is-high', !!u.window && u.context / u.window > 0.8);
  }

  private async handoff(): Promise<void> {
    try {
      const s = await api<State>('/claude/chat');
      if (!s.session) { toast('Start a conversation here first', 'error'); return; }
      if (s.busy) { toast('Wait for Claude to finish, or press Stop, before moving the conversation', 'error'); return; }
      this.onHandoff(s.session);
    } catch (e) { toast((e as Error).message, 'error'); }
  }

  private async adopt(): Promise<void> {
    try {
      const r = await api<{ sessions: Array<{ id: string; title: string; modified: number; current: boolean }> }>('/claude/sessions');
      const list = [...r.sessions].sort((a, b) => b.modified - a.modified).slice(0, 20);
      if (!list.length) { toast('No Claude Code conversation was found in FormDesign’s agent folder yet', 'info'); return; }
      this.dialog ??= new AdoptDialog();
      this.dialog.ask(list, (session, title) => {
        api('/claude/adopt', { json: { session } })
          .then(() => toast(`Continuing “${title || 'untitled'}”`, 'info'))
          .catch((e) => toast((e as Error).message, 'error'));
      });
    } catch (e) { toast((e as Error).message, 'error'); }
  }
}

export function installClaude(ws: Workspace): void {
  registerChat(() => new ClaudePanel());
  ws.addCommands({ id: 'ui.claude', label: 'Chat with Claude', keys: ['Alt+Mod+J'], run: () => openChat() });
}
