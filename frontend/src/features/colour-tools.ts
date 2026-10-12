import { api } from '../engine/client';
import { editor } from '../editor/editor';
import { Dialog } from '../ui/dialogs';
import { el, q, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';
import { gradientStops } from './attr-editors';

export function swatchNames(r: unknown): string[] {
  return Array.isArray(r) ? r.map((s) => (typeof s === 'string' ? s : String((s as { name?: string })?.name ?? ''))).filter((n) => n && n !== '[None]') : [];
}

async function swatches(): Promise<string[]> {
  return swatchNames(await editor.exec<unknown>('swatch.list', {}, true).catch(() => []));
}

async function gradientDialog(ws: Workspace, params: Record<string, unknown> = {}): Promise<void> {
  const d = new Dialog('New Gradient Swatch', 560);
  const form = el(`<div class="fo-form">
    <label class="fo-field"><span>Swatch Name</span><input class="fgr-name" type="text" spellcheck="false"></label>
    <label class="fo-field"><span>Type</span><select class="fgr-type"><option value="linear">Linear</option><option value="radial">Radial</option></select></label>
    <div class="fo-field fae-field"><span>Stops</span><div class="fgr-stops"></div></div>
  </div>`);
  const name = q<HTMLInputElement>(form, '.fgr-name');
  name.value = String(params.name ?? 'New Gradient');
  const type = q<HTMLSelectElement>(form, '.fgr-type');
  type.value = params.type === 'radial' ? 'radial' : 'linear';
  const stops = gradientStops(params.stops, { charStyles: [], swatches: await swatches() });
  q(form, '.fgr-stops').appendChild(stops.node);
  d.body.appendChild(form);
  const cancel = el<HTMLButtonElement>('<button class="cv-btn" type="button">Cancel</button>');
  const ok = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">OK</button>');
  cancel.addEventListener('click', () => d.close());
  ok.addEventListener('click', () => {
    const list = (stops.read() as Array<{ color: string; at: number; opacity: number }>).map((s) => ({ color: s.color, at: Math.max(0, Math.min(100, Number(s.at))), opacity: Math.max(0, Math.min(100, Number(s.opacity))) }));
    if (list.length < 2) { toast('A gradient needs at least two stops', 'error'); return; }
    if (!name.value.trim()) { toast('Name the swatch', 'error'); return; }
    d.close();
    void ws.runCore('swatch.createGradient', { name: name.value.trim(), type: type.value, stops: list.sort((a, b) => a.at - b.at) });
  });
  d.foot.append(cancel, ok);
  d.onClose = () => d.scrim.remove();
  d.open(name);
}

async function colourTheme(ws: Workspace): Promise<void> {
  const files = await ws.pickFiles('image/*', false);
  if (!files.length) return;
  let colours: string[] = [];
  try {
    const [path] = await ws.upload(files);
    colours = (await api<{ colours: string[] }>('/colour/theme', { method: 'POST', json: { path, count: 6 } })).colours;
  } catch (e) { toast((e as Error).message, 'error'); return; }
  const base = files[0].name.replace(/\.[^.]+$/, '').slice(0, 40) || 'Image';
  const d = new Dialog('Colour Theme from Image', 460);
  const row = el('<div class="fct-chips"></div>');
  for (const c of colours) {
    const chip = el('<label class="fct-chip"><input type="checkbox" checked><i></i><span></span></label>');
    (q(chip, 'i') as HTMLElement).style.background = c;
    setText(q(chip, 'span'), c);
    chip.dataset.hex = c;
    row.appendChild(chip);
  }
  const form = el('<div class="fo-form"><label class="fo-field"><span>Colour Group</span><input class="fct-group" type="text" spellcheck="false"></label></div>');
  const group = q<HTMLInputElement>(form, '.fct-group');
  group.value = `${base} theme`;
  d.body.append(row, form);
  const cancel = el<HTMLButtonElement>('<button class="cv-btn" type="button">Cancel</button>');
  const ok = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">Add to Swatches</button>');
  cancel.addEventListener('click', () => d.close());
  ok.addEventListener('click', () => {
    const picked = Array.from(row.querySelectorAll<HTMLElement>('.fct-chip')).filter((c) => (c.querySelector('input') as HTMLInputElement).checked).map((c) => c.dataset.hex as string);
    d.close();
    if (!picked.length) return;
    const g = group.value.trim() || `${base} theme`;
    const names = picked.map((h, i) => `${g} ${i + 1}`);
    const commands = [...picked.map((h, i) => ({ command: 'swatch.create', params: { name: names[i], color: h } })), { command: 'swatch.group', params: { swatches: names, group: g } }];
    void ws.runCore('script.run', { commands, label: 'Colour Theme' }).then((r) => { if (r !== undefined) toast(`${picked.length} swatches added in ${g}`); });
  });
  d.foot.append(cancel, ok);
  d.onClose = () => d.scrim.remove();
  d.open(ok);
}

export function installColourTools(ws: Workspace): void {
  ws.override('swatch.createGradient', (p) => void gradientDialog(ws, p));
  ws.addCommands({ id: 'ui.colourTheme', label: 'Colour Theme from Image…', enabled: () => editor.hasDoc, run: () => void colourTheme(ws) });
  ws.addMenuItems({ menu: 'Window', items: ['ui.colourTheme'], at: 'end' });
}
