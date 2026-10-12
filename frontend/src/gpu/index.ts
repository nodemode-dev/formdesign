// The display paths of the canvas. A Compositor draws one frame (pasteboard, page shadows, paper, tiles) from the tiles
// it holds; viewport.ts decides which tiles and pages are in the frame and requests the ones that are missing. The
// frame model mirrors native/formdes-gpu (composite.rs): pasteboard clear, a soft drop shadow under each page, white
// paper under its tiles (so a tile that has not arrived leaves paper, never a hole), tiles through the view transform,
// linear filtering when minified and nearest filtering, with edges on whole device pixels, at exactly 1:1.

export type Rect4 = [number, number, number, number];
export type TileSource = ImageBitmap | ImageData;

export interface FrameTile {
  /** The key the tile was uploaded under. */
  key: string;
  /** The tile's rectangle in canvas units (x0, y0, x1, y1). */
  rect: Rect4;
  /** The scale (device pixels per canvas unit) the tile was rastered at. */
  level: number;
}

export interface Frame {
  /** The target in device pixels. */
  width: number;
  height: number;
  dpr: number;
  /** The pasteboard colour, 0 to 255 per channel. */
  bg: [number, number, number];
  /** The view: device pixels per canvas unit, and the canvas point at the target's top left. */
  scale: number;
  x: number;
  y: number;
  /** The pages that may be visible, in canvas units. */
  pages: Rect4[];
  /** The tiles to draw in order (older levels first, the current level last). */
  tiles: FrameTile[];
}

export interface Compositor {
  /** Shown in the status bar: WebGPU, WebGL2 or Canvas 2D. */
  readonly name: string;
  /** The canvas this compositor draws to (null until init succeeds). */
  readonly canvas: HTMLCanvasElement | null;
  /** Called once if the device or context is lost; the owner falls back to the next path. */
  onLost: ((why: string) => void) | null;
  init(canvas: HTMLCanvasElement): Promise<boolean>;
  upload(key: string, source: TileSource): void;
  has(key: string): boolean;
  evict(key: string): void;
  /** Draws the frame and returns the keys of frame tiles that are not resident (their paper is drawn, nothing else). */
  draw(frame: Frame): string[];
  resize(width: number, height: number, dpr: number): void;
  destroy(): void;
}

export type Preference = 'auto' | 'webgpu' | 'webgl2' | 'canvas2d';
export const PREFERENCES: Array<{ id: Preference; label: string }> = [
  { id: 'auto', label: 'Auto' }, { id: 'webgpu', label: 'WebGPU' }, { id: 'webgl2', label: 'WebGL2' }, { id: 'canvas2d', label: 'Canvas 2D' },
];

/** The shadow under a page, in CSS pixels (the same numbers as composite.rs). */
export const SHADOW_BLUR = 8;
export const SHADOW_OFFSET = 3;
export const SHADOW_ALPHA = 0.35;

/** A canvas rectangle on whole device pixels. Neighbouring tiles share an edge value, so they never leave a seam. */
export function place(f: { scale: number; x: number; y: number }, r: Rect4): Rect4 {
  return [Math.round((r[0] - f.x) * f.scale), Math.round((r[1] - f.y) * f.scale), Math.round((r[2] - f.x) * f.scale), Math.round((r[3] - f.y) * f.scale)];
}

/** True when a tile of this level lands one raster pixel on one device pixel. */
export function isOneToOne(frame: Frame, tile: FrameTile): boolean {
  return Math.abs(tile.level / frame.scale - 1) < 1e-3;
}

export function sizeCanvas(canvas: HTMLCanvasElement, w: number, h: number, dpr: number): void {
  const pw = Math.max(1, Math.round(w * dpr)), ph = Math.max(1, Math.round(h * dpr));
  if (canvas.width !== pw) canvas.width = pw;
  if (canvas.height !== ph) canvas.height = ph;
  canvas.style.width = `${w}px`;
  canvas.style.height = `${h}px`;
}

export function sourceSize(s: TileSource): [number, number] { return [s.width, s.height]; }
