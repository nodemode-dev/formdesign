import { toast } from '../../../frontend/src/ui/toast';
import { bytes, canon, isOpen, markOpen, NEEDS, session, table, type HttpReply, type WsReply } from './session';
import { drawTiles, recordedView } from './tiles';

const BUST = new Set(['r', 'v', 't', 'rev']);
const NO_REPLAY = new Set(['pointer', 'key', 'text', 'tool']);

const pathOf = (raw: string): string | null => {
  const at = raw.indexOf('/api/');
  if (at < 0 || !(raw.startsWith('/api/') || raw.startsWith('file:'))) return null;
  const [p, q = ''] = raw.slice(at + 5).split('?');
  const kept = q.split('&').filter((kv) => kv && !BUST.has(kv.split('=')[0]));
  return kept.length ? `${p}?${kept.join('&')}` : p;
};

const blobOf = (r: HttpReply): Blob => r.b64 !== undefined ? new Blob([bytes(r.b64)], { type: r.type })
  : new Blob([r.json !== undefined ? JSON.stringify(r.json) : r.text ?? ''], { type: r.type });

const respond = (r: HttpReply): Response => new Response(r.status === 204 ? null : blobOf(r), { status: r.status, headers: { 'Content-Type': r.type } });

const needs = (): Response => new Response(JSON.stringify({ detail: NEEDS }), { status: 503, headers: { 'Content-Type': 'application/json' } });

let pulse: (() => void) | null = null;

export const tileWork = { busy: 0, done: 0, at: 0 };

const lookup = (method: string, path: string, body: string): HttpReply | undefined => {
  const k = `${method} ${path}`;
  return table('http', body ? `${k} ${body}` : k) as HttpReply | undefined ?? table('http', k) as HttpReply | undefined;
};

const native = window.fetch.bind(window);
window.fetch = async (input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
  const raw = typeof input === 'string' ? input : input instanceof URL ? input.href : input.url;
  const path = pathOf(raw);
  if (path === null) return native(input, init);
  const method = (init?.method ?? (input instanceof Request ? input.method : 'GET')).toUpperCase();
  const body = typeof init?.body === 'string' ? init.body : '';
  if (session?.open && `${method} ${path}` === session.open.key) {
    if (isOpen()) return needs();
    markOpen();
    window.setTimeout(() => pulse?.(), 0);
    return respond(session.open.reply);
  }
  const hit = lookup(method, path, body);
  return hit ? respond(hit) : needs();
};

const images = new Map<string, string>();
const srcOf = (value: string): string => {
  const path = pathOf(value);
  if (path === null) return value;
  const hit = lookup('GET', path, '');
  if (!hit || hit.b64 === undefined) return 'data:,';
  let url = images.get(path);
  if (!url) { url = URL.createObjectURL(blobOf(hit)); images.set(path, url); }
  return url;
};
const imgSrc = Object.getOwnPropertyDescriptor(HTMLImageElement.prototype, 'src');
if (imgSrc?.set && imgSrc.get) {
  const set = imgSrc.set, get = imgSrc.get;
  Object.defineProperty(HTMLImageElement.prototype, 'src', { configurable: true, enumerable: true, get() { return get.call(this); }, set(v: string) { set.call(this, srcOf(String(v))); } });
}
const setAttr = Element.prototype.setAttribute;
Element.prototype.setAttribute = function (name: string, value: string): void {
  setAttr.call(this, name, this instanceof HTMLImageElement && name === 'src' ? srcOf(String(value)) : value);
};

const warned = new Set<string>();
const warnOnce = (what: string): void => {
  if (warned.has(what)) return;
  warned.add(what);
  toast(NEEDS, 'error');
};

class ReplaySocket extends EventTarget {
  static readonly CONNECTING = 0;
  static readonly OPEN = 1;
  static readonly CLOSING = 2;
  static readonly CLOSED = 3;
  readyState = 0;
  binaryType: BinaryType = 'blob';
  onopen: ((e: Event) => void) | null = null;
  onclose: ((e: Event) => void) | null = null;
  onerror: ((e: Event) => void) | null = null;
  onmessage: ((e: MessageEvent) => void) | null = null;
  private rev = 1;
  private selRev = 1;

  constructor(readonly url: string) {
    super();
    window.setTimeout(() => {
      this.readyState = 1;
      const e = new Event('open');
      this.onopen?.(e);
      this.dispatchEvent(e);
      if (!session) return;
      this.rev = session.hello.rev;
      this.selRev = session.hello.selRev;
      this.deliver(JSON.stringify(session.hello));
      pulse = () => this.changed();
    }, 0);
  }

  private deliver(data: string | ArrayBuffer): void {
    if (this.readyState !== 1) return;
    const e = new MessageEvent('message', { data });
    this.onmessage?.(e);
    this.dispatchEvent(e);
  }

  private changed(): void {
    const st = table('ws', 'state {}') as WsReply | undefined;
    this.rev = st?.rev ?? this.rev + 1;
    this.selRev = st?.selRev ?? this.selRev + 1;
    this.deliver(JSON.stringify({ type: 'changed', rev: this.rev, selRev: this.selRev, docChanged: true, source: 'rest', method: 'batch', damage: null }));
  }

  send(raw: string): void {
    let msg: { id: number; method: string; params?: Record<string, unknown> };
    try { msg = JSON.parse(raw); } catch { return; }
    void this.answer(msg.id, msg.method, msg.params ?? {});
  }

  private async answer(id: number, method: string, params: Record<string, unknown>): Promise<void> {
    await new Promise((done) => window.setTimeout(done, 0));
    const meta = { rev: this.rev, selRev: this.selRev, changed: false };
    if (method === 'tiles' && session && isOpen()) {
      if (!recordedView(params)) { warnOnce('view'); this.deliver(JSON.stringify({ id, ok: false, error: NEEDS })); return; }
      tileWork.busy++;
      const { result, data } = await drawTiles(params).finally(() => { tileWork.busy--; tileWork.done++; tileWork.at = performance.now(); });
      const head = new TextEncoder().encode(JSON.stringify({ id, ok: true, result, ...meta }));
      const buf = new Uint8Array(4 + head.length + data.length);
      new DataView(buf.buffer).setUint32(0, head.length);
      buf.set(head, 4);
      buf.set(data, 4 + head.length);
      this.deliver(buf.buffer);
      return;
    }
    let hit = NO_REPLAY.has(method) ? undefined : table('ws', `${method} ${canon(params)}`) as WsReply | undefined;
    if (!hit && method === 'overlays' && isOpen()) hit = session?.overlays;
    this.deliver(JSON.stringify(hit ? { ...meta, ...hit, id } : { id, ok: false, error: NEEDS }));
  }

  close(): void {
    if (this.readyState === 3) return;
    this.readyState = 3;
    const e = new Event('close');
    this.onclose?.(e);
    this.dispatchEvent(e);
  }
}

class QuietSocket extends EventTarget {
  readyState = 0;
  binaryType: BinaryType = 'blob';
  onopen: ((e: Event) => void) | null = null;
  onclose: ((e: Event) => void) | null = null;
  onerror: ((e: Event) => void) | null = null;
  onmessage: ((e: MessageEvent) => void) | null = null;
  constructor(readonly url: string) {
    super();
    window.setTimeout(() => { this.readyState = 3; const e = new Event('close'); this.onclose?.(e); this.dispatchEvent(e); }, 0);
  }
  send(): void { return; }
  close(): void { this.readyState = 3; }
}

const NativeSocket = window.WebSocket;
window.WebSocket = new Proxy(NativeSocket, {
  construct(target, args: ConstructorParameters<typeof WebSocket>) {
    const url = String(args[0]);
    if (url.endsWith('/api/ws')) return new ReplaySocket(url) as unknown as WebSocket;
    return url.includes('/api/') ? new QuietSocket(url) as unknown as WebSocket : new target(...args);
  },
});
