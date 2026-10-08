import '../styles/separations.css';
import { editor } from '../editor/editor';
import { el, patchList, q, setChecked, setHidden, setText, setValue } from '../ui/dom';
import type { Workspace } from '../ui/workspace';

type Mode = 'off' | 'separations' | 'ink';
interface Ink { name: string; process: boolean; alias?: string }

const PLATES: Array<[number | null, string, string]> = [
  [null, 'CMYK', 'conic-gradient(#00aeef 0 25%, #ec008c 0 50%, #fff200 0 75%, #231f20 0)'],
  [0, 'Cyan', '#00aeef'], [1, 'Magenta', '#ec008c'], [2, 'Yellow', '#fff200'], [3, 'Black', '#231f20'],
];

export class SeparationsPanel {
  readonly root = el(`<div class="fsp-panel">
    <div class="cv-seg fsp-mode" role="radiogroup" aria-label="View"><button type="button" data-mode="off">Off</button><button type="button" data-mode="separations">Separations</button><button type="button" data-mode="ink">Ink Limit</button></div>
    <div class="fsp-plates" role="radiogroup" aria-label="Plates"></div>
    <div class="fsp-limit"><div class="fp-num fsp-num"><span class="fp-num-lab fsp-lab">Limit</span><input class="fp-num-in fsp-limit-in" type="text" value="300" aria-label="Total ink limit in percent"></div><span class="fsp-unit">% total ink; areas above show red</span></div>
    <label class="fsp-check"><input type="checkbox" class="fsp-over"><span>Overprint Preview</span></label>
    <div class="fsp-sec">Spot inks</div>
    <div class="fsp-inks"></div>
    <p class="fsp-note">Plates come from each object's own colours through the colour settings. Spot inks set to print as process are shown in their process plates.</p>
  </div>`);
  private mode: Mode = 'off';
  private plate: number | null = null;
  private inks: Ink[] = [];

  constructor(private readonly ws: Workspace) {
    const plates = q(this.root, '.fsp-plates');
    for (const [p, name, sw] of PLATES) {
      const b = el<HTMLButtonElement>(`<button class="fsp-plate" type="button" role="radio"><span class="fsp-sw"></span><span class="fsp-pname"></span></button>`);
      (q(b, '.fsp-sw') as HTMLElement).style.setProperty('--fsp-sw', sw);
      setText(q(b, '.fsp-pname'), name);
      b.dataset.plate = p == null ? '' : String(p);
      b.addEventListener('click', () => { this.plate = p; this.apply(); });
      plates.appendChild(b);
    }
    for (const b of this.root.querySelectorAll<HTMLButtonElement>('.fsp-mode button')) b.addEventListener('click', () => { this.mode = b.dataset.mode as Mode; this.apply(); });
    const lim = q<HTMLInputElement>(this.root, '.fsp-limit-in');
    lim.addEventListener('change', () => this.apply());
    lim.addEventListener('keydown', (e) => { if (e.key === 'Enter') this.apply(); });
    q<HTMLInputElement>(this.root, '.fsp-over').addEventListener('change', (e) => { ws.viewport.opts.overprint = (e.target as HTMLInputElement).checked; ws.viewport.invalidateAll(); });
    editor.on('docs', () => void this.loadInks());
    this.render();
  }

  onShow(): void { void this.loadInks(); this.render(); }

  toggleOverprint(): void {
    this.ws.viewport.opts.overprint = !this.ws.viewport.opts.overprint;
    this.ws.viewport.invalidateAll();
    this.render();
  }

  private apply(): void {
    const vp = this.ws.viewport;
    const limit = Math.max(100, Math.min(400, Number(q<HTMLInputElement>(this.root, '.fsp-limit-in').value) || 300));
    setValue(q<HTMLInputElement>(this.root, '.fsp-limit-in'), String(limit));
    const plate = this.mode === 'separations' ? this.plate : null;
    const ink = this.mode === 'ink' ? limit : null;
    if (vp.opts.plate !== plate || vp.opts.inkLimit !== ink) {
      vp.opts.plate = plate;
      vp.opts.inkLimit = ink;
      vp.invalidateAll();
    }
    this.render();
  }

  private async loadInks(): Promise<void> {
    const r = editor.hasDoc ? await editor.exec<{ inks?: Ink[] }>('ink.list', {}, true).catch(() => undefined) : undefined;
    this.inks = r?.inks ?? [];
    this.render();
  }

  private render(): void {
    for (const b of this.root.querySelectorAll<HTMLButtonElement>('.fsp-mode button')) {
      const on = b.dataset.mode === this.mode;
      b.classList.toggle('is-on', on);
      b.setAttribute('aria-checked', String(on));
    }
    setHidden(q(this.root, '.fsp-plates'), this.mode !== 'separations');
    setHidden(q(this.root, '.fsp-limit'), this.mode !== 'ink');
    for (const b of this.root.querySelectorAll<HTMLButtonElement>('.fsp-plate')) {
      const on = b.dataset.plate === (this.plate == null ? '' : String(this.plate));
      b.classList.toggle('is-on', on);
      b.setAttribute('aria-checked', String(on));
    }
    setChecked(q<HTMLInputElement>(this.root, '.fsp-over'), this.ws.viewport.opts.overprint);
    const box = q(this.root, '.fsp-inks');
    patchList<Ink>(box, this.inks, {
      key: (i) => i.name,
      create: (i) => {
        const r = el('<label class="fsp-ink"><span class="fsp-iname"></span><span class="fsp-proc"><input type="checkbox"><span>Print as process</span></span></label>');
        q<HTMLInputElement>(r, 'input').addEventListener('change', (e) => void editor.exec('ink.options', { ink: r.dataset.key, toProcess: (e.target as HTMLInputElement).checked }).then(() => { this.ws.viewport.invalidateAll(); void this.loadInks(); }));
        setText(q(r, '.fsp-iname'), i.name);
        setChecked(q<HTMLInputElement>(r, 'input'), i.process);
        return r;
      },
      update: (r, i) => { setText(q(r, '.fsp-iname'), i.name + (i.alias ? ` (as ${i.alias})` : '')); setChecked(q<HTMLInputElement>(r, 'input'), i.process); },
    });
    if (!this.inks.length && !box.querySelector('.fsp-none')) box.appendChild(el('<span class="fsp-none">This document has no spot inks.</span>'));
    if (this.inks.length) box.querySelector('.fsp-none')?.remove();
  }
}

export function installSeparations(ws: Workspace): void {
  let panel: SeparationsPanel | null = null;
  const make = () => (panel ??= new SeparationsPanel(ws));
  ws.registerPanel({ id: 'separations', label: 'Separations', group: 'preflight', column: 'dock', order: 30, create: () => make().root, onShow: () => panel?.onShow() });
  ws.addCommands(
    { id: 'ui.separations', label: 'Separations Preview', keys: ['Shift+F6'], run: () => ws.showPanel('separations') },
    { id: 'ui.overprint', label: 'Overprint Preview', keys: ['Alt+Shift+Mod+Y'], checked: () => ws.viewport.opts.overprint, run: () => make().toggleOverprint() },
  );
  ws.addMenuItems({ menu: 'View', items: ['ui.overprint', 'ui.separations'], at: 'end' });
}
