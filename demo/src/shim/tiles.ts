import { bytes, session, type Render } from './session';

interface TileAsk { key: string; s: number; ox: number; oy: number; w: number; h: number }

const pictures = new Map<Render, Promise<ImageBitmap>>();

const picture = (r: Render): Promise<ImageBitmap> => {
  let p = pictures.get(r);
  if (!p) {
    p = createImageBitmap(new Blob([bytes(r.b64)], { type: 'image/webp' }));
    pictures.set(r, p);
  }
  return p;
};

const pick = (list: Render[], s: number): Render => list.find((r) => r.s >= s - 1e-6) ?? list[list.length - 1];

export const recordedView = (p: Record<string, unknown>): boolean =>
  !p.preview && !p.overprint && p.plate == null && !p.inkLimit && !p.proof && !p.changes;

export async function drawTiles(p: Record<string, unknown>): Promise<{ result: unknown; data: Uint8Array }> {
  const asks = (p.tiles as TileAsk[]) ?? [];
  const bg = (p.bg as number[]) ?? [28, 27, 29];
  const parts: Uint8ClampedArray[] = [];
  const out: Array<{ key: string; w: number; h: number; offset: number; len: number }> = [];
  let offset = 0;
  for (const t of asks) {
    const c = new OffscreenCanvas(t.w, t.h);
    const g = c.getContext('2d', { willReadFrequently: true });
    if (!g) throw new Error('no 2d context');
    g.fillStyle = `rgb(${bg[0]}, ${bg[1]}, ${bg[2]})`;
    g.fillRect(0, 0, t.w, t.h);
    g.imageSmoothingQuality = 'high';
    const x1 = t.ox + t.w / t.s, y1 = t.oy + t.h / t.s;
    for (const sp of session?.spreads ?? []) {
      const [bx0, by0, bx1, by1] = sp.bounds;
      if (bx1 < t.ox || bx0 > x1 || by1 < t.oy || by0 > y1) continue;
      const r = pick(sp.scales, t.s);
      const k = t.s / r.s;
      g.drawImage(await picture(r), (bx0 - t.ox) * t.s, (by0 - t.oy) * t.s, r.w * k, r.h * k);
    }
    const px = g.getImageData(0, 0, t.w, t.h).data;
    parts.push(px);
    out.push({ key: t.key, w: t.w, h: t.h, offset, len: px.length });
    offset += px.length;
  }
  const data = new Uint8Array(offset);
  let at = 0;
  for (const px of parts) { data.set(px, at); at += px.length; }
  return { result: { tiles: out, format: 'rgba' }, data };
}
