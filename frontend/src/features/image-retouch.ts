import { editor } from '../editor/editor';
import { Dialog } from '../ui/dialogs';
import { el, q, setClass, setDisabled, setText } from '../ui/dom';
import { t } from '../ui/i18n';
import { icon } from '../ui/icons';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

type Tool = 'heal' | 'clone' | 'redEye';
interface Stroke { tool: Tool; points: Array<[number, number]>; radius: number; offset?: [number, number] }

/** Object > Image > Retouch Image: Affinity's Healing Brush, Clone Brush and Red Eye tool over a placed picture.
 *  The picture shown is the engine's own render of the strokes so far (image.retouchPreview), so the dialog never
 *  approximates the result; while painting, only the overlay canvas draws the brush and the stroke's path. */
class Retouch {
  private readonly d = new Dialog(t('Retouch Image'), 800);
  private readonly pic = el<HTMLCanvasElement>('<canvas class="rt-pic" width="560" height="380"></canvas>');
  private readonly over = el<HTMLCanvasElement>('<canvas class="rt-over" width="560" height="380"></canvas>');
  private readonly size: HTMLInputElement;
  private readonly sizeVal: HTMLElement;
  private readonly count: HTMLElement;
  private readonly note: HTMLElement;
  private readonly undoBtn: HTMLButtonElement;
  private readonly clearBtn: HTMLButtonElement;
  private readonly toolBtns = new Map<Tool, HTMLButtonElement>();
  private tool: Tool = 'heal';
  private radius = 0.03;
  private strokes: Stroke[] = [];
  private live: Stroke | null = null;
  private source: [number, number] | null = null;
  private offset: [number, number] | null = null;
  private hover: [number, number] | null = null;
  private frame = 0;
  private box = { x: 0, y: 0, w: 1, h: 1 };
  private shown: ImageBitmap | null = null;
  private ticket = 0;
  private raf = 0;

  constructor() {
    const wrap = el(`<div class="rt-wrap">
      <div class="rt-stage"></div>
      <div class="rt-side">
        <div class="rt-tools" role="group"></div>
        <label class="rt-row"><span class="rt-name">${t('Size')}</span><input type="range" min="0.005" max="0.2" step="0.005"><span class="rt-val" data-no-i18n></span></label>
        <p class="cv-note rt-note"></p>
        <div class="rt-hist"><span class="rt-count" data-no-i18n></span>
          <button class="cv-btn cv-btn-ghost rt-undo" type="button">${t('Undo Stroke')}</button>
          <button class="cv-btn cv-btn-ghost rt-clear" type="button">${t('Clear')}</button></div>
      </div></div>`);
    q(wrap, '.rt-stage').append(this.pic, this.over);
    const tools: Array<[Tool, string, string]> = [['heal', 'tHealBrush', t('Healing Brush')], ['clone', 'tStamp', t('Clone Brush')], ['redEye', 'tRedEye', t('Red Eye')]];
    for (const [id, ic, label] of tools) {
      const b = el<HTMLButtonElement>(`<button class="cv-btn cv-btn-ghost rt-tool" type="button">${icon(ic, 16)}<span class="rt-tool-name"></span></button>`);
      setText(q(b, '.rt-tool-name'), label);
      b.addEventListener('click', () => this.pick(id));
      this.toolBtns.set(id, b);
      q(wrap, '.rt-tools').appendChild(b);
    }
    this.size = q<HTMLInputElement>(wrap, 'input');
    this.sizeVal = q(wrap, '.rt-val');
    this.count = q(wrap, '.rt-count');
    this.note = q(wrap, '.rt-note');
    this.undoBtn = q<HTMLButtonElement>(wrap, '.rt-undo');
    this.clearBtn = q<HTMLButtonElement>(wrap, '.rt-clear');
    this.size.addEventListener('input', () => { this.radius = Number(this.size.value); this.readouts(); this.drawOver(); });
    this.undoBtn.addEventListener('click', () => { this.strokes.pop(); this.readouts(); void this.refresh(); });
    this.clearBtn.addEventListener('click', () => { this.strokes = []; this.readouts(); void this.refresh(); });
    this.d.body.appendChild(wrap);
    this.d.button(t('Cancel'), 'ghost', () => this.d.close());
    this.d.button(t('OK'), 'primary', () => void this.apply());
    this.over.addEventListener('pointerdown', (ev) => this.down(ev));
    this.over.addEventListener('pointermove', (ev) => this.move(ev));
    this.over.addEventListener('pointerup', () => this.up());
    this.over.addEventListener('pointerleave', () => { this.hover = null; this.drawOver(); });
  }

  private pick(tool: Tool): void {
    this.tool = tool;
    for (const [id, b] of this.toolBtns) {
      setClass(b, 'is-on', id === tool);
      if (b.getAttribute('aria-pressed') !== String(id === tool)) b.setAttribute('aria-pressed', String(id === tool));
    }
    this.readouts();
    this.drawOver();
  }

  private readouts(): void {
    if (Number(this.size.value) !== this.radius) this.size.value = String(this.radius);
    setText(this.sizeVal, `${(this.radius * 100).toFixed(1)}%`);
    setText(this.count, this.strokes.length === 1 ? t('1 stroke') : t('{n} strokes').replace('{n}', String(this.strokes.length)));
    setText(this.note, this.tool === 'redEye' ? t('Click each red pupil, with the brush a little larger than the pupil.')
      : this.source ? t('Paint over the flaw. Alt-click to take the source from somewhere else.') : t('Alt-click a clean area to set the source, then paint over the flaw.'));
    setDisabled(this.undoBtn, this.strokes.length === 0);
    setDisabled(this.clearBtn, this.strokes.length === 0);
  }

  async open(): Promise<void> {
    const id = editor.selection?.items?.[0];
    if (id === undefined) { toast(t('Select a frame with a placed picture.'), 'error'); return; }
    this.frame = id;
    this.strokes = []; this.source = null; this.offset = null; this.live = null; this.shown = null;
    if (!(await this.refresh(true))) return;
    this.pick(this.tool);
    this.d.open();
  }

  /** Asks the engine for the picture with the strokes so far; a later request wins over an earlier one. */
  private async refresh(first = false): Promise<boolean> {
    const mine = ++this.ticket;
    const r = await editor.exec<{ base64: string; mime: string }>('image.retouchPreview', { ids: [this.frame], strokes: this.strokes, maxSide: 720 }, !first);
    if (!r || mine !== this.ticket) return false;
    const bytes = Uint8Array.from(atob(r.base64), (ch) => ch.charCodeAt(0));
    const bmp = await createImageBitmap(new Blob([bytes], { type: r.mime }));
    if (mine !== this.ticket) return false;
    this.shown?.close();
    this.shown = bmp;
    this.drawPic();
    return true;
  }

  private drawPic(): void {
    const bmp = this.shown;
    if (!bmp) return;
    const s = Math.min(this.pic.width / bmp.width, this.pic.height / bmp.height);
    this.box = { x: (this.pic.width - bmp.width * s) / 2, y: (this.pic.height - bmp.height * s) / 2, w: bmp.width * s, h: bmp.height * s };
    const c = this.pic.getContext('2d') as CanvasRenderingContext2D;
    c.clearRect(0, 0, this.pic.width, this.pic.height);
    c.drawImage(bmp, this.box.x, this.box.y, this.box.w, this.box.h);
    this.drawOver();
  }

  private toCanvas(p: [number, number]): [number, number] { return [this.box.x + p[0] * this.box.w, this.box.y + p[1] * this.box.h]; }

  /** The brush ring, the source crosshair and the stroke being painted, on the overlay only. */
  private drawOver(): void {
    cancelAnimationFrame(this.raf);
    this.raf = requestAnimationFrame(() => {
      const c = this.over.getContext('2d') as CanvasRenderingContext2D;
      c.clearRect(0, 0, this.over.width, this.over.height);
      const rpx = this.radius * Math.max(this.box.w, this.box.h);
      c.lineWidth = 1;
      if (this.live) {
        c.strokeStyle = 'rgba(79, 153, 255, 0.35)'; c.lineCap = 'round'; c.lineJoin = 'round'; c.lineWidth = rpx * 2;
        c.beginPath();
        this.live.points.forEach((p, i) => { const [x, y] = this.toCanvas(p); if (i) c.lineTo(x, y); else c.moveTo(x, y); });
        if (this.live.points.length === 1) { const [x, y] = this.toCanvas(this.live.points[0]); c.lineTo(x + 0.01, y); }
        c.stroke(); c.lineWidth = 1;
      }
      const src = this.tool === 'redEye' ? null : this.hover && this.offset ? [this.hover[0] + this.offset[0], this.hover[1] + this.offset[1]] as [number, number] : this.source;
      if (src) {
        const [x, y] = this.toCanvas(src);
        c.strokeStyle = '#ffffff'; c.beginPath(); c.moveTo(x - 7, y); c.lineTo(x + 7, y); c.moveTo(x, y - 7); c.lineTo(x, y + 7); c.stroke();
        c.strokeStyle = 'rgba(0, 0, 0, 0.6)'; c.beginPath(); c.arc(x, y, rpx, 0, Math.PI * 2); c.stroke();
      }
      if (this.hover) {
        const [x, y] = this.toCanvas(this.hover);
        c.strokeStyle = '#ffffff'; c.beginPath(); c.arc(x, y, rpx, 0, Math.PI * 2); c.stroke();
        c.strokeStyle = 'rgba(0, 0, 0, 0.5)'; c.beginPath(); c.arc(x, y, rpx + 1, 0, Math.PI * 2); c.stroke();
      }
    });
  }

  private share(ev: PointerEvent): [number, number] {
    const r = this.over.getBoundingClientRect();
    const px = ((ev.clientX - r.left) / r.width) * this.over.width, py = ((ev.clientY - r.top) / r.height) * this.over.height;
    return [(px - this.box.x) / this.box.w, (py - this.box.y) / this.box.h];
  }

  private down(ev: PointerEvent): void {
    const p = this.share(ev);
    if (p[0] < 0 || p[1] < 0 || p[0] > 1 || p[1] > 1) return;
    if (this.tool !== 'redEye' && ev.altKey) { this.source = p; this.offset = null; this.readouts(); this.drawOver(); return; }
    if (this.tool !== 'redEye' && !this.source) { setClass(this.note, 'is-warn', true); return; }
    setClass(this.note, 'is-warn', false);
    // Aligned, as Affinity and Photoshop default to: the source keeps its distance from the brush across strokes.
    if (this.tool !== 'redEye' && !this.offset && this.source) {
      // Painting on the source itself would copy pixels onto themselves; the engine refuses a zero offset.
      if (Math.hypot(this.source[0] - p[0], this.source[1] - p[1]) < this.radius * 0.5) { setClass(this.note, 'is-warn', true); return; }
      this.offset = [this.source[0] - p[0], this.source[1] - p[1]];
    }
    this.over.setPointerCapture(ev.pointerId);
    this.live = { tool: this.tool, points: [p], radius: this.radius, ...(this.tool === 'redEye' ? {} : { offset: this.offset ?? [0, 0] }) };
    this.drawOver();
  }

  private move(ev: PointerEvent): void {
    this.hover = this.share(ev);
    if (this.live && this.live.tool !== 'redEye') {
      const last = this.live.points[this.live.points.length - 1];
      const step = this.radius * 0.25;
      if (Math.hypot(this.hover[0] - last[0], this.hover[1] - last[1]) >= step) this.live.points.push([Math.round(this.hover[0] * 1e5) / 1e5, Math.round(this.hover[1] * 1e5) / 1e5]);
    }
    this.drawOver();
  }

  private up(): void {
    if (!this.live) return;
    this.strokes.push(this.live);
    this.live = null;
    this.readouts();
    void this.refresh();
  }

  private async apply(): Promise<void> {
    if (!this.strokes.length) { this.d.close(); return; }
    const r = await editor.exec('image.retouch', { ids: [this.frame], strokes: this.strokes });
    if (r !== undefined) this.d.close();
  }
}

export function installImageRetouch(ws: Workspace): void {
  let dlg: Retouch | null = null;
  ws.override('image.retouch', () => { void (dlg ??= new Retouch()).open(); });
}
