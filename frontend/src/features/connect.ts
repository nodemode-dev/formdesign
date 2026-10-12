import { api, engine, type ChangeEvent } from '../engine/client';
import { editor } from '../editor/editor';
import { confirmAction } from '../ui/dialogs';
import { el, patchList, q, setClass, setHidden, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Client { name: string; version: string | null; transport: string | null; lastSeen: number | null; calls: number; policy: 'allow' | 'ask' | 'deny' }
interface Approval { id: string; client: string; tool: string; args: string; reason: string; at: number }
interface Act { id: number; at: number; client: string; tool: string; args: string; ok: boolean; ms: number; note: string; changed: boolean }
interface Snap { enabled: boolean; endpoint: string; stdio: string; clients: Client[]; approvals: Approval[]; activity: Act[] }

const FOLLOW_KEY = 'formdes.connect.follow';
const INSTALL: Array<[string, string]> = [['claude', 'Claude Code'], ['codex', 'Codex'], ['cursor', 'Cursor']];
const ago = (t: number | null): string => {
  if (!t) return 'not seen';
  const s = Math.max(0, Math.round(Date.now() / 1000 - t));
  return s < 60 ? `${s}s ago` : s < 3600 ? `${Math.round(s / 60)}m ago` : `${Math.round(s / 3600)}h ago`;
};
const clock = (t: number): string => new Date(t * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' });

class ConnectPanel {
  readonly root = el(`<div class="fc-connect">
    <div class="fc-sec">
      <label class="fc-switch"><input type="checkbox" class="fc-on"><span>Allow agents to use FormDesign</span></label>
      <div class="fc-kv"><span>HTTP</span><code class="fc-endpoint"></code></div>
      <div class="fc-kv"><span>stdio</span><code class="fc-stdio"></code></div>
      <label class="fc-switch"><input type="checkbox" class="fc-follow"><span>Follow the agent: show each change it makes</span></label>
    </div>
    <div class="fc-sec fc-approvals-sec" hidden>
      <h4 class="fc-h">Waiting for you</h4>
      <div class="fc-approvals"></div>
    </div>
    <div class="fc-sec">
      <h4 class="fc-h">Agents</h4>
      <div class="fc-clients"></div>
      <p class="cv-note fc-none">No agent has connected since the engine started. Set one up below, or start one from the Terminal panel.</p>
    </div>
    <div class="fc-sec">
      <h4 class="fc-h">Set up</h4>
      <div class="fc-install"></div>
    </div>
    <div class="fc-sec fc-grow">
      <h4 class="fc-h">Activity</h4>
      <div class="fc-activity"></div>
    </div>
  </div>`);
  private readonly on = q<HTMLInputElement>(this.root, '.fc-on');
  private readonly follow = q<HTMLInputElement>(this.root, '.fc-follow');
  private snap: Snap | null = null;
  private loading = false;
  private again = false;
  private ticker = 0;

  constructor(private readonly ws: Workspace) {
    try { this.follow.checked = localStorage.getItem(FOLLOW_KEY) === '1'; } catch { }
    this.follow.addEventListener('change', () => { try { localStorage.setItem(FOLLOW_KEY, this.follow.checked ? '1' : '0'); } catch { } });
    this.on.addEventListener('change', () => void api('/agents/enabled', { method: 'POST', json: { enabled: this.on.checked } }).catch((e) => toast((e as Error).message, 'error')));
    const inst = q(this.root, '.fc-install');
    for (const [id, label] of INSTALL) {
      const b = el<HTMLButtonElement>('<button class="cv-btn fc-install-btn" type="button"></button>');
      setText(b, label);
      b.title = `Write ${label}'s MCP configuration and the FormDesign skills for your user`;
      b.addEventListener('click', () => void this.install(id, label));
      inst.appendChild(b);
    }
    engine.on('agents', (e: { what: string; client?: string; tool?: string; reason?: string }) => {
      if (e.what === 'approval') toast(`${e.client} asks to run ${e.tool}: it ${e.reason}. Approve or decline in Agent Connect.`);
      void this.load();
    });
    engine.on('changed', (e: ChangeEvent) => this.followChange(e));
    engine.on('open', () => void this.load());
  }

  onShow(): void {
    void this.load();
    if (!this.ticker) this.ticker = window.setInterval(() => { if (this.root.isConnected && this.snap) this.renderClients(); }, 15000);
  }

  private async load(): Promise<void> {
    if (this.loading) { this.again = true; return; }
    this.loading = true;
    try {
      this.snap = await api<Snap>('/agents');
      this.render();
    } catch { } finally {
      this.loading = false;
      if (this.again) { this.again = false; void this.load(); }
    }
  }

  private render(): void {
    const s = this.snap;
    if (!s) return;
    if (this.on.checked !== s.enabled) this.on.checked = s.enabled;
    setText(q(this.root, '.fc-endpoint'), s.endpoint);
    setText(q(this.root, '.fc-stdio'), s.stdio);
    this.renderClients();
    setHidden(q(this.root, '.fc-approvals-sec'), s.approvals.length === 0);
    patchList<Approval>(q(this.root, '.fc-approvals'), s.approvals, {
      key: (a) => a.id,
      create: (a) => {
        const n = el(`<div class="fc-approval"><div class="fc-approval-text"><b></b><span></span><code></code></div>
          <div class="fc-approval-btns"><button class="cv-btn fc-deny" type="button">Decline</button><button class="cv-btn cv-btn-primary fc-ok" type="button">Approve</button></div></div>`);
        q(n, '.fc-ok').addEventListener('click', () => void this.decide(n.dataset.key as string, true));
        q(n, '.fc-deny').addEventListener('click', () => void this.decide(n.dataset.key as string, false));
        this.fillApproval(n, a);
        return n;
      },
      update: (n, a) => this.fillApproval(n, a),
    });
    patchList<Act>(q(this.root, '.fc-activity'), s.activity.slice(0, 40), {
      key: (a) => String(a.id),
      create: (a) => { const n = el('<div class="fc-act"><span class="fc-act-time"></span><span class="fc-act-tool"></span><span class="fc-act-note"></span></div>'); this.fillAct(n, a); return n; },
      update: (n, a) => this.fillAct(n, a),
    });
  }

  private renderClients(): void {
    const s = this.snap;
    if (!s) return;
    setHidden(q(this.root, '.fc-none'), s.clients.length > 0);
    patchList<Client>(q(this.root, '.fc-clients'), s.clients, {
      key: (c) => c.name,
      create: (c) => {
        const n = el(`<div class="fc-client"><div class="fc-client-id"><b></b><span class="fc-client-meta"></span></div>
          <select class="cv-select fc-policy" aria-label="What this agent may do"><option value="allow">Allow all</option><option value="ask">Ask for risky steps</option><option value="deny">Deny</option></select></div>`);
        const sel = q<HTMLSelectElement>(n, '.fc-policy');
        sel.addEventListener('change', () => void api('/agents/policy', { method: 'POST', json: { client: n.dataset.key, policy: sel.value } }).catch((e) => toast((e as Error).message, 'error')));
        this.fillClient(n, c);
        return n;
      },
      update: (n, c) => this.fillClient(n, c),
    });
  }

  private fillClient(n: HTMLElement, c: Client): void {
    setText(q(n, 'b'), c.name);
    setText(q(n, '.fc-client-meta'), [c.version ? `v${c.version}` : '', c.transport ?? '', `${c.calls} calls`, ago(c.lastSeen)].filter(Boolean).join(' · '));
    const sel = q<HTMLSelectElement>(n, '.fc-policy');
    if (sel.value !== c.policy && document.activeElement !== sel) sel.value = c.policy;
  }

  private fillApproval(n: HTMLElement, a: Approval): void {
    setText(q(n, 'b'), `${a.client} · ${a.tool}`);
    setText(q(n, 'span'), ` ${a.reason}`);
    setText(q(n, 'code'), a.args);
  }

  private fillAct(n: HTMLElement, a: Act): void {
    setText(q(n, '.fc-act-time'), clock(a.at));
    setText(q(n, '.fc-act-tool'), `${a.client} · ${a.tool}`);
    setText(q(n, '.fc-act-note'), a.ok ? (a.changed ? `changed the document · ${a.ms} ms` : `${a.ms} ms`) : a.note);
    setClass(n, 'is-error', !a.ok);
    n.title = a.args;
  }

  private async decide(id: string, approve: boolean): Promise<void> {
    try { await api(`/agents/approvals/${id}`, { method: 'POST', json: { approve } }); } catch (e) { toast((e as Error).message, 'error'); }
    void this.load();
  }

  private async install(id: string, label: string): Promise<void> {
    try {
      const dry = await api<{ text: string }>('/agents/install', { method: 'POST', json: { client: id, dry: true } });
      if (!(await confirmAction(`Set up ${label}`, dry.text, 'Set up'))) return;
      const r = await api<{ text: string }>('/agents/install', { method: 'POST', json: { client: id } });
      toast(r.text.split('\n').slice(-1)[0] || `${label} is set up`);
    } catch (e) { toast((e as Error).message, 'error'); }
  }

  private followChange(e: ChangeEvent): void {
    if (!this.follow.checked || e.source !== 'mcp' || !e.docChanged || !e.damage?.length || !editor.hasDoc) return;
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (const r of e.damage) { x0 = Math.min(x0, r[0]); y0 = Math.min(y0, r[1]); x1 = Math.max(x1, r[2]); y1 = Math.max(y1, r[3]); }
    if (!Number.isFinite(x0) || x1 - x0 > 1e6) return;
    this.ws.viewport.reveal([x0, y0, x1, y1]);
  }
}

export function installConnect(ws: Workspace): void {
  let panel: ConnectPanel | null = null;
  const make = (): ConnectPanel => (panel ??= new ConnectPanel(ws));
  ws.registerPanel({ id: 'connect', label: 'Agent Connect', group: 'connect', column: 'left', order: 10, create: () => make().root, onShow: () => panel?.onShow() });
  engine.on('agents', (e: { what: string }) => { if (e.what === 'approval') { make(); ws.showPanel('connect'); } });
}
