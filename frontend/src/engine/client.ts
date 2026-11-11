export interface CoreMeta {
  rev: number;
  selRev: number;
  changed?: boolean;
  damage?: number[][] | null;
  requests?: UiRequest[];
}

export interface UiRequest { kind: 'dialog' | 'view' | 'pick' | 'toast'; id?: string; text?: string; purpose?: string; params?: Record<string, unknown> }

export interface ChangeEvent { type: 'changed'; rev: number; selRev: number; docChanged: boolean; damage?: number[][] | null; source: string; method: string }

export interface Reply<T = unknown> { result: T; meta: CoreMeta }
export interface BinaryReply<T = unknown> extends Reply<T> { data: ArrayBuffer }

type Listener = (payload: any) => void;

interface Pending { resolve: (v: any) => void; reject: (e: Error) => void; binary: boolean }

export class EngineError extends Error {}

export class EngineClient {
  private ws: WebSocket | null = null;
  private seq = 0;
  private readonly pending = new Map<number, Pending>();
  private readonly listeners = new Map<string, Set<Listener>>();
  private retry = 0;
  private opening: Promise<void> | null = null;
  connected = false;
  rev = 0;
  selRev = 0;

  on(event: 'changed' | 'open' | 'close' | 'hello' | 'agents' | 'view' | 'motion' | 'workflow' | 'claude' | 'command', fn: Listener): () => void {
    let set = this.listeners.get(event);
    if (!set) { set = new Set(); this.listeners.set(event, set); }
    set.add(fn);
    return () => set?.delete(fn);
  }

  private emit(event: string, payload?: unknown): void {
    for (const fn of this.listeners.get(event) ?? []) {
      try { fn(payload); } catch (e) { console.error('[formdesign] listener failed', e); }
    }
  }

  connect(): Promise<void> {
    if (this.connected) return Promise.resolve();
    if (this.opening) return this.opening;
    this.opening = new Promise<void>((resolve) => {
      const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
      const ws = new WebSocket(`${proto}//${location.host}/api/ws`);
      ws.binaryType = 'arraybuffer';
      ws.onopen = () => { this.ws = ws; this.connected = true; this.retry = 0; this.opening = null; this.emit('open'); resolve(); };
      ws.onmessage = (ev) => this.receive(ev.data);
      ws.onclose = () => {
        const was = this.connected;
        this.connected = false;
        this.ws = null;
        this.opening = null;
        for (const [, p] of this.pending) p.reject(new EngineError('the FormDesign engine connection closed'));
        this.pending.clear();
        if (was) this.emit('close');
        const wait = Math.min(4000, 250 * 2 ** this.retry++);
        window.setTimeout(() => { void this.connect(); }, wait);
        resolve();
      };
    });
    return this.opening;
  }

  private receive(data: string | ArrayBuffer): void {
    if (typeof data === 'string') {
      let msg: any;
      try { msg = JSON.parse(data); } catch { return; }
      if (msg.type === 'changed') {
        this.rev = msg.rev; this.selRev = msg.selRev;
        this.emit('changed', msg as ChangeEvent);
        return;
      }
      if (msg.type === 'hello') { this.rev = msg.rev; this.selRev = msg.selRev; this.emit('hello', msg); return; }
      if (msg.type === 'agents' || msg.type === 'view' || msg.type === 'motion' || msg.type === 'workflow' || msg.type === 'claude' || msg.type === 'command') { this.emit(msg.type, msg); return; }
      this.settle(msg, null);
      return;
    }
    const view = new DataView(data);
    const n = view.getUint32(0);
    const head = JSON.parse(new TextDecoder().decode(new Uint8Array(data, 4, n)));
    this.settle(head, data.slice(4 + n));
  }

  private settle(msg: any, payload: ArrayBuffer | null): void {
    const p = this.pending.get(msg.id);
    if (!p) return;
    this.pending.delete(msg.id);
    if (typeof msg.rev === 'number') { this.rev = Math.max(this.rev, msg.rev); this.selRev = Math.max(this.selRev, msg.selRev ?? 0); }
    if (!msg.ok) { p.reject(new EngineError(msg.error || 'the engine refused')); return; }
    const meta: CoreMeta = { rev: msg.rev, selRev: msg.selRev, changed: msg.changed, damage: msg.damage, requests: msg.requests };
    p.resolve(p.binary ? { result: msg.result, meta, data: payload ?? new ArrayBuffer(0) } : { result: msg.result, meta });
  }

  private send(method: string, params: unknown, binary: boolean): Promise<any> {
    if (!this.ws || !this.connected) return Promise.reject(new EngineError('the FormDesign engine is not connected'));
    const id = ++this.seq;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject, binary });
      this.ws?.send(JSON.stringify({ id, method, params }));
    });
  }

  call<T = unknown>(method: string, params: unknown = {}): Promise<Reply<T>> { return this.send(method, params, false); }
  callBinary<T = unknown>(method: string, params: unknown = {}): Promise<BinaryReply<T>> { return this.send(method, params, true); }

  async exec<T = unknown>(command: string, params: Record<string, unknown> = {}): Promise<T> {
    return (await this.call<T>('exec', { command, params })).result;
  }
}

export async function api<T = unknown>(path: string, init?: RequestInit & { json?: unknown }): Promise<T> {
  const opts: RequestInit = { ...init };
  if (init?.json !== undefined) {
    opts.method = opts.method ?? 'POST';
    opts.headers = { 'Content-Type': 'application/json', ...(init.headers ?? {}) };
    opts.body = JSON.stringify(init.json);
  }
  const r = await fetch(`/api${path}`, opts);
  if (!r.ok) {
    let detail = r.statusText;
    try { const j = await r.json(); detail = j.detail ?? detail; } catch { }
    throw new EngineError(typeof detail === 'string' ? detail : JSON.stringify(detail));
  }
  const ct = r.headers.get('content-type') ?? '';
  return (ct.includes('application/json') ? r.json() : r.blob()) as Promise<T>;
}

export const engine = new EngineClient();
