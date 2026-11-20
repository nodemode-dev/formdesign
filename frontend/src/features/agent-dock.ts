import { Dock } from '../dock/dock';
import { formdesignDock, type ChatView } from '../dock-adapter';
import { editor } from '../editor/editor';
import { formatKeys } from '../ui/commands';
import { setClass } from '../ui/dom';
import type { Workspace } from '../ui/workspace';

export type { ChatView } from '../dock-adapter';

const TOGGLE = 'ui.terminal.toggle';

let dock: Dock | null = null;
let makeChat: (() => ChatView) | null = null;

export function documentContext(ws: Workspace): string {
  const parts: string[] = [];
  const doc = editor.active === null ? null : editor.docs[editor.active];
  if (doc) parts.push(`open document: ${doc.title || 'untitled'}`);
  parts.push(`page ${ws.viewport.currentPage() + 1} is in view`);
  const sel = editor.selection?.items ?? [];
  if (sel.length) parts.push(`selected item ids: ${sel.slice(0, 40).join(', ')}`);
  return parts.join('; ');
}

export function registerChat(make: () => ChatView): void { makeChat = make; }

export async function openChat(): Promise<void> { await dock?.show('chat'); }

export function installTerminal(ws: Workspace): void {
  const main = ws.root.querySelector<HTMLElement>('.cv-main');
  if (!main) return;
  const d: Dock = new Dock(formdesignDock({
    context: () => documentContext(ws),
    chat: () => makeChat?.() ?? null,
    handoff: (session) => void d.claude(undefined, session),
    suggest: (text) => {
      const line = d.root.querySelector<HTMLInputElement>('.ft-dock-ask input');
      if (!line) return;
      line.value = text;
      line.focus();
    },
    openSettings: () => ws.showPanel('connect'),
    isChord: (e) => ws.commands.forEvent(e)?.id === TOGGLE,
    chordLabel: () => {
      const key = ws.commands.get(TOGGLE)?.keys?.[0];
      return key ? formatKeys(key) : '';
    },
  }));
  dock = d;
  const lights: Array<['claude' | 'shell', HTMLElement | null]> = [
    ['claude', ws.root.querySelector<HTMLElement>('.cv-titlebar [data-cmd="claude"]')],
    ['shell', ws.root.querySelector<HTMLElement>('.cv-titlebar [data-cmd="terminal"]')],
  ];
  d.onTab = () => {
    for (const [kind, button] of lights) {
      if (!button) continue;
      const on = d.isShowing(kind);
      setClass(button, 'is-active', on);
      if (button.getAttribute('aria-pressed') !== String(on)) button.setAttribute('aria-pressed', String(on));
    }
  };
  let wasOpen = d.isOpen();
  d.onChange = () => {
    if (d.isOpen() === wasOpen) return;
    wasOpen = d.isOpen();
    window.dispatchEvent(new CustomEvent('fd-dock', { detail: { open: wasOpen } }));
  };
  d.mount(main);
  ws.addCommands(
    { id: TOGGLE, label: 'Show Terminal', keys: ['Mod+J'], checked: () => d.isOpen(), run: () => d.toggle() },
    { id: 'ui.terminal.new', label: 'New Terminal', run: () => d.newTerminal() },
    { id: 'ui.terminal.claude', label: 'Claude Code', keys: ['Shift+Mod+J'], run: async () => { if (!d.isOpen()) d.toggle(true, 'none'); await d.claude(); } },
  );
  ws.addMenuItems({ menu: 'Window', items: [TOGGLE, 'ui.terminal.new', 'ui.terminal.claude', 'ui.claude'], at: 'end' });
  ws.addMenuItems({ menu: 'Help', items: ['ui.claude', 'ui.terminal.claude'], at: 'end' });
}
