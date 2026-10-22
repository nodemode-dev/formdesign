import type { Commands } from './commands';
import type { MenuDef, MenuItem as MenuSpec } from './menu';
import { t } from './i18n';

export const wantsNativeMenus = (): boolean => '__TAURI_INTERNALS__' in window && /Mac/.test(navigator.platform) && !menusInWindow();

export const MENUS_IN_WINDOW_KEY = 'formdesign.menusInWindow';
export function menusInWindow(): boolean {
  try { return localStorage.getItem(MENUS_IN_WINDOW_KEY) === '1'; } catch { return false; }
}

export function accelerator(combo: string | undefined): string | null {
  if (!combo) return null;
  const parts = combo.endsWith('++') ? [...combo.slice(0, -2).split('+'), 'Plus'] : combo.split('+');
  const names: Record<string, string> = { Mod: 'CmdOrCtrl', Alt: 'Alt', Shift: 'Shift', Ctrl: 'Ctrl', '=': '=', '-': '-', '[': '[', ']': ']', Delete: 'Delete', Backspace: 'Backspace', Escape: 'Escape', Enter: 'Enter', Tab: 'Tab', Space: 'Space', Plus: 'Plus' };
  const out: string[] = [];
  for (const p of parts) {
    if (names[p]) out.push(names[p]);
    else if (/^[A-Za-z0-9]$/.test(p)) out.push(p.toUpperCase());
    else if (/^F([1-9]|1[0-2])$/.test(p)) out.push(p);
    else return null;
  }
  return out.join('+');
}

type Api = typeof import('@tauri-apps/api/menu');

const TEXT_EDITS: Record<string, string> = { 'ui.undo': 'undo', 'ui.redo': 'redo', 'edit.cut': 'cut', 'edit.copy': 'copy', 'edit.paste': 'paste', 'edit.selectAll': 'select-all' };
const NOT_TEXT = new Set(['button', 'checkbox', 'radio', 'range', 'color', 'file', 'submit', 'reset', 'image']);

export function textFieldFocused(): boolean {
  const a = document.activeElement;
  if (a instanceof HTMLTextAreaElement) return !a.readOnly && !a.disabled;
  if (a instanceof HTMLInputElement) return !NOT_TEXT.has(a.type) && !a.readOnly && !a.disabled;
  return a instanceof HTMLElement && a.isContentEditable;
}

interface Live { id: string; text: string; item: { setEnabled(on: boolean): Promise<void>; setChecked?(on: boolean): Promise<void>; setText?(text: string): Promise<void> } }

export class NativeMenus {
  private items: Live[] = [];
  private building = false;
  private again = false;
  private stateTimer = 0;
  private needCore = false;

  constructor(private readonly commands: Commands, private readonly menus: () => MenuDef[], private readonly before: () => Promise<void> = () => Promise.resolve()) {
    for (const ev of ['pointerup', 'keyup', 'focusin', 'focusout'] as const) window.addEventListener(ev, () => { if (this.items.length) this.refresh(); }, { passive: true });
  }

  async build(): Promise<void> {
    if (this.building) { this.again = true; return; }
    this.building = true;
    try {
      const api: Api = await import('@tauri-apps/api/menu');
      const items: Live[] = [];
      const make = async (spec: MenuSpec): Promise<unknown> => {
        if (spec === '-') return api.PredefinedMenuItem.new({ item: 'Separator' });
        if (typeof spec === 'object') return api.Submenu.new({ text: t(spec.label), items: (await Promise.all(spec.items.map(make))).filter(Boolean) as never[] });
        const cmd = this.commands.get(spec);
        if (!cmd) return null;
        const text = t(this.commands.label(cmd)), acc = accelerator(cmd.keys?.[0]);
        const opts = { text, enabled: this.enabled(spec), action: () => {
          const edit = TEXT_EDITS[spec];
          if (edit && textFieldFocused()) void import('@tauri-apps/api/core').then((m) => m.invoke('text_edit', { action: edit })).catch(() => { document.execCommand(edit === 'select-all' ? 'selectAll' : edit); });
          else this.commands.run(spec);
          this.refresh();
        } };
        let item;
        if (cmd.checked) {
          const copts = { ...opts, checked: !!cmd.checked() };
          try { item = await api.CheckMenuItem.new(acc ? { ...copts, accelerator: acc } : copts); } catch { item = await api.CheckMenuItem.new(copts); }
        } else {
          try { item = await api.MenuItem.new(acc ? { ...opts, accelerator: acc } : opts); } catch { item = await api.MenuItem.new(opts); }
        }
        items.push({ id: spec, text, item });
        return item;
      };
      const top = [];
      top.push(await api.Submenu.new({ text: 'FormDesign', items: [
        await api.PredefinedMenuItem.new({ item: { About: { name: 'FormDesign' } }, text: t('About FormDesign') }),
        await api.PredefinedMenuItem.new({ item: 'Separator' }),
        ...(this.commands.get('ui.prefs') ? [await make('ui.prefs')] : []),
        await api.PredefinedMenuItem.new({ item: 'Separator' }),
        await api.PredefinedMenuItem.new({ item: 'Hide' }),
        await api.PredefinedMenuItem.new({ item: 'Quit' }),
      ] as never[] }));
      for (const def of this.menus()) {
        const list = typeof def.items === 'function' ? def.items() : def.items;
        top.push(await api.Submenu.new({ text: t(def.label), items: (await Promise.all(list.map(make))).filter(Boolean) as never[] }));
      }
      const menu = await api.Menu.new({ items: top as never[] });
      await menu.setAsAppMenu();
      this.items = items;
      document.documentElement.classList.add('has-native-menus');
    } finally {
      this.building = false;
      if (this.again) { this.again = false; void this.build(); }
    }
  }

  private enabled(id: string): boolean {
    if (TEXT_EDITS[id] && textFieldFocused()) return true;
    const cmd = this.commands.get(id);
    return !!cmd && this.commands.enabled(cmd);
  }

  refresh(core = false): void {
    if (core) this.needCore = true;
    if (this.stateTimer) return;
    this.stateTimer = window.setTimeout(() => {
      this.stateTimer = 0;
      const ask = this.needCore;
      this.needCore = false;
      void (ask ? this.before() : Promise.resolve()).then(() => {
        for (const live of this.items) {
          const cmd = this.commands.get(live.id);
          if (!cmd) continue;
          void live.item.setEnabled(this.enabled(live.id)).catch(() => undefined);
          if (cmd.checked && live.item.setChecked) void live.item.setChecked(!!cmd.checked()).catch(() => undefined);
          const text = t(this.commands.label(cmd));
          if (text !== live.text && live.item.setText) { live.text = text; void live.item.setText(text).catch(() => undefined); }
        }
      });
    }, 150);
  }
}
