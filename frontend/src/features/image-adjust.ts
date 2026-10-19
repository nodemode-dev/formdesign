import { editor } from '../editor/editor';
import { Dialog } from '../ui/dialogs';
import { el, q, setDisabled, setText } from '../ui/dom';
import { t } from '../ui/i18n';
import type { Workspace } from '../ui/workspace';

type Pt = [number, number];
interface Adj { exposure: number; contrast: number; saturation: number; temperature: number; hue: number; lightness: number; black: number; white: number; gamma: number; blackWhite: boolean; curve: Pt[] }
const fresh = (): Adj => ({ exposure: 0, contrast: 0, saturation: 0, temperature: 0, hue: 0, lightness: 0, black: 0, white: 255, gamma: 1, blackWhite: false, curve: [[0, 0], [255, 255]] });
// Labels are literal t() calls, because the key extractor only sees literals.
const SLIDERS: Array<[keyof Adj, () => string, number, number, number]> = [
  ['exposure', () => t('Exposure'), -3, 3, 0.05], ['contrast', () => t('Contrast'), -100, 100, 1], ['saturation', () => t('Saturation'), -100, 100, 1],
  ['temperature', () => t('Temperature'), -100, 100, 1], ['hue', () => t('Hue'), -180, 180, 1], ['lightness', () => t('Lightness'), -100, 100, 1],
  ['black', () => t('Black'), 0, 254, 1], ['white', () => t('White'), 1, 255, 1], ['gamma', () => t('Gamma'), 0.1, 3, 0.01],
];

/** The curve's shape as the engine draws it (pixedit::curve_lut, monotone cubic), for the editor only. */
function curveAt(pts: Pt[], x: number): number {
  const p = pts.slice().sort((a, b) => a[0] - b[0]);
  const n = p.length;
  if (x <= p[0][0]) return p[0][1];
  if (x >= p[n - 1][0]) return p[n - 1][1];
  const d = p.slice(0, -1).map((a, i) => (p[i + 1][1] - a[1]) / (p[i + 1][0] - a[0]));
  const m = p.map((_, i) => (i === 0 ? d[0] : i === n - 1 ? d[n - 2] : d[i - 1] * d[i] <= 0 ? 0 : (d[i - 1] + d[i]) / 2));
  for (let i = 0; i < n - 1; i++) {
    if (d[i] === 0) { m[i] = 0; m[i + 1] = 0; continue; }
    const a = m[i] / d[i], b = m[i + 1] / d[i], s = a * a + b * b;
    if (s > 9) { const k = 3 / Math.sqrt(s); m[i] = k * a * d[i]; m[i + 1] = k * b * d[i]; }
  }
  const i = p.findIndex((_, j) => j < n - 1 && x <= p[j + 1][0]);
  const h = p[i + 1][0] - p[i][0], u = (x - p[i][0]) / h, u2 = u * u, u3 = u2 * u;
  return (2 * u3 - 3 * u2 + 1) * p[i][1] + (u3 - 2 * u2 + u) * h * m[i] + (-2 * u3 + 3 * u2) * p[i + 1][1] + (u3 - u2) * h * m[i + 1];
}

/** Object > Image > Adjustments: adjustment layers over image.adjust. The picture shown is the engine's
 *  own render of the settings (image.adjustPreview), at most one request in flight and the latest settings winning,
 *  so it matches what the page and the PDF will show. Moving a control changes canvases and readouts only. */
class Adjustments {
  private readonly d = new Dialog(t('Adjustments'), 780);
  private readonly pic = el<HTMLCanvasElement>('<canvas class="ia-pic" width="440" height="320"></canvas>');
  private readonly crv = el<HTMLCanvasElement>('<canvas class="ia-curve" width="180" height="180"></canvas>');
  private readonly ranges = new Map<keyof Adj, { input: HTMLInputElement; val: HTMLElement }>();
  private readonly bw = el<HTMLInputElement>('<input type="checkbox">');
  private readonly removeBtn: HTMLButtonElement;
  private a: Adj = fresh();
  private had = false;
  private frame = 0;
  private want = 0;
  private busy = false;
  private drag = -1;
  private bmp: ImageBitmap | null = null;

  constructor() {
    const wrap = el(`<div class="ia-wrap"><div class="ia-left"></div><div class="ia-right"><div class="ia-sliders"></div>
      <label class="cv-opt ia-bw"></label>
      <div class="ia-curvebox"><span class="ia-label">${t('Curve')}</span></div>
      <p class="cv-note ia-hint">${t('Click the curve to add a point, drag to move it, double-click to remove it.')}</p>
      <button class="cv-btn cv-btn-ghost ia-reset" type="button">${t('Reset')}</button></div></div>`);
    q(wrap, '.ia-left').appendChild(this.pic);
    q(wrap, '.ia-curvebox').appendChild(this.crv);
    q(wrap, '.ia-bw').append(this.bw, el(`<span>${t('Black and White')}</span>`));
    const box = q(wrap, '.ia-sliders');
    for (const [key, label, min, max, step] of SLIDERS) {
      const row = el(`<label class="ia-row"><span class="ia-name"></span><input type="range"><span class="ia-val" data-no-i18n></span></label>`);
      setText(q(row, '.ia-name'), label());
      const input = q<HTMLInputElement>(row, 'input');
      Object.assign(input, { min: String(min), max: String(max), step: String(step) });
      input.addEventListener('input', () => {
        let v = Number(input.value);
        if (key === 'black') v = Math.min(v, this.a.white - 1);
        if (key === 'white') v = Math.max(v, this.a.black + 1);
        (this.a as unknown as Record<string, number>)[key] = v;
        this.sync();
        this.kick();
      });
      this.ranges.set(key, { input, val: q(row, '.ia-val') });
      box.appendChild(row);
    }
    this.bw.addEventListener('change', () => { this.a.blackWhite = this.bw.checked; this.kick(); });
    q(wrap, '.ia-reset').addEventListener('click', () => { this.a = fresh(); this.sync(); this.drawCurve(); this.kick(); });
    this.crv.addEventListener('pointerdown', (e) => this.down(e));
    this.crv.addEventListener('pointermove', (e) => this.move(e));
    this.crv.addEventListener('pointerup', () => { this.drag = -1; });
    this.crv.addEventListener('dblclick', (e) => this.remove(e));
    this.d.body.appendChild(wrap);
    this.removeBtn = this.d.button(t('Remove Adjustments'), 'ghost', () => void this.commit(true));
    this.removeBtn.classList.add('cv-btn-apart');
    this.d.button(t('Cancel'), 'ghost', () => this.d.close());
    this.d.button(t('OK'), 'primary', () => void this.commit(false));
  }

  private sync(): void {
    for (const [k, r] of this.ranges) {
      const v = this.a[k] as number;
      if (Number(r.input.value) !== v) r.input.value = String(v);
      setText(r.val, k === 'exposure' || k === 'gamma' ? v.toFixed(2) : String(Math.round(v)));
    }
    if (this.bw.checked !== this.a.blackWhite) this.bw.checked = this.a.blackWhite;
  }

  /** What the engine takes: every key, so the preview and OK describe the whole stack. */
  private params(): Record<string, unknown> {
    const a = this.a;
    const flat = a.curve.length === 2 && a.curve.some((p) => p[0] === 0 && p[1] === 0) && a.curve.some((p) => p[0] === 255 && p[1] === 255);
    return {
      exposure: a.exposure, contrast: a.contrast, saturation: a.saturation, temperature: a.temperature, hue: a.hue, lightness: a.lightness,
      blackWhite: a.blackWhite, levels: a.black === 0 && a.white === 255 && a.gamma === 1 ? null : [a.black, a.white, a.gamma],
      curve: flat ? null : a.curve.slice().sort((p, r) => p[0] - r[0]).map((p) => [Math.round(p[0]), Math.round(p[1])]),
    };
  }

  private isDefault(): boolean {
    const p = this.params();
    return !p.exposure && !p.contrast && !p.saturation && !p.temperature && !p.hue && !p.lightness && !p.blackWhite && p.levels === null && p.curve === null;
  }

  async open(): Promise<void> {
    const id = editor.selection?.items?.[0];
    if (id === undefined) return;
    this.frame = id;
    const r = await editor.exec<{ current: Record<string, unknown> | null }>('image.adjustPreview', { ids: [id], maxSide: 640 });
    if (!r) return;
    const c = r.current ?? {};
    const lv = (c.levels as number[] | undefined) ?? [0, 255, 1];
    this.a = { ...fresh(), exposure: Number(c.exposure ?? 0), contrast: Number(c.contrast ?? 0), saturation: Number(c.saturation ?? 0), temperature: Number(c.temperature ?? 0), hue: Number(c.hue ?? 0), lightness: Number(c.lightness ?? 0), black: lv[0], white: lv[1], gamma: lv[2], blackWhite: c.blackWhite === true, curve: (c.curve as Pt[] | undefined) ?? [[0, 0], [255, 255]] };
    this.had = r.current !== null;
    setDisabled(this.removeBtn, !this.had);
    this.sync();
    this.drawCurve();
    await this.show(r as unknown as { base64: string; mime: string });
    this.d.open();
  }

  private kick(): void {
    this.want += 1;
    if (!this.busy) void this.pump();
  }

  private async pump(): Promise<void> {
    this.busy = true;
    for (;;) {
      const ticket = this.want;
      const r = await editor.exec<{ base64: string; mime: string }>('image.adjustPreview', { ids: [this.frame], maxSide: 640, ...this.params() }, true);
      if (r) await this.show(r);
      if (ticket === this.want) break;
    }
    this.busy = false;
  }

  private async show(r: { base64: string; mime: string }): Promise<void> {
    const bytes = Uint8Array.from(atob(r.base64), (ch) => ch.charCodeAt(0));
    const bmp = await createImageBitmap(new Blob([bytes], { type: r.mime }));
    this.bmp?.close();
    this.bmp = bmp;
    const c = this.pic.getContext('2d') as CanvasRenderingContext2D;
    const s = Math.min(this.pic.width / bmp.width, this.pic.height / bmp.height);
    const w = bmp.width * s, h = bmp.height * s;
    c.clearRect(0, 0, this.pic.width, this.pic.height);
    c.drawImage(bmp, (this.pic.width - w) / 2, (this.pic.height - h) / 2, w, h);
  }

  private drawCurve(): void {
    const c = this.crv.getContext('2d') as CanvasRenderingContext2D;
    const W = this.crv.width, H = this.crv.height;
    const css = getComputedStyle(this.crv);
    c.clearRect(0, 0, W, H);
    c.strokeStyle = css.getPropertyValue('--ia-grid').trim() || 'rgba(255,255,255,0.12)';
    c.lineWidth = 1;
    for (let i = 1; i < 4; i++) {
      c.beginPath(); c.moveTo((W * i) / 4 + 0.5, 0); c.lineTo((W * i) / 4 + 0.5, H); c.moveTo(0, (H * i) / 4 + 0.5); c.lineTo(W, (H * i) / 4 + 0.5); c.stroke();
    }
    c.beginPath(); c.moveTo(0, H); c.lineTo(W, 0); c.stroke();
    c.strokeStyle = css.getPropertyValue('--ia-line').trim() || '#f97316';
    c.lineWidth = 2;
    c.beginPath();
    for (let x = 0; x <= 255; x += 3) {
      const px = (x / 255) * W, py = H - (curveAt(this.a.curve, x) / 255) * H;
      if (x) c.lineTo(px, py); else c.moveTo(px, py);
    }
    c.stroke();
    c.fillStyle = c.strokeStyle;
    for (const [x, y] of this.a.curve) { c.beginPath(); c.arc((x / 255) * W, H - (y / 255) * H, 4, 0, Math.PI * 2); c.fill(); }
  }

  private at(e: PointerEvent | MouseEvent): Pt {
    const r = this.crv.getBoundingClientRect();
    return [Math.min(255, Math.max(0, ((e.clientX - r.left) / r.width) * 255)), Math.min(255, Math.max(0, (1 - (e.clientY - r.top) / r.height) * 255))];
  }

  private hit(p: Pt): number {
    return this.a.curve.findIndex((q2) => Math.hypot(q2[0] - p[0], q2[1] - p[1]) < 12);
  }

  private down(e: PointerEvent): void {
    const p = this.at(e);
    let i = this.hit(p);
    if (i < 0) { this.a.curve = [...this.a.curve, p]; i = this.a.curve.length - 1; }
    this.drag = i;
    this.crv.setPointerCapture(e.pointerId);
    this.drawCurve();
    this.kick();
  }

  private move(e: PointerEvent): void {
    if (this.drag < 0) return;
    const p = this.at(e), old = this.a.curve[this.drag];
    // The two ends keep their input and move only up and down, so the curve always covers 0 to 255.
    const end = old[0] === 0 || old[0] === 255;
    this.a.curve = this.a.curve.map((q2, k) => (k === this.drag ? [end ? old[0] : p[0], p[1]] : q2));
    this.drawCurve();
    this.kick();
  }

  private remove(e: MouseEvent): void {
    const i = this.hit(this.at(e));
    if (i < 0 || this.a.curve[i][0] === 0 || this.a.curve[i][0] === 255) return;
    this.a.curve = this.a.curve.filter((_, k) => k !== i);
    this.drawCurve();
    this.kick();
  }

  private async commit(remove: boolean): Promise<void> {
    if (remove || this.isDefault()) {
      if (!this.had) { this.d.close(); return; }
      if ((await editor.exec('image.adjust', { ids: [this.frame], remove: true })) !== undefined) this.d.close();
      return;
    }
    if ((await editor.exec('image.adjust', { ids: [this.frame], ...this.params() })) !== undefined) this.d.close();
  }
}

export function installImageAdjust(ws: Workspace): void {
  let dlg: Adjustments | null = null;
  ws.override('image.adjust', () => { void (dlg ??= new Adjustments()).open(); });
}
