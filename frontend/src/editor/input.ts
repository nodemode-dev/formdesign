import { engine } from '../engine/client';
import { anyDialogOpen } from '../ui/dialogs';
import { isMac } from '../ui/dom';
import { editor, friendly } from './editor';
import type { Viewport } from './viewport';
import { toast } from '../ui/toast';

interface Mods { shift: boolean; alt: boolean; cmd: boolean; ctrl: boolean; space: boolean }
interface PEvent { kind: 'down' | 'drag' | 'up' | 'move' | 'doubleclick'; x: number; y: number; mods: Mods }

const CURSORS: Record<string, string> = {
  arrow: 'default', arrowhollow: 'default', move: 'move', crosshair: 'crosshair', resizeh: 'ew-resize', resizev: 'ns-resize',
  resizenwse: 'nwse-resize', resizenesw: 'nesw-resize', rotate: 'alias', pen: 'crosshair', text: 'text', hand: 'grab', handgrab: 'grabbing',
  zoomin: 'zoom-in', zoomout: 'zoom-out', eyedropper: 'crosshair', loadedtext: 'copy', loadedgraphic: 'copy', notallowed: 'not-allowed',
};

export class Input {
  private queue: PEvent[] = [];
  private sending = false;
  private down = false;
  private panning: { x: number; y: number } | null = null;
  private space = false;
  private lastMove = 0;
  private hoverQueued: PEvent | null = null;
  private zoomTool = false;
  onKeyCommand: ((e: KeyboardEvent) => boolean) | null = null;

  constructor(private readonly vp: Viewport, private readonly target: HTMLElement) {
    target.addEventListener('pointerdown', (e) => this.onDown(e));
    target.addEventListener('pointermove', (e) => this.onMove(e));
    target.addEventListener('pointerup', (e) => this.onUp(e));
    target.addEventListener('pointercancel', (e) => this.onUp(e));
    target.addEventListener('dblclick', (e) => this.send({ kind: 'doubleclick', ...this.at(e), mods: this.mods(e) }));
    target.addEventListener('wheel', (e) => this.onWheel(e), { passive: false });
    target.addEventListener('contextmenu', (e) => e.preventDefault());
    window.addEventListener('keydown', (e) => this.onKey(e));
    window.addEventListener('keyup', (e) => { if (e.code === 'Space') { this.space = false; this.setCursor(this.down ? 'grabbing' : ''); } });
    window.addEventListener('blur', () => { this.space = false; });
  }

  private mods(e: { shiftKey: boolean; altKey: boolean; metaKey: boolean; ctrlKey: boolean }): Mods {
    return { shift: e.shiftKey, alt: e.altKey, cmd: isMac ? e.metaKey : e.ctrlKey, ctrl: isMac ? e.ctrlKey : false, space: this.space };
  }

  private at(e: { clientX: number; clientY: number }): { x: number; y: number } {
    const r = this.target.getBoundingClientRect();
    const [x, y] = this.vp.toCanvas(e.clientX - r.left, e.clientY - r.top);
    return { x, y };
  }

  private setCursor(c: string): void {
    if (this.target.style.cursor !== c) this.target.style.cursor = c;
  }

  private onDown(e: PointerEvent): void {
    if (!editor.hasDoc) return;
    (document.activeElement as HTMLElement | null)?.blur?.();
    this.target.setPointerCapture(e.pointerId);
    if (e.button === 1 || this.space || editor.tool === 'hand') {
      this.panning = { x: e.clientX, y: e.clientY };
      this.setCursor('grabbing');
      e.preventDefault();
      return;
    }
    if (e.button !== 0) return;
    if (editor.tool === 'zoom') {
      const r = this.target.getBoundingClientRect();
      this.vp.zoomAt(e.altKey ? 0.5 : 2, e.clientX - r.left, e.clientY - r.top);
      this.zoomTool = true;
      return;
    }
    this.down = true;
    this.send({ kind: 'down', ...this.at(e), mods: this.mods(e) });
  }

  private onMove(e: PointerEvent): void {
    if (this.panning) {
      this.vp.panBy(e.clientX - this.panning.x, e.clientY - this.panning.y);
      this.panning = { x: e.clientX, y: e.clientY };
      return;
    }
    if (!editor.hasDoc) return;
    if (this.down) {
      const evs = typeof e.getCoalescedEvents === 'function' ? e.getCoalescedEvents() : [e];
      const last = evs[evs.length - 1] ?? e;
      this.send({ kind: 'drag', ...this.at(last), mods: this.mods(e) });
      return;
    }
    if (this.space) { this.setCursor('grab'); return; }
    this.hoverQueued = { kind: 'move', ...this.at(e), mods: this.mods(e) };
    const now = performance.now();
    if (now - this.lastMove > 32) { this.lastMove = now; this.flushHover(); }
    else window.setTimeout(() => this.flushHover(), 34);
  }

  private flushHover(): void {
    const ev = this.hoverQueued;
    if (!ev || this.down || this.sending) return;
    this.hoverQueued = null;
    this.send(ev);
  }

  private onUp(e: PointerEvent): void {
    if (this.panning) { this.panning = null; this.setCursor(this.space ? 'grab' : ''); return; }
    if (this.zoomTool) { this.zoomTool = false; return; }
    if (!this.down) return;
    this.down = false;
    this.send({ kind: 'up', ...this.at(e), mods: this.mods(e) });
  }

  private send(ev: PEvent): void {
    if (ev.kind === 'drag' && this.queue.length && this.queue[this.queue.length - 1].kind === 'drag') this.queue[this.queue.length - 1] = ev;
    else this.queue.push(ev);
    void this.flush();
  }

  private async flush(): Promise<void> {
    if (this.sending || !this.queue.length) return;
    this.sending = true;
    const events = this.queue.splice(0);
    try {
      const r = await engine.call<{ cursor?: string; wantsText?: boolean; tool?: string }>('pointer', { events, zoom: this.vp.view.zoom });
      editor.handleRequests(r.meta?.requests);
      const c = r.result?.cursor;
      if (c && !this.panning) this.setCursor(CURSORS[String(c).toLowerCase()] ?? 'default');
      if (r.result?.tool && r.result.tool !== editor.tool) editor.schedule();
      if (events.some((x) => x.kind !== 'move')) editor.schedule();
      this.vp.refreshOverlays();
    } catch (err) {
      if (events.some((x) => x.kind === 'up' || x.kind === 'down')) toast(friendly((err as Error).message), 'error');
    } finally {
      this.sending = false;
    }
    if (this.queue.length) void this.flush();
    else if (this.hoverQueued) this.flushHover();
  }

  private onWheel(e: WheelEvent): void {
    if (!editor.hasDoc) return;
    e.preventDefault();
    const r = this.target.getBoundingClientRect();
    const scale = e.deltaMode === 1 ? 16 : e.deltaMode === 2 ? 400 : 1;
    if (e.ctrlKey || e.metaKey || e.altKey) {
      const f = Math.exp(-e.deltaY * scale * (e.ctrlKey && !e.metaKey && Math.abs(e.deltaY) < 50 ? 0.01 : 0.0022));
      this.vp.zoomAt(f, e.clientX - r.left, e.clientY - r.top);
    } else {
      const dx = e.shiftKey && !e.deltaX ? e.deltaY : e.deltaX;
      const dy = e.shiftKey && !e.deltaX ? 0 : e.deltaY;
      this.vp.panBy(-dx * scale, -dy * scale);
    }
  }

  private typingTarget(e: KeyboardEvent): boolean {
    const t = e.target as HTMLElement | null;
    return !!t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT' || t.isContentEditable);
  }

  private onKey(e: KeyboardEvent): void {
    if (anyDialogOpen() || this.typingTarget(e)) return;
    if (e.code === 'Space' && !editor.wantsText) {
      if (!this.space) { this.space = true; this.setCursor('grab'); }
      e.preventDefault();
      return;
    }
    if (this.onKeyCommand?.(e)) { e.preventDefault(); return; }
    if (!editor.hasDoc) return;
    if (e.isComposing || ['Shift', 'Control', 'Alt', 'Meta', 'CapsLock'].includes(e.key)) return;
    const mods = this.mods(e);
    const typing = editor.wantsText && !mods.cmd && !mods.ctrl;
    const key = e.key === ' ' ? 'space' : e.key;
    if (!typing && key.length === 1 && !mods.cmd && !mods.ctrl && !mods.alt && !editor.tools.some((g) => g.some((t) => t.shortcut && t.shortcut.toLowerCase().endsWith(key.toLowerCase())))) return;
    e.preventDefault();
    const params: Record<string, unknown> = { key, mods, zoom: this.vp.view.zoom };
    if (typing && e.key.length === 1) params.text = e.key;
    void engine.call('key', params).then((r) => { editor.handleRequests(r.meta?.requests); editor.schedule(); this.vp.refreshOverlays(); })
      .catch((err) => toast(friendly((err as Error).message), 'error'));
  }

  paste(text: string): void {
    if (!editor.wantsText || !text) return;
    void engine.call('text', { text }).then(() => editor.schedule()).catch((err) => toast(friendly((err as Error).message), 'error'));
  }
}
