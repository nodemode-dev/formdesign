import { api, engine, type ChangeEvent, type UiRequest } from '../engine/client';
import { toast } from '../ui/toast';

export type Rect4 = [number, number, number, number];

export interface PageInfo { abs: number; name: string; spread: number; index: number; rect: Rect4; parent: string | null }
export interface SlotInfo { spread: number; bounds: Rect4; rotation: number; xf: number[] }
export interface LayoutInfo { slots: SlotInfo[]; pages: PageInfo[]; pasteboard: Rect4; editingParents: boolean; bleed: number | number[] }
export interface DocInfo {
  index: number; uid: number; title: string; dirty: boolean; path: string | null; pages: number;
  canUndo: boolean; canRedo: boolean; undoLabel: string | null; redoLabel: string | null; template?: string | null; trackChanges?: boolean;
}
export interface CoreCommand { id: string; label: string; menu: string[]; params: string; shortcut: string | null; enabled: boolean; undoable?: boolean; help?: string }
export interface ToolInfo { id: string; label: string; icon: string; shortcut: string | null }
export interface SelectionInfo { items: number[]; content?: boolean; text?: unknown; [k: string]: unknown }

type EventName = 'ready' | 'docs' | 'layout' | 'selection' | 'tool' | 'mode' | 'change' | 'commands' | 'busy' | 'request' | 'connection';
type Listener = (payload?: any) => void;

export class Editor {
  docs: DocInfo[] = [];
  active: number | null = null;
  layout: LayoutInfo | null = null;
  selection: SelectionInfo | null = null;
  tool = 'selection';
  drawMode = 'normal';
  isolated: number | null = null;
  private modeKey = 'normal|';
  wantsText = false;
  busy = false;
  tools: ToolInfo[][] = [];
  commands: CoreCommand[] = [];
  connected = false;
  coreVersion = '';
  private layoutKey = '';
  private selKey = '';
  private docsKey = '';
  private listeners = new Map<EventName, Set<Listener>>();
  private scheduled = false;
  private needLayout = true;
  private syncing: Promise<void> | null = null;
  private again = false;
  private inspectCache: { rev: number; data: Promise<any> } | null = null;
  private commandIndex = new Map<string, CoreCommand>();
  private rawDocsKey = '';
  private docMeta = new Map<number, { uid: number; title: string; template: string | null; path: string | null }>();

  constructor() {
    engine.on('hello', (h: { core: string; running: boolean }) => { this.coreVersion = h.core; this.connected = true; this.emit('connection', true); void this.boot(); });
    engine.on('close', () => { this.connected = false; this.emit('connection', false); });
    engine.on('changed', (e: ChangeEvent) => this.onChanged(e));
  }

  on(ev: EventName, fn: Listener): () => void {
    let s = this.listeners.get(ev);
    if (!s) { s = new Set(); this.listeners.set(ev, s); }
    s.add(fn);
    return () => s?.delete(fn);
  }

  emit(ev: EventName, payload?: unknown): void {
    for (const fn of this.listeners.get(ev) ?? []) {
      try { fn(payload); } catch (err) { console.error('[formdesign]', ev, err); }
    }
  }

  get doc(): DocInfo | null { return this.active === null ? null : this.docs[this.active] ?? null; }
  get hasDoc(): boolean { return this.active !== null && this.docs.length > 0; }
  command(id: string): CoreCommand | undefined { return this.commandIndex.get(id); }

  private async boot(): Promise<void> {
    try {
      const [tools, cmds] = await Promise.all([engine.call<ToolInfo[][]>('tools'), engine.call<CoreCommand[] | { commands: CoreCommand[] }>('commands')]);
      this.tools = tools.result ?? [];
      const list = Array.isArray(cmds.result) ? cmds.result : cmds.result.commands;
      this.commands = list;
      this.commandIndex = new Map(list.map((c) => [c.id, c]));
      this.emit('commands');
    } catch (err) {
      toast(`The layout core did not answer: ${(err as Error).message}`, 'error');
    }
    this.needLayout = true;
    await this.sync();
    this.emit('ready');
  }

  private onChanged(e: ChangeEvent): void {
    if (e.docChanged) { this.needLayout = true; this.inspectCache = null; }
    this.emit('change', e);
    this.schedule();
  }

  refreshMeta(): void { this.rawDocsKey = ''; this.schedule(); }

  schedule(): void {
    if (this.scheduled) return;
    this.scheduled = true;
    requestAnimationFrame(() => { this.scheduled = false; void this.sync(); });
  }

  async sync(): Promise<void> {
    if (this.syncing) { this.again = true; return this.syncing; }
    this.syncing = this.doSync().finally(() => {
      this.syncing = null;
      if (this.again) { this.again = false; void this.sync(); }
    });
    return this.syncing;
  }

  private async doSync(): Promise<void> {
    if (!engine.connected) return;
    let st: any;
    try { st = (await engine.call('state')).result; } catch { return; }
    let docs: DocInfo[] = st.docs ?? [];
    const rawKey = JSON.stringify(docs.map((d) => [d.uid, d.title, d.dirty, d.path]));
    if (rawKey !== this.rawDocsKey) {
      this.rawDocsKey = rawKey;
      try {
        const ov = await api<{ docs: Array<{ uid: number; title: string; template: string | null; path: string | null }> }>('/documents');
        this.docMeta = new Map(ov.docs.map((d) => [d.uid, d]));
      } catch { }
    }
    docs = docs.map((d) => {
      const m = this.docMeta.get(d.uid);
      return { ...d, title: m?.title ?? d.title, path: m?.path ?? d.path, template: m?.template ?? null };
    });
    const docsKey = JSON.stringify([docs, st.active]);
    const switched = st.active !== this.active || docs[st.active ?? -1]?.uid !== this.doc?.uid;
    if (docsKey !== this.docsKey) {
      this.docs = docs;
      this.active = st.active ?? null;
      this.docsKey = docsKey;
      this.emit('docs');
    }
    if (switched) { this.needLayout = true; this.inspectCache = null; }
    const selKey = JSON.stringify([st.selection, st.wantsText]);
    if (selKey !== this.selKey) {
      this.selKey = selKey;
      this.selection = st.selection;
      this.wantsText = !!st.wantsText;
      this.emit('selection');
    }
    if (st.tool !== this.tool) { this.tool = st.tool; this.emit('tool'); }
    const modeKey = `${st.drawMode ?? 'normal'}|${st.isolated ?? ''}`;
    if (modeKey !== this.modeKey) {
      this.modeKey = modeKey;
      this.drawMode = st.drawMode ?? 'normal';
      this.isolated = st.isolated ?? null;
      this.emit('mode');
    }
    if (!!st.busy !== this.busy) { this.busy = !!st.busy; this.emit('busy'); }
    if (this.needLayout) {
      this.needLayout = false;
      if (this.active === null) {
        if (this.layout) { this.layout = null; this.layoutKey = ''; this.emit('layout', { switched }); }
        return;
      }
      try {
        const lay = (await engine.call<LayoutInfo>('layout')).result;
        const key = JSON.stringify([lay.slots, lay.pages, lay.editingParents]);
        if (key !== this.layoutKey || switched) {
          this.layoutKey = key;
          this.layout = lay;
          this.emit('layout', { switched });
        }
      } catch { }
    }
  }

  onExec: ((command: string, params: Record<string, unknown>) => void) | null = null;

  async exec<T = any>(command: string, params: Record<string, unknown> = {}, quiet = false): Promise<T | undefined> {
    try {
      const r = await engine.call<T>('exec', { command, params });
      this.handleRequests(r.meta?.requests);
      this.schedule();
      this.onExec?.(command, params);
      return r.result;
    } catch (err) {
      if (!quiet) toast(friendly((err as Error).message), 'error');
      return undefined;
    }
  }

  async batch(commands: Array<{ command: string; params?: Record<string, unknown> }>, quiet = false): Promise<any> {
    try {
      const r = await engine.call('batch', { commands });
      this.schedule();
      return r.result;
    } catch (err) {
      if (!quiet) toast(friendly((err as Error).message), 'error');
      return undefined;
    }
  }

  handleRequests(reqs: UiRequest[] | undefined): void {
    for (const r of reqs ?? []) this.emit('request', r);
  }

  inspect(): Promise<any> {
    if (!this.inspectCache || this.inspectCache.rev !== engine.rev) {
      this.inspectCache = { rev: engine.rev, data: engine.call('inspect').then((r) => r.result).catch(() => null) };
    }
    return this.inspectCache.data;
  }

  async setTool(id: string): Promise<void> {
    try { await engine.call('tool', { id }); } catch { }
    this.schedule();
  }

  async activate(index: number): Promise<void> {
    try { await engine.call('docs.activate', { index }); } catch (err) { toast(friendly((err as Error).message), 'error'); }
    this.schedule();
  }

  pageUrl(page: number, maxSide: number): string {
    return `/api/core/page.png?page=${page}&maxSide=${Math.round(maxSide)}&r=${engine.rev}`;
  }
}

export function friendly(msg: string): string {
  return msg.replace(/^the engine refused:?\s*/i, '').replace(/\s*\(nothing was changed: the batch was rolled back\)/, '');
}

export const editor = new Editor();
