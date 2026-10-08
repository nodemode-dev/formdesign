import { editor } from '../editor/editor';
import { engine } from '../engine/client';
import { Dialog } from '../ui/dialogs';
import { el, q } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface PathType { id: number; start: number; flip: boolean; align: string }

const ALIGN: Array<[string, string]> = [['baseline', 'Baseline'], ['ascender', 'Ascender'], ['descender', 'Descender'], ['center', 'Center']];

async function options(ws: Workspace): Promise<void> {
  const r = await engine.call<PathType | null>('pathType', {}).catch(() => undefined);
  const pt = r?.result as PathType | null | undefined;
  if (!pt) { toast('Select type on a path first: click its text with a type tool, or select the path.', 'error'); return; }
  const d = new Dialog('Type on a Path Options', 400);
  const body = el(`<div class="fo-form">
    <label class="fo-field"><span>Align</span><select class="fpt-align"></select></label>
    <label class="fo-field"><span>Start (pt)</span><input class="fpt-start" type="number" min="0" step="1"></label>
    <label class="fo-field"><span>Flip</span><input class="fpt-flip" type="checkbox"></label>
    <p class="fo-note">Align sets which part of the type sits on the path. Flip runs the text the other way round, on the other side of the path.</p>
  </div>`);
  const align = q<HTMLSelectElement>(body, '.fpt-align');
  for (const [v, l] of ALIGN) align.append(new Option(l, v));
  align.value = pt.align;
  q<HTMLInputElement>(body, '.fpt-start').value = String(Math.round(pt.start * 100) / 100);
  q<HTMLInputElement>(body, '.fpt-flip').checked = pt.flip;
  d.body.appendChild(body);
  const del = el<HTMLButtonElement>('<button class="cv-btn cv-btn-danger cv-btn-apart" type="button">Delete Type from Path</button>');
  const cancel = el<HTMLButtonElement>('<button class="cv-btn" type="button">Cancel</button>');
  const ok = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">OK</button>');
  d.foot.append(del, cancel, ok);
  const run = async (params: Record<string, unknown>) => {
    d.close();
    await editor.exec('type.pathOptions', { id: pt.id, ...params });
    ws.viewport.refreshOverlays();
  };
  cancel.addEventListener('click', () => d.close());
  del.addEventListener('click', () => void run({ delete: true }));
  ok.addEventListener('click', () => {
    const start = Number(q<HTMLInputElement>(body, '.fpt-start').value);
    void run({ align: align.value, flip: q<HTMLInputElement>(body, '.fpt-flip').checked, ...(Number.isFinite(start) && start >= 0 ? { start } : {}) });
  });
  d.onClose = () => d.scrim.remove();
  d.open(align);
}

export function installPathType(ws: Workspace): void {
  ws.override('type.pathOptions', () => void options(ws));
}
