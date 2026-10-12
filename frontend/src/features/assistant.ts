import { api, EngineError } from '../engine/client';
import { editor } from '../editor/editor';
import { el, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';
import { AI_CHANGED, loadAi, type AiSettings } from './ai-settings';

type Ev =
  | { type: 'text'; text: string }
  | { type: 'tool'; id: string; name: string; args: string }
  | { type: 'tool_result'; id: string; ok: boolean; note: string }
  | { type: 'approval'; id: string; tool: string; reason: string; args: string }
  | { type: 'done' } | { type: 'stopped' } | { type: 'error'; error: string };
interface Hist { kind: 'user' | 'assistant' | 'tool'; text?: string; id?: string; name?: string; args?: string; ok?: boolean; note?: string }
interface Lane { root: HTMLElement; loaded: boolean; tools: Map<string, HTMLElement> }

const SUGGESTIONS = ['Describe this page', 'Check the page for problems and fix them', 'Make the headline bigger'];

class AssistantPanel {
  readonly root = el(`<div class="fa-assist">
    <div class="fa-head"><span class="fa-status"></span><span class="fa-head-btns"><button class="cv-btn fa-new" type="button" title="Start a new conversation for this document">New chat</button><button class="cv-btn fa-set" type="button" title="Open AI preferences">Settings</button></span></div>
    <div class="fa-setup" hidden><p class="fa-setup-text">The assistant is off. Choose a provider, a model and a key in AI preferences, then turn it on.</p><button class="cv-btn cv-btn-primary fa-setup-btn" type="button">Open AI preferences</button></div>
    <div class="fa-lanes"></div>
    <div class="fa-compose"><textarea class="cv-input fa-input" rows="2" placeholder="Ask for a change to this document" aria-label="Message the assistant"></textarea><button class="cv-btn cv-btn-primary fa-send" type="button">Send</button></div>
  </div>`);
  private readonly lanes = q(this.root, '.fa-lanes');
  private readonly input = q<HTMLTextAreaElement>(this.root, '.fa-input');
  private readonly send = q<HTMLButtonElement>(this.root, '.fa-send');
  private readonly byDoc = new Map<number, Lane>();
  private cfg: AiSettings | null = null;
  private busy = false;
  private runDoc: number | null = null;

  constructor(private readonly ws: Workspace) {
    this.send.addEventListener('click', () => (this.busy ? void this.stop() : void this.submit()));
    this.input.addEventListener('keydown', (e) => {
      e.stopPropagation();
      if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) { e.preventDefault(); if (!this.busy) void this.submit(); }
    });
    q(this.root, '.fa-set').addEventListener('click', () => ws.commands.run('ui.prefsAi'));
    q(this.root, '.fa-setup-btn').addEventListener('click', () => ws.commands.run('ui.prefsAi'));
    q(this.root, '.fa-new').addEventListener('click', () => void this.reset());
    editor.on('docs', () => this.showLane());
    document.addEventListener(AI_CHANGED, () => void this.status());
  }

  onShow(): void { void this.status(); this.showLane(); }

  private uid(): number | null { return editor.active === null ? null : (editor.docs[editor.active]?.uid ?? null); }

  private lane(uid: number): Lane {
    let l = this.byDoc.get(uid);
    if (!l) {
      l = { root: el('<div class="fa-list" hidden></div>'), loaded: false, tools: new Map() };
      this.lanes.appendChild(l.root);
      this.byDoc.set(uid, l);
    }
    return l;
  }

  private showLane(): void {
    const uid = this.uid();
    for (const [id, l] of this.byDoc) setHidden(l.root, id !== uid);
    if (uid === null) return;
    const l = this.lane(uid);
    setHidden(l.root, false);
    if (!l.loaded) { l.loaded = true; void this.history(uid, l); }
    if (!l.root.childElementCount) this.hint(l);
  }

  private hint(l: Lane): void {
    if (l.root.querySelector('.fa-hint')) return;
    const h = el('<div class="fa-hint"><p>Ask for a change and the assistant will read the page, use FormDesign&rsquo;s tools and say what it did. Steps that cannot be undone wait for your approval.</p><div class="fa-chips"></div></div>');
    for (const s of SUGGESTIONS) {
      const b = el<HTMLButtonElement>('<button class="cv-btn fa-chip" type="button"></button>');
      setText(b, s);
      b.addEventListener('click', () => { this.input.value = s; this.input.focus(); });
      q(h, '.fa-chips').appendChild(b);
    }
    l.root.appendChild(h);
  }

  private async status(): Promise<void> {
    try { this.cfg = await loadAi(); } catch { return; }
    const c = this.cfg;
    setText(q(this.root, '.fa-status'), c.configured ? `${c.provider === 'openai' ? 'OpenAI compatible' : c.provider === 'ollama' ? 'Ollama' : 'Anthropic'} · ${c.model}` : c.enabled ? 'Not set up' : 'Off');
    setHidden(q(this.root, '.fa-setup'), c.configured);
    setText(q(this.root, '.fa-setup-text'), c.enabled ? 'Add a model name and a key (or a local address) in AI preferences.' : 'The assistant is off. Choose a provider, a model and a key in AI preferences, then turn it on.');
    setDisabled(this.send, !c.configured && !this.busy);
    this.input.disabled = !c.configured;
  }

  private async history(uid: number, l: Lane): Promise<void> {
    try {
      const h = await api<{ doc: number | null; running: boolean; items: Hist[] }>('/assistant/history');
      if (h.doc !== uid || l.root.querySelector('.fa-msg, .fa-tool')) return;
      l.root.querySelector('.fa-hint')?.remove();
      for (const it of h.items) {
        if (it.kind === 'user') this.msg(l, 'user', it.text ?? '');
        else if (it.kind === 'assistant') this.msg(l, 'assistant', it.text ?? '');
        else this.tool(l, { id: it.id ?? '', name: it.name ?? '', args: it.args ?? '' }, it.ok, it.note);
      }
      this.scroll(l, true);
      if (!l.root.childElementCount) this.hint(l);
    } catch { if (!l.root.childElementCount) this.hint(l); }
  }

  private scroll(l: Lane, force = false): void {
    const r = l.root;
    if (force || r.scrollHeight - r.scrollTop - r.clientHeight < 90) r.scrollTop = r.scrollHeight;
  }

  private add(l: Lane, node: HTMLElement): void {
    l.root.querySelector('.fa-hint')?.remove();
    const near = l.root.scrollHeight - l.root.scrollTop - l.root.clientHeight < 90;
    l.root.appendChild(node);
    if (near) l.root.scrollTop = l.root.scrollHeight;
  }

  private msg(l: Lane, who: 'user' | 'assistant' | 'note', text: string): void {
    const n = el(`<div class="fa-msg fa-${who}"><div class="fa-bubble"></div></div>`);
    setText(q(n, '.fa-bubble'), text);
    this.add(l, n);
  }

  private tool(l: Lane, t: { id: string; name: string; args: string }, ok?: boolean, note?: string): void {
    const n = el(`<details class="fa-tool is-run"><summary><span class="fa-dot"></span><span class="fa-tname"></span><span class="fa-tstate">running</span></summary><div class="fa-tbody"><code class="fa-targs"></code><code class="fa-tnote"></code></div></details>`);
    setText(q(n, '.fa-tname'), t.name);
    setText(q(n, '.fa-targs'), t.args);
    l.tools.set(t.id, n);
    this.done(n, ok, note);
    this.add(l, n);
  }

  private done(n: HTMLElement, ok?: boolean, note?: string): void {
    if (ok === undefined) return;
    setClass(n, 'is-run', false);
    setClass(n, 'is-bad', !ok);
    setText(q(n, '.fa-tstate'), ok ? 'done' : 'not done');
    setText(q(n, '.fa-tnote'), note ?? '');
  }

  private approval(l: Lane, ev: Extract<Ev, { type: 'approval' }>): void {
    const n = el(`<div class="fa-approval"><div class="fa-approval-text"><b></b><span></span><code></code></div><div class="fa-approval-btns"><button class="cv-btn fa-deny" type="button">Decline</button><button class="cv-btn cv-btn-primary fa-ok" type="button">Approve</button></div></div>`);
    setText(q(n, 'b'), `${ev.tool} asks to run`);
    setText(q(n, 'span'), ` it ${ev.reason}`);
    setText(q(n, 'code'), ev.args);
    const decide = async (approve: boolean): Promise<void> => {
      for (const b of Array.from(n.querySelectorAll<HTMLButtonElement>('button'))) b.disabled = true;
      try {
        await api(`/agents/approvals/${ev.id}`, { json: { approve } });
        setText(q(n, 'b'), approve ? 'Approved' : 'Declined');
        q(n, '.fa-approval-btns').remove();
        setClass(n, 'is-decided', true);
      } catch (e) { setText(q(n, 'b'), (e as Error).message); q(n, '.fa-approval-btns').remove(); }
    };
    q(n, '.fa-ok').addEventListener('click', () => void decide(true));
    q(n, '.fa-deny').addEventListener('click', () => void decide(false));
    this.add(l, n);
    this.ws.showPanel('assistant');
  }

  private setBusy(on: boolean): void {
    this.busy = on;
    setText(this.send, on ? 'Stop' : 'Send');
    setClass(this.send, 'is-stop', on);
    setDisabled(this.send, !on && !(this.cfg?.configured));
    setClass(this.root, 'is-busy', on);
  }

  private async submit(): Promise<void> {
    const text = this.input.value.trim();
    const uid = this.uid();
    if (!text || uid === null || this.busy) return;
    if (!this.cfg?.configured) { await this.status(); if (!this.cfg?.configured) return; }
    const l = this.lane(uid);
    this.msg(l, 'user', text);
    this.input.value = '';
    this.setBusy(true);
    this.runDoc = uid;
    try {
      const r = await fetch('/api/assistant/chat', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ message: text, page: this.ws.viewport.currentPage() + 1 }) });
      if (!r.ok || !r.body) {
        let d = r.statusText;
        try { d = (await r.json()).detail ?? d; } catch { }
        throw new EngineError(typeof d === 'string' ? d : JSON.stringify(d));
      }
      const reader = r.body.getReader();
      const dec = new TextDecoder();
      let buf = '';
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        buf += dec.decode(value, { stream: true });
        let at: number;
        while ((at = buf.indexOf('\n\n')) >= 0) {
          const chunk = buf.slice(0, at);
          buf = buf.slice(at + 2);
          if (chunk.startsWith('data: ')) { try { this.handle(l, JSON.parse(chunk.slice(6)) as Ev); } catch { } }
        }
      }
    } catch (e) { this.msg(l, 'note', (e as Error).message); } finally {
      this.setBusy(false);
      this.runDoc = null;
    }
  }

  private handle(l: Lane, ev: Ev): void {
    switch (ev.type) {
      case 'text': this.msg(l, 'assistant', ev.text); break;
      case 'tool': this.tool(l, ev); break;
      case 'tool_result': { const n = l.tools.get(ev.id); if (n) this.done(n, ev.ok, ev.note); break; }
      case 'approval': this.approval(l, ev); break;
      case 'error': this.msg(l, 'note', ev.error); break;
      case 'stopped': this.msg(l, 'note', 'Stopped.'); break;
      default: break;
    }
  }

  private async stop(): Promise<void> {
    try { await api('/assistant/stop', { json: {} }); } catch (e) { toast((e as Error).message, 'error'); }
  }

  private async reset(): Promise<void> {
    const uid = this.uid();
    if (uid === null || this.busy) return;
    await api('/assistant/reset', { json: {} }).catch(() => undefined);
    const l = this.lane(uid);
    l.root.replaceChildren();
    l.tools.clear();
    this.hint(l);
  }
}

export function installAssistant(ws: Workspace): void {
  let panel: AssistantPanel | null = null;
  const make = (): AssistantPanel => (panel ??= new AssistantPanel(ws));
  ws.registerPanel({ id: 'assistant', label: 'Assistant', group: 'connect', column: 'left', order: 20, create: () => make().root, onShow: () => panel?.onShow() });
  ws.addCommands({ id: 'ui.assistant', label: 'Assistant', run: () => ws.showPanel('assistant') });
  ws.addMenuItems({ menu: 'Window', items: ['ui.assistant'], at: 'end' });
}
