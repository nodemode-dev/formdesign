import { editor } from '../editor/editor';
import type { Workspace } from '../ui/workspace';

interface ClipResult { text?: string }

function writeSystem(text: string | undefined): void {
  if (!text) return;
  try { void navigator.clipboard?.writeText(text).catch(() => undefined); } catch { }
}

async function readSystem(): Promise<string | undefined> {
  try {
    const read = navigator.clipboard?.readText?.();
    if (!read) return undefined;
    const t = await Promise.race([read, new Promise<undefined>((res) => setTimeout(() => res(undefined), 600))]);
    return typeof t === 'string' && t ? t : undefined;
  } catch { return undefined; }
}

function viewTarget(ws: Workspace): Record<string, unknown> {
  const lay = editor.layout;
  if (!lay || lay.editingParents || !lay.pages.length) return {};
  const abs = ws.viewport.currentPage();
  const page = lay.pages.find((p) => p.abs === abs);
  if (!page) return {};
  const slot = lay.slots.find((s) => {
    const r = s.spread as unknown as { kind?: string; index?: number } | number;
    return typeof r === 'number' ? r === page.spread : r?.kind !== 'parent' && r?.index === page.spread;
  });
  const vr = ws.viewport.visibleRect();
  const cx = (vr[0] + vr[2]) / 2, cy = (vr[1] + vr[3]) / 2;
  if (!slot || slot.xf.length < 6) return { spread: page.spread };
  const [a, b, c, d, e, f] = slot.xf;
  const det = a * d - b * c;
  if (Math.abs(det) < 1e-9) return { spread: page.spread };
  const x = cx - e, y = cy - f;
  return { spread: page.spread, at: [(d * x - c * y) / det, (-b * x + a * y) / det] };
}

export function installClipboard(ws: Workspace): void {
  const copy = (id: string) => async () => { writeSystem(((await ws.runCore(id)) as ClipResult | undefined)?.text); };
  ws.override('edit.copy', () => void copy('edit.copy')());
  ws.override('edit.cut', () => void copy('edit.cut')());
  const paste = (id: string, here: boolean) => async () => {
    const text = await readSystem();
    const target = here && !editor.wantsText ? viewTarget(ws) : {};
    await ws.runCore(id, { ...target, ...(text ? { text } : {}) });
  };
  ws.override('edit.paste', () => void paste('edit.paste', true)());
  ws.override('edit.pasteWithoutFormatting', () => void paste('edit.pasteWithoutFormatting', true)());
}
