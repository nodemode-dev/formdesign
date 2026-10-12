import { isOneToOne, place, sizeCanvas, SHADOW_ALPHA, SHADOW_BLUR, SHADOW_OFFSET, type Compositor, type Frame, type TileSource } from './index';

// Today's drawing, behind the Compositor interface: the pasteboard, a drop shadow and white paper per page, then the
// tiles with high quality smoothing (nearest at exactly 1:1). It needs nothing from the browser but a 2D context, so it
// is the path every other path falls back to.
export class Canvas2dCompositor implements Compositor {
  readonly name = 'Canvas 2D';
  onLost: ((why: string) => void) | null = null;
  canvas: HTMLCanvasElement | null = null;
  private ctx: CanvasRenderingContext2D | null = null;
  private readonly tiles = new Map<string, CanvasImageSource>();

  async init(canvas: HTMLCanvasElement): Promise<boolean> {
    const ctx = canvas.getContext('2d', { alpha: false });
    if (!ctx) return false;
    this.canvas = canvas;
    this.ctx = ctx;
    return true;
  }

  upload(key: string, source: TileSource): void {
    this.evict(key);
    if (typeof ImageData !== 'undefined' && source instanceof ImageData) {
      // Rare: a caller that has pixels but no bitmap. Keep it as a small canvas so drawImage can take it.
      const c = document.createElement('canvas');
      c.width = source.width;
      c.height = source.height;
      c.getContext('2d')?.putImageData(source, 0, 0);
      this.tiles.set(key, c);
      return;
    }
    // The bitmap stays owned by the caller (the viewport keeps it for every path), so it is only referenced here.
    this.tiles.set(key, source as ImageBitmap);
  }

  has(key: string): boolean { return this.tiles.has(key); }

  evict(key: string): void { this.tiles.delete(key); }

  draw(frame: Frame): string[] {
    const ctx = this.ctx;
    if (!ctx || !this.canvas) return [];
    const missing: string[] = [];
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.globalAlpha = 1;
    ctx.fillStyle = `rgb(${frame.bg[0]},${frame.bg[1]},${frame.bg[2]})`;
    ctx.fillRect(0, 0, this.canvas.width, this.canvas.height);
    const dpr = frame.dpr;
    for (const p of frame.pages) {
      const [x0, y0, x1, y1] = place(frame, p);
      ctx.save();
      ctx.shadowColor = `rgba(0,0,0,${SHADOW_ALPHA})`;
      ctx.shadowBlur = SHADOW_BLUR * 2 * dpr;
      ctx.shadowOffsetY = SHADOW_OFFSET * dpr;
      ctx.fillStyle = '#ffffff';
      ctx.fillRect(x0, y0, x1 - x0, y1 - y0);
      ctx.restore();
      ctx.fillStyle = '#ffffff';
      ctx.fillRect(x0, y0, x1 - x0, y1 - y0);
    }
    ctx.imageSmoothingQuality = 'high';
    for (const t of frame.tiles) {
      const img = this.tiles.get(t.key);
      if (!img) { missing.push(t.key); continue; }
      const [x0, y0, x1, y1] = place(frame, t.rect);
      ctx.imageSmoothingEnabled = !isOneToOne(frame, t);
      ctx.drawImage(img, x0, y0, x1 - x0, y1 - y0);
    }
    ctx.imageSmoothingEnabled = true;
    return missing;
  }

  resize(width: number, height: number, dpr: number): void {
    if (this.canvas) sizeCanvas(this.canvas, width, height, dpr);
  }

  destroy(): void {
    this.tiles.clear();
    this.ctx = null;
  }
}
