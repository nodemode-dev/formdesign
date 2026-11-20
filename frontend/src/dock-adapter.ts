import type { ConsentChoice, ConsentRequest, DockAdapter, DockAgentStatus, DockChannel, DockLegacy, DockOwnChat, DockTerminal, PtyKind, StoredChoice, StoredChoiceState } from './dock/adapter';
import { engine } from './engine/client';
import { keyHeaders, streamTicket } from './engine/window-key';

export const DOCK_KEY = 'formdesign.agentDock';
export const DOCK_OLD_HEIGHT = 'formdesign.dock.height';
export const DOCK_RIGHT_MIN = 1200;
export const DOCK_SIZES = { minHeight: 140, maxHeightShare: 0.7, defaultHeight: 280, minWidth: 320, maxWidthShare: 0.3, defaultWidth: 380 };
export const RISKY_NOTE = 'FormDesign asks about steps like this one only. Other edits by this agent run without a question.';

export interface ChatView {
  readonly root: HTMLElement;
  onShow(): void;
  send(text: string, context: string): Promise<boolean>;
  isBusy(): boolean;
  onHandoff: (session: string) => void;
  onSuggest: (text: string) => void;
}

export class DockError extends Error {
  constructor(message: string, readonly status: number) { super(message); }
}

async function call<T>(method: string, path: string, body?: unknown): Promise<T> {
  const response = await fetch(path, { method, headers: body === undefined ? keyHeaders() : { 'Content-Type': 'application/json', ...keyHeaders() }, body: body === undefined ? undefined : JSON.stringify(body) });
  const data = await response.json().catch(() => ({})) as { detail?: unknown };
  if (!response.ok) throw new DockError(typeof data.detail === 'string' ? data.detail : `Request failed (${response.status}).`, response.status);
  return data as T;
}

export const oldHeight: DockLegacy = {
  keys: [DOCK_OLD_HEIGHT],
  read: (store) => {
    const h = Number(store.getItem(DOCK_OLD_HEIGHT));
    return h > 0 ? { h: Math.round(Math.max(DOCK_SIZES.minHeight, h)), seen: true } : null;
  },
};

interface Asked { id?: unknown; client?: unknown; agent?: unknown; asked?: unknown; tool?: unknown; reason?: unknown; detail?: unknown; answers?: unknown; alwaysScope?: unknown; expiresAt?: unknown }
interface AgentsEvent extends Asked { what?: string; withdrawn?: boolean }
interface AgentsState { enabled?: boolean; sealBroken?: boolean; approvals?: Asked[] }
const CHOICES: ConsentChoice[] = ['deny', 'once', 'always'];

export function consentRequest(a: Asked): ConsentRequest | null {
  const id = typeof a.id === 'string' ? a.id : '';
  if (!id || a.client === 'assistant') return null;
  const given = Array.isArray(a.answers) ? a.answers as unknown[] : ['deny', 'once'];
  const offers = CHOICES.filter((c) => given.includes(c));
  const name = typeof a.agent === 'string' && a.agent ? a.agent : typeof a.client === 'string' ? a.client : '';
  const detail = typeof a.detail === 'string' ? a.detail : '';
  return {
    id, agent: name,
    what: typeof a.asked === 'string' && a.asked ? a.asked : `run ${typeof a.tool === 'string' ? a.tool : 'a tool'}: it ${typeof a.reason === 'string' ? a.reason : 'needs your approval'}`,
    offers: offers.length ? offers : ['deny'], always: a.alwaysScope === 'all' ? 'all' : 'agent',
    ...(typeof a.expiresAt === 'number' ? { expires: a.expiresAt * 1000 } : {}),
    notes: detail ? [RISKY_NOTE, detail] : [RISKY_NOTE],
  };
}

function subscribe(add: (request: ConsentRequest) => void, withdraw: (id: string) => void): void {
  const known = new Set<string>();
  const list = async (): Promise<void> => {
    let rows: Asked[];
    try { rows = (await call<AgentsState>('GET', '/api/agents')).approvals ?? []; } catch { return; }
    const live = new Set<string>();
    for (const row of rows) {
      const r = consentRequest(row);
      if (!r) continue;
      live.add(r.id);
      known.add(r.id);
      add(r);
    }
    for (const id of [...known]) if (!live.has(id)) { known.delete(id); withdraw(id); }
  };
  engine.on('agents', (e: AgentsEvent) => {
    if (e.what === 'approval') {
      const r = consentRequest(e);
      if (r) { known.add(r.id); add(r); }
    } else if (e.what === 'approvals' && e.withdrawn === true && typeof e.id === 'string') {
      known.delete(e.id);
      withdraw(e.id);
    }
  });
  engine.on('open', () => void list());
}

async function answer(id: string, choice: ConsentChoice): Promise<void> {
  await call('POST', `/api/agents/approvals/${encodeURIComponent(id)}`, choice === 'deny' ? { approve: false } : choice === 'always' ? { approve: true, always: true } : { approve: true });
}

async function storedChoice(): Promise<StoredChoiceState> {
  const s = await call<AgentsState>('GET', '/api/agents');
  return { value: s.enabled === false ? 'blocked' : 'ask', changedOutside: s.sealBroken === true };
}

async function storeChoice(value: StoredChoice): Promise<void> {
  await call('POST', '/api/agents/enabled', { enabled: value !== 'blocked' });
}

async function channel(id: string): Promise<DockChannel> {
  const ticket = await streamTicket().catch(() => '');
  const socket = new WebSocket(`${location.protocol === 'https:' ? 'wss:' : 'ws:'}//${location.host}/api/term/${encodeURIComponent(id)}/ws?ticket=${encodeURIComponent(ticket)}&notices=1`);
  const bytes = new TextEncoder();
  const waiting: Array<string | Uint8Array<ArrayBuffer>> = [];
  let data = (_chunk: string | Uint8Array): void => undefined;
  let exit = (): void => undefined;
  let closed = (): void => undefined;
  socket.binaryType = 'arraybuffer';
  socket.addEventListener('open', () => { for (const frame of waiting.splice(0)) socket.send(frame); });
  socket.addEventListener('message', (msg) => {
    if (typeof msg.data !== 'string') { data(new Uint8Array(msg.data as ArrayBuffer)); return; }
    try { if ((JSON.parse(msg.data) as { type?: string }).type === 'exit') exit(); } catch { return; }
  });
  socket.addEventListener('close', () => closed());
  const send = (frame: string | Uint8Array<ArrayBuffer>): void => {
    if (socket.readyState === WebSocket.OPEN) socket.send(frame);
    else if (socket.readyState === WebSocket.CONNECTING) waiting.push(frame);
  };
  return {
    send: (chunk) => send(bytes.encode(chunk)),
    resize: (cols, rows) => send(JSON.stringify({ resize: [cols, rows] })),
    onData: (handler) => { data = handler; },
    onExit: (handler) => { exit = handler; },
    onClose: (handler) => { closed = handler; },
    close: () => socket.close(),
  };
}

interface Session { id: string; preset: string; title?: string; alive?: boolean }
const KINDS: Array<PtyKind | 'antigravity'> = ['shell', 'claude', 'codex', 'cursor', 'antigravity'];
const NAMES: Record<string, string> = { shell: 'Terminal', claude: 'Claude Code', codex: 'Codex', cursor: 'Cursor', antigravity: 'Antigravity' };
const titles = new Map<string, { base: string; shown: string }>();

function record(s: Session, alive: boolean): DockTerminal<'antigravity'> {
  const kind = KINDS.find((k) => k === s.preset) ?? 'shell';
  const base = s.title || NAMES[kind] || 'Terminal';
  let have = titles.get(s.id);
  if (!have || have.base !== base) {
    const taken = new Set([...titles].filter(([id, t]) => id !== s.id && t.base === base).map(([, t]) => t.shown));
    let n = 1;
    while (taken.has(n === 1 ? base : `${base} ${n}`)) n += 1;
    have = { base, shown: n === 1 ? base : `${base} ${n}` };
    titles.set(s.id, have);
  }
  return { id: s.id, title: have.shown, alive, kind };
}

export interface DockDeps {
  context(): string;
  chat(): ChatView | null;
  handoff(session: string): void;
  suggest(text: string): void;
  openSettings(): void;
  isChord(event: KeyboardEvent): boolean;
  chordLabel(): string;
}

export function formdesignDock(deps: DockDeps): DockAdapter<'antigravity'> {
  return {
    words: {
      app: 'FormDesign', noun: 'document', example: 'Check every page for overset text and fix it', start: './formdesign boot',
      lead: "Ask Claude about the open document in the line below. It uses your own Claude Code sign-in and FormDesign's tools; every change appears in the window as it happens and can be undone.",
    },
    strings: {
      chatPlaceholder: 'Ask Claude about your document',
      empty: 'No terminal is open. Start a shell with the plus button, Claude Code with the robot button, or Chat with the speech bubble.',
      outsideAgents: 'Agent Connect',
      consentTitle: 'Allow an agent calling itself {agent} to run this step in {app}?',
      consentTitleUnnamed: 'Allow an outside agent to run this step in {app}?',
      consentChanged: 'The saved agent permission was changed outside FormDesign, so it is back to Ask.',
      tourAskBody: 'If another AI program on this computer wants to run a step that cannot be undone or that reaches outside your document, such as printing, publishing or overwriting a file, FormDesign asks you first in a box in the middle of the window. Choose Deny, Allow once or Always allow. Its other edits are not asked about. You can change what each agent may do under Agent Connect in the menu here.',
    },
    placement: {
      mode: 'own',
      storage: { key: DOCK_KEY, legacy: oldHeight },
      sizes: DOCK_SIZES,
      canDockRight: () => window.innerWidth >= DOCK_RIGHT_MIN,
    },
    passKey: () => false,
    isDockChord: (e) => e.target instanceof Element && e.target.closest('.ft-dock') !== null && deps.isChord(e),
    chordLabel: deps.chordLabel,
    tabs: () => ({ builtin: ['chat', 'shell', 'claude', 'codex', 'cursor', 'antigravity'] }),
    classes: { iconButton: 'cv-icon-btn', button: 'cv-btn', primaryButton: 'cv-btn-primary', input: 'cv-input', select: 'cv-select', note: 'cv-note' },
    t: (_key, english) => english,
    onTheme: (handler) => { document.addEventListener('nm:ui-settings-changed', handler); },
    keyHeaders,
    errors: {
      message: (err) => (err instanceof DockError ? err.message : null),
      kind: (err) => {
        if (err instanceof TypeError) return 'unreachable';
        if (!(err instanceof DockError)) return null;
        if (err.status === 502 || err.status === 503 || err.status === 504) return 'unreachable';
        if (err.status === 403) return 'refused';
        if (err.status === 400 && /not installed|was not found\. Looked in|but does not start/i.test(err.message)) return 'notInstalled';
        return (err.status === 404 || err.status === 405) && /^(Not Found|Method Not Allowed)$/i.test(err.message) ? 'oldEngine' : null;
      },
    },
    terminal: {
      list: async () => {
        const sessions = (await call<{ sessions: Session[] }>('GET', '/api/term')).sessions;
        for (const id of [...titles.keys()]) if (!sessions.some((s) => s.id === id)) titles.delete(id);
        return sessions.map((s) => record(s, s.alive !== false));
      },
      open: async (kind, o) => {
        if (kind !== 'claude') return record(await call<Session>('POST', '/api/term', { preset: kind, cols: o.cols, rows: o.rows }), true);
        const extra = o.prompt ? deps.context() : '';
        return record(await call<Session>('POST', '/api/term', {
          preset: 'claude', cols: o.cols, rows: o.rows, reuse: !o.resume,
          ...(o.prompt ? { prompt: extra ? `${o.prompt} (${extra})` : o.prompt } : {}), ...(o.resume ? { resume: o.resume } : {}),
        }), true);
      },
      restart: async (id) => { await call('POST', `/api/term/${encodeURIComponent(id)}/restart`, {}); },
      close: async (id) => {
        titles.delete(id);
        await call('DELETE', `/api/term/${encodeURIComponent(id)}`);
      },
      channel,
      scrollback: 5000,
    },
    agentStatus: () => call<DockAgentStatus>('GET', '/api/agents/programs'),
    chat: {
      pane: (host): DockOwnChat => {
        const chat = deps.chat();
        if (!chat) return {};
        chat.onHandoff = deps.handoff;
        chat.onSuggest = deps.suggest;
        host.append(chat.root);
        return {
          send: (text) => chat.send(text, deps.context()),
          shown: () => chat.onShow(),
          busy: () => chat.isBusy(),
        };
      },
      ask: 'line',
    },
    consent: { subscribe, answer },
    storedChoice: { get: storedChoice, set: storeChoice },
    openSettings: deps.openSettings,
  };
}
