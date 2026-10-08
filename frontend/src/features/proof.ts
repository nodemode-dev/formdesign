import { editor } from '../editor/editor';
import { api } from '../engine/client';
import { Dialog } from '../ui/dialogs';
import { enhanceSelect } from '../ui/dropdown';
import { el, q, setText } from '../ui/dom';
import type { SubMenu } from '../ui/menu';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Setup { target: string; intent: string; simulatePaper: boolean }
interface Profile { name: string; kind: string; builtin: boolean }

const KEY = 'formdes.proof';
const PRESETS: Array<[string, string, string]> = [
  ['working', 'workingCmyk', 'Working CMYK'], ['srgb', 'srgb', 'Internet Standard RGB (sRGB)'], ['mac', 'legacyMacRgb', 'Legacy Macintosh RGB'],
  ['monitor', 'monitorRgb', 'Monitor RGB'], ['protanopia', 'protanopia', 'Color Blindness, Protanopia'], ['deuteranopia', 'deuteranopia', 'Color Blindness, Deuteranopia'],
];
const INTENTS: Array<[string, string]> = [['perceptual', 'Perceptual'], ['relative', 'Relative Colorimetric'], ['saturation', 'Saturation'], ['absolute', 'Absolute Colorimetric']];

export function installProof(ws: Workspace): void {
  let setup: Setup = { target: 'workingCmyk', intent: 'relative', simulatePaper: false };
  try { const s = JSON.parse(localStorage.getItem(KEY) ?? 'null'); if (s && typeof s.target === 'string') setup = { target: s.target, intent: typeof s.intent === 'string' ? s.intent : 'relative', simulatePaper: !!s.simulatePaper }; } catch { }
  const chip = el('<span class="fd-st-proof"></span>');
  const status = q(ws.root, '.fd-status');
  status.insertBefore(chip, q(status, '.fd-st-version'));

  const label = (s: Setup): string => PRESETS.find((p) => p[1] === s.target)?.[2] ?? s.target.replace(/^cmyk:/, '');
  const sync = (): void => {
    const pr = ws.viewport.opts.proof;
    setText(chip, pr ? `Proof: ${label(pr)}${pr.simulatePaper && /cmyk/i.test(pr.target) ? ', paper' : ''}` : '');
  };
  const show = (on: boolean): void => {
    ws.viewport.opts.proof = on ? { ...setup } : null;
    ws.viewport.invalidateAll();
    sync();
  };
  const choose = (next: Setup): void => {
    setup = next;
    try { localStorage.setItem(KEY, JSON.stringify(setup)); } catch { }
    show(true);
  };

  const custom = async (): Promise<void> => {
    let profiles: Profile[] = [];
    try { profiles = (await editor.exec<{ profiles: Profile[] }>('color.settings', {}, true))?.profiles ?? []; } catch { profiles = []; }
    const d = new Dialog('Customize Proof Condition', 460);
    d.body.append(el(`<div class="fd-proof">
      <label class="fd-field"><span>Device to simulate</span><select class="cv-select fd-proof-target" aria-label="Device to simulate"></select></label>
      <label class="fd-field"><span>Rendering intent</span><select class="cv-select fd-proof-intent" aria-label="Rendering intent"></select></label>
      <label class="frv-check"><input type="checkbox" class="fd-proof-paper"><span>Simulate Paper Color</span></label>
      <p class="fd-note">Paper colour shows the press's paper white and the ink's real black, as an absolute colorimetric proof does. Load an ICC profile from your printer to proof against their press.</p>
    </div>`));
    const tgt = q<HTMLSelectElement>(d.body, '.fd-proof-target'), intent = q<HTMLSelectElement>(d.body, '.fd-proof-intent'), paper = q<HTMLInputElement>(d.body, '.fd-proof-paper');
    const fill = (list: Profile[], want: string): void => {
      const cm = document.createElement('optgroup');
      cm.label = 'Presses (CMYK)';
      cm.append(new Option('Working CMYK', 'workingCmyk'), ...list.filter((p) => p.kind === 'cmyk').map((p) => new Option(p.name, `cmyk:${p.name}`)));
      const rgb = document.createElement('optgroup');
      rgb.label = 'Screens and vision';
      rgb.append(...PRESETS.slice(1).map(([, v, l]) => new Option(l, v)));
      tgt.replaceChildren(cm, rgb);
      tgt.value = [...tgt.options].some((o) => o.value === want) ? want : 'workingCmyk';
    };
    fill(profiles, setup.target);
    intent.append(...INTENTS.map(([v, l]) => new Option(l, v)));
    intent.value = setup.intent;
    paper.checked = setup.simulatePaper;
    enhanceSelect(tgt);
    enhanceSelect(intent);
    d.button('Load Profile…', 'ghost', async () => {
      const files = await ws.pickFiles('.icc,.icm', false);
      if (!files.length) return;
      try {
        const [path] = await ws.upload(files);
        const info = await api<Profile>('/prefs/profiles', { json: { path } });
        if (info.kind !== 'cmyk') { toast(`${info.name} is an RGB profile; proofing simulates a CMYK press.`, 'error'); return; }
        profiles = [...profiles.filter((p) => p.name !== info.name), info];
        fill(profiles, `cmyk:${info.name}`);
      } catch (err) { toast((err as Error).message, 'error'); }
    });
    d.button('Cancel', '', () => d.close());
    d.button('OK', 'primary', () => { choose({ target: tgt.value, intent: intent.value, simulatePaper: paper.checked }); d.close(); });
    d.onClose = () => d.scrim.remove();
    d.open(tgt);
  };

  ws.addCommands(
    { id: 'ui.proofColors', label: 'Proof Colors', checked: () => !!ws.viewport.opts.proof, enabled: () => editor.hasDoc, run: () => show(!ws.viewport.opts.proof) },
    ...PRESETS.map(([id, target, l]) => ({ id: `ui.proofSetup.${id}`, label: l, checked: () => setup.target === target, run: () => choose({ ...setup, target }) })),
    { id: 'ui.proofSetup.custom', label: 'Custom…', checked: () => !PRESETS.some((p) => p[1] === setup.target), run: () => void custom() },
    { id: 'ui.proofSetup.paper', label: 'Simulate Paper Color', checked: () => setup.simulatePaper, run: () => choose({ ...setup, simulatePaper: !setup.simulatePaper }) },
  );
  const sub: SubMenu = { label: 'Proof Setup', items: ['ui.proofSetup.working', 'ui.proofSetup.custom', '-', 'ui.proofSetup.srgb', 'ui.proofSetup.mac', 'ui.proofSetup.monitor', '-', 'ui.proofSetup.protanopia', 'ui.proofSetup.deuteranopia', '-', 'ui.proofSetup.paper'] };
  ws.addMenuItems({ menu: 'View', items: [sub, 'ui.proofColors'], at: 'end' });
  editor.on('docs', () => { if (!editor.hasDoc && ws.viewport.opts.proof) { ws.viewport.opts.proof = null; sync(); } });
}
