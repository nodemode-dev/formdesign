import { inTauri, PREFERENCES as ALL_PREFERENCES, type Preference } from '../gpu/index';
import { el, q, setText } from '../ui/dom';
import type { SubMenu } from '../ui/menu';
import type { Workspace } from '../ui/workspace';

// View > Renderer: Auto, WebGPU, WebGL2 or Canvas 2D for the page canvas. The choice is kept by the viewport in
// localStorage (formdesign.renderer); this module only registers the menu and shows the active path in the status bar.
export function installRenderer(ws: Workspace): void {
  const vp = ws.viewport;
  // The Native (desktop) path exists only inside the Tauri shell; a browser never lists it.
  const PREFERENCES = ALL_PREFERENCES.filter((p) => p.id !== 'native' || inTauri());
  const chip = el('<span class="fd-st-renderer"></span>');
  const status = q(ws.root, '.fd-status');
  status.insertBefore(chip, q(status, '.fd-st-version'));

  const sync = (): void => {
    const r = vp.renderer;
    setText(chip, r.name);
    chip.title = r.note ? `Renderer: ${r.name}. ${r.note}.` : `Renderer: ${r.name}`;
  };
  vp.onRenderer(sync);
  sync();

  const label = (p: { id: Preference; label: string }): string => (p.id === 'auto' ? `Auto (${vp.renderer.name})` : p.label);
  ws.addCommands(...PREFERENCES.map((p) => ({
    id: `ui.renderer.${p.id}`,
    label: () => label(p),
    checked: () => vp.renderer.pref === p.id,
    run: () => { void vp.setRenderer(p.id); },
  })));
  const sub: SubMenu = { label: 'Renderer', items: PREFERENCES.map((p) => `ui.renderer.${p.id}`) };
  ws.addMenuItems({ menu: 'View', items: [sub], at: 'end' });
}
