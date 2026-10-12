import { Canvas2dCompositor } from './canvas2d';
import type { Compositor, Preference } from './index';
import { WebGl2Compositor } from './webgl2';
import { NativeCompositor } from './native';
import { WebGpuCompositor } from './webgpu';

// Choosing a display path. Auto tries WebGPU, then WebGL2, then Canvas 2D (the family standard, as in formvis); a forced
// path is tried first and the paths after it in that order are the fallbacks, so a forced path that this browser lacks
// still ends on something that draws. A canvas that has handed out one kind of context cannot hand out another, so every
// attempt gets a fresh canvas from the factory and a failed attempt gives it back.

export type PathId = Exclude<Preference, 'auto'>;
export const ORDER: PathId[] = ['native', 'webgpu', 'webgl2', 'canvas2d'];
/** Auto never picks the native path (it is opt-in and exists only inside the desktop shell), so it starts after it. */
const AUTO_FROM = 1;

export interface CanvasFactory {
  /** A canvas for a path: the page's own canvas for Canvas 2D, a new one inserted under the overlays otherwise. */
  create(id: PathId): HTMLCanvasElement;
  /** Takes back a canvas whose path failed to start. */
  discard(canvas: HTMLCanvasElement, id: PathId): void;
}

export interface Selected { id: PathId; compositor: Compositor }

/** The paths to try for a preference, optionally only those after one that was lost. */
export function chainFor(pref: Preference, after?: PathId): PathId[] {
  const start = pref === 'auto' ? AUTO_FROM : ORDER.indexOf(pref);
  const from = after ? Math.max(start, ORDER.indexOf(after) + 1) : start;
  return ORDER.slice(from);
}

function make(id: PathId): Compositor {
  if (id === 'native') return new NativeCompositor();
  return id === 'webgpu' ? new WebGpuCompositor() : id === 'webgl2' ? new WebGl2Compositor() : new Canvas2dCompositor();
}

export async function selectCompositor(pref: Preference, factory: CanvasFactory, after?: PathId, existing?: Map<PathId, Compositor>): Promise<Selected | null> {
  for (const id of chainFor(pref, after)) {
    const reuse = existing?.get(id);
    if (reuse) return { id, compositor: reuse };
    const canvas = factory.create(id);
    const c = make(id);
    let ok = false;
    try { ok = await c.init(canvas); } catch { ok = false; }
    if (ok) return { id, compositor: c };
    try { c.destroy(); } catch { }
    factory.discard(canvas, id);
  }
  return null;
}
