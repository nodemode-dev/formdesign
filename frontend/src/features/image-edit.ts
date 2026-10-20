import { editor } from '../editor/editor';
import { Dialog } from '../ui/dialogs';
import { el, q, setText } from '../ui/dom';
import { t } from '../ui/i18n';
import { icon } from '../ui/icons';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Item { id: number; asset?: number; children?: Item[] }

/** The edit, as the engine's image.edit takes it. Defaults mean "no change". */
interface Edit { crop: [number, number, number, number]; rotate: number; flipH: boolean; flipV: boolean; exposure: number; contrast: number; saturation: number; temperature: number; levels: [number, number, number] }
const fresh = (): Edit => ({ crop: [0, 0, 1, 1], rotate: 0, flipH: false, flipV: false, exposure: 0, contrast: 0, saturation: 0, temperature: 0, levels: [0, 255, 1] });

/** The same tone curve as formdesign-render's pixedit::tone, so the preview shows what the engine will write. */
function tone(v: number, e: Edit): number {
  const [black, white, gamma] = e.levels;
  if (black !== 0 || white !== 255 || gamma !== 1) {
    const b = black / 255, w = Math.max(white / 255, b + 1e-3);
    v = Math.pow(Math.min(1, Math.max(0, (v - b) / (w - b))), 1 / Math.min(10, Math.max(0.1, gamma)));
  }
  if (e.exposure) v *= Math.pow(2, Math.max(-3, Math.min(3, e.exposure)));
  if (e.contrast) {
    const c = Math.max(-1, Math.min(1, e.contrast / 100));
    v = (v - 0.5) * (c >= 0 ? 1 / (1 - c * 0.95) : 1 + c) + 0.5;
  }
  return v;
}

function adjust(src: ImageData, e: Edit): ImageData {
  const out = new ImageData(new Uint8ClampedArray(src.data), src.width, src.height);
  const d = out.data, sat = 1 + Math.max(-1, Math.min(1, e.saturation / 100)), warm = Math.max(-1, Math.min(1, e.temperature / 100)) * 0.12;
  for (let i = 0; i < d.length; i += 4) {
    let r = tone(d[i] / 255, e), g = tone(d[i + 1] / 255, e), b = tone(d[i + 2] / 255, e);
    if (sat !== 1) { const l = 0.2126 * r + 0.7152 * g + 0.0722 * b; r = l + (r - l) * sat; g = l + (g - l) * sat; b = l + (b - l) * sat; }
    r += warm; b -= warm;
    d[i] = r * 255; d[i + 1] = g * 255; d[i + 2] = b * 255;
  }
  return out;
}

function find(list: Item[], id: number): Item | null {
  for (const it of list) {
    if (it.id === id) return it;
    const hit = it.children ? find(it.children, id) : null;
    if (hit) return hit;
  }
  return null;
}

class EditImage {
  private readonly d = new Dialog(t('Edit Image'), 760);
  private readonly pic = el<HTMLCanvasElement>('<canvas class="ie-pic" width="440" height="300"></canvas>');
  private readonly res = el<HTMLCanvasElement>('<canvas class="ie-res" width="220" height="150"></canvas>');
  private readonly ranges = new Map<string, { input: HTMLInputElement; val: HTMLElement }>();
  private base: ImageData | null = null;
  private toned: ImageData | null = null;
  private e: Edit = fresh();
  private frame = 0;
  private drag: { x: number; y: number } | null = null;
  private box = { x: 0, y: 0, w: 1, h: 1 };
  private raf = 0;

  constructor() {
    const wrap = el(`<div class="ie-wrap">
      <div class="ie-left"><div class="ie-stage"></div><p class="cv-note ie-hint">${t('Drag on the picture to crop.')}</p></div>
      <div class="ie-right"><div class="ie-result"></div>
        <div class="ie-turns">
          <button class="cv-icon-btn ie-rl" type="button" title="${t('Rotate Left')}" aria-label="${t('Rotate Left')}">${icon('rotate', 16)}</button>
          <button class="cv-icon-btn ie-rr" type="button" title="${t('Rotate Right')}" aria-label="${t('Rotate Right')}">${icon('rotate', 16)}</button>
          <button class="cv-icon-btn ie-fh" type="button" title="${t('Flip Horizontal')}" aria-label="${t('Flip Horizontal')}">${icon('flipH', 16)}</button>
          <button class="cv-icon-btn ie-fv" type="button" title="${t('Flip Vertical')}" aria-label="${t('Flip Vertical')}">${icon('flipV', 16)}</button>
          <button class="cv-btn cv-btn-ghost ie-reset" type="button">${t('Reset')}</button>
        </div>
        <div class="ie-sliders"></div>
      </div></div>`);
    q(wrap, '.ie-stage').appendChild(this.pic);
    q(wrap, '.ie-result').appendChild(this.res);
    q<HTMLElement>(wrap, '.ie-rl').style.transform = 'scaleX(-1)';
    q(wrap, '.ie-rl').addEventListener('click', () => { this.e.rotate = (this.e.rotate + 270) % 360; this.paint(); });
    q(wrap, '.ie-rr').addEventListener('click', () => { this.e.rotate = (this.e.rotate + 90) % 360; this.paint(); });
    q(wrap, '.ie-fh').addEventListener('click', () => { this.e.flipH = !this.e.flipH; this.paint(); });
    q(wrap, '.ie-fv').addEventListener('click', () => { this.e.flipV = !this.e.flipV; this.paint(); });
    q(wrap, '.ie-reset').addEventListener('click', () => { this.e = fresh(); this.sync(); this.retone(); });
    const sliders = q(wrap, '.ie-sliders');
    const add = (key: string, label: string, min: number, max: number, step: number) => {
      const row = el(`<label class="ie-row"><span class="ie-name"></span><input type="range"><span class="ie-val" data-no-i18n></span></label>`);
      setText(q(row, '.ie-name'), label);
      const input = q<HTMLInputElement>(row, 'input');
      Object.assign(input, { min: String(min), max: String(max), step: String(step) });
      input.addEventListener('input', () => { this.set(key, Number(input.value)); this.retone(); });
      this.ranges.set(key, { input, val: q(row, '.ie-val') });
      sliders.appendChild(row);
    };
    add('exposure', t('Exposure'), -3, 3, 0.05);
    add('contrast', t('Contrast'), -100, 100, 1);
    add('saturation', t('Saturation'), -100, 100, 1);
    add('temperature', t('Temperature'), -100, 100, 1);
    add('black', t('Black'), 0, 254, 1);
    add('white', t('White'), 1, 255, 1);
    add('gamma', t('Gamma'), 0.1, 3, 0.01);
    this.d.body.appendChild(wrap);
    this.d.button(t('Cancel'), 'ghost', () => this.d.close());
    this.d.button(t('OK'), 'primary', () => void this.apply());
    this.pic.addEventListener('pointerdown', (ev) => this.down(ev));
    this.pic.addEventListener('pointermove', (ev) => this.move(ev));
    this.pic.addEventListener('pointerup', () => { this.drag = null; });
  }

  private set(key: string, v: number): void {
    if (key === 'black') this.e.levels = [Math.min(v, this.e.levels[1] - 1), this.e.levels[1], this.e.levels[2]];
    else if (key === 'white') this.e.levels = [this.e.levels[0], Math.max(v, this.e.levels[0] + 1), this.e.levels[2]];
    else if (key === 'gamma') this.e.levels = [this.e.levels[0], this.e.levels[1], v];
    else (this.e as unknown as Record<string, number>)[key] = v;
    this.sync();
  }

  /** Sliders and their readouts from the edit, written only where they differ. */
  private sync(): void {
    const val: Record<string, number> = { exposure: this.e.exposure, contrast: this.e.contrast, saturation: this.e.saturation, temperature: this.e.temperature, black: this.e.levels[0], white: this.e.levels[1], gamma: this.e.levels[2] };
    for (const [k, r] of this.ranges) {
      if (Number(r.input.value) !== val[k]) r.input.value = String(val[k]);
      setText(r.val, k === 'exposure' ? val[k].toFixed(2) : k === 'gamma' ? val[k].toFixed(2) : String(Math.round(val[k])));
    }
  }

  async open(): Promise<void> {
    const id = editor.selection?.items?.[0];
    const data = id === undefined ? null : await editor.inspect();
    let it: Item | null = null;
    for (const s of (data?.spreads ?? []) as Array<{ items: Item[] }>) { it = id === undefined ? null : find(s.items, id); if (it) break; }
    if (!it?.asset) { toast(t('Select a frame with a placed picture.'), 'error'); return; }
    let bmp: ImageBitmap;
    try {
      const r = await fetch(`/api/core/asset?asset=${it.asset}`);
      if (!r.ok) throw new Error(await r.text());
      bmp = await createImageBitmap(await r.blob());
    } catch { toast(t('This picture cannot be edited here: use Edit Original.'), 'error'); return; }
    const k = Math.min(1, 480 / Math.max(bmp.width, bmp.height));
    const off = new OffscreenCanvas(Math.max(1, Math.round(bmp.width * k)), Math.max(1, Math.round(bmp.height * k)));
    const ctx = off.getContext('2d') as OffscreenCanvasRenderingContext2D;
    ctx.drawImage(bmp, 0, 0, off.width, off.height);
    this.base = ctx.getImageData(0, 0, off.width, off.height);
    this.frame = it.id;
    this.e = fresh();
    this.sync();
    this.retone();
    this.d.open();
  }

  private retone(): void {
    if (!this.base) return;
    cancelAnimationFrame(this.raf);
    this.raf = requestAnimationFrame(() => { this.toned = adjust(this.base as ImageData, this.e); this.paint(); });
  }

  /** The picture with the crop drawn over it, and the result: cropped, turned and flipped. */
  private paint(): void {
    const src = this.toned;
    if (!src) return;
    const tmp = new OffscreenCanvas(src.width, src.height);
    (tmp.getContext('2d') as OffscreenCanvasRenderingContext2D).putImageData(src, 0, 0);
    const pc = this.pic.getContext('2d') as CanvasRenderingContext2D;
    const s = Math.min(this.pic.width / src.width, this.pic.height / src.height);
    this.box = { x: (this.pic.width - src.width * s) / 2, y: (this.pic.height - src.height * s) / 2, w: src.width * s, h: src.height * s };
    pc.clearRect(0, 0, this.pic.width, this.pic.height);
    pc.drawImage(tmp, this.box.x, this.box.y, this.box.w, this.box.h);
    const [x0, y0, x1, y1] = this.e.crop;
    const cx = this.box.x + x0 * this.box.w, cy = this.box.y + y0 * this.box.h, cw = (x1 - x0) * this.box.w, ch = (y1 - y0) * this.box.h;
    pc.fillStyle = 'rgba(0, 0, 0, 0.55)';
    pc.beginPath(); pc.rect(this.box.x, this.box.y, this.box.w, this.box.h); pc.rect(cx, cy, cw, ch); pc.fill('evenodd');
    pc.strokeStyle = '#4f99ff'; pc.lineWidth = 1; pc.strokeRect(Math.round(cx) + 0.5, Math.round(cy) + 0.5, Math.round(cw), Math.round(ch));
    const sw = Math.max(1, (x1 - x0) * src.width), sh = Math.max(1, (y1 - y0) * src.height);
    const turned = this.e.rotate % 180 !== 0;
    const ow = turned ? sh : sw, oh = turned ? sw : sh;
    const rs = Math.min(this.res.width / ow, this.res.height / oh);
    const rc = this.res.getContext('2d') as CanvasRenderingContext2D;
    rc.setTransform(1, 0, 0, 1, 0, 0);
    rc.clearRect(0, 0, this.res.width, this.res.height);
    rc.translate(this.res.width / 2, this.res.height / 2);
    rc.rotate((this.e.rotate * Math.PI) / 180);
    rc.scale(this.e.flipH ? -1 : 1, this.e.flipV ? -1 : 1);
    if (turned && (this.e.flipH !== this.e.flipV)) rc.scale(-1, -1);
    rc.drawImage(tmp, x0 * src.width, y0 * src.height, sw, sh, (-sw * rs) / 2, (-sh * rs) / 2, sw * rs, sh * rs);
    rc.setTransform(1, 0, 0, 1, 0, 0);
  }

  private share(ev: PointerEvent): { x: number; y: number } {
    const r = this.pic.getBoundingClientRect();
    const px = ((ev.clientX - r.left) / r.width) * this.pic.width, py = ((ev.clientY - r.top) / r.height) * this.pic.height;
    return { x: Math.min(1, Math.max(0, (px - this.box.x) / this.box.w)), y: Math.min(1, Math.max(0, (py - this.box.y) / this.box.h)) };
  }

  private down(ev: PointerEvent): void { this.pic.setPointerCapture(ev.pointerId); this.drag = this.share(ev); }

  private move(ev: PointerEvent): void {
    if (!this.drag) return;
    const p = this.share(ev);
    if (Math.abs(p.x - this.drag.x) < 0.01 || Math.abs(p.y - this.drag.y) < 0.01) return;
    this.e.crop = [Math.min(p.x, this.drag.x), Math.min(p.y, this.drag.y), Math.max(p.x, this.drag.x), Math.max(p.y, this.drag.y)];
    this.paint();
  }

  private async apply(): Promise<void> {
    const e = this.e, p: Record<string, unknown> = { ids: [this.frame] };
    const [x0, y0, x1, y1] = e.crop;
    if (x0 > 0 || y0 > 0 || x1 < 1 || y1 < 1) p.crop = e.crop.map((v) => Math.round(v * 10000) / 10000);
    if (e.rotate) p.rotate = e.rotate;
    if (e.flipH) p.flipH = true;
    if (e.flipV) p.flipV = true;
    for (const k of ['exposure', 'contrast', 'saturation', 'temperature'] as const) if (e[k]) p[k] = e[k];
    if (e.levels[0] !== 0 || e.levels[1] !== 255 || e.levels[2] !== 1) p.levels = e.levels;
    if (Object.keys(p).length === 1) { this.d.close(); return; }
    const r = await editor.exec('image.edit', p);
    if (r !== undefined) this.d.close();
  }
}

export function installImageEdit(ws: Workspace): void {
  let dlg: EditImage | null = null;
  ws.override('image.edit', () => { void (dlg ??= new EditImage()).open(); });
}
