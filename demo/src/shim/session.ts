export interface WsReply { ok: boolean; result?: unknown; error?: string; rev?: number; selRev?: number; changed?: boolean }
export interface HttpReply { status: number; type: string; json?: unknown; text?: string; b64?: string }
export interface Table { ws: Record<string, WsReply>; http: Record<string, HttpReply> }
export interface Render { s: number; w: number; h: number; b64: string }
export interface Spread { bounds: [number, number, number, number]; scales: Render[] }

export interface Session {
  v: number;
  demo: string;
  hello: { type: 'hello'; rev: number; selRev: number; core: string; running: boolean };
  home: Table;
  doc: Table;
  open: { key: string; reply: HttpReply } | null;
  overlays: WsReply;
  spreads: Spread[];
}

declare global { interface Window { FD_SESSION?: Session } }

export const NEEDS = 'This needs the FormDesign engine. This page replays the answers the engine gave when the demo was recorded; editing, saving, export, the assistant and agents need the app.';

export const session: Session | null = window.FD_SESSION ?? null;

let opened = false;
export const isOpen = (): boolean => opened;
export const markOpen = (): void => { opened = true; };

export function table(kind: 'ws' | 'http', key: string): WsReply | HttpReply | undefined {
  if (!session) return undefined;
  if (opened) {
    const hit = session.doc[kind][key];
    if (hit) return hit;
  }
  return session.home[kind][key];
}

export function canon(v: unknown): string {
  if (Array.isArray(v)) return `[${v.map(canon).join(',')}]`;
  if (v && typeof v === 'object') {
    const o = v as Record<string, unknown>;
    return `{${Object.keys(o).filter((k) => o[k] !== undefined).sort().map((k) => `${JSON.stringify(k)}:${canon(o[k])}`).join(',')}}`;
  }
  if (typeof v === 'number' && Number.isInteger(v)) return String(v);
  return JSON.stringify(v);
}

export function bytes(b64: string): Uint8Array<ArrayBuffer> {
  const s = atob(b64);
  const out = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
  return out;
}
