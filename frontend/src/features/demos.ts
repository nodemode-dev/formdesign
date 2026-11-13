import { api } from '../engine/client';
import { editor, friendly } from '../editor/editor';
import { DEMOS, type DemoId } from '../ui/demos';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

export function installDemos(ws: Workspace): void {
  let busy: DemoId | null = null;
  const mark = (id: DemoId | null): void => {
    for (const n of Array.from(ws.root.querySelectorAll<HTMLElement>('[data-demo]'))) n.setAttribute('aria-busy', String(n.dataset.demo === id));
  };
  const open = async (id: DemoId): Promise<void> => {
    if (busy) { toast('A demo is already opening.'); return; }
    busy = id;
    mark(id);
    toast('Opening the demo as a new document…');
    try {
      await api(`/demos/${id}`, { method: 'POST' });
      editor.refreshMeta();
      toast('The demo is open. Save it to keep your changes.');
    } catch (err) {
      toast(`The demo could not open: ${friendly((err as Error).message)}`, 'error');
    } finally {
      busy = null;
      mark(null);
    }
  };
  ws.addCommands(
    ...DEMOS.map((d) => ({ id: d.command, label: d.title, group: 'window', description: `Opens the ${d.title.replace(/^an? /i, '')} demo as a new document, laid out by Make from its own material with placeholder pictures.`,
      when: 'To see what FormDesign makes from a little material; nothing open is changed.', mutates: true, run: () => open(d.id) })),
    { id: 'ui.demo.open', label: 'Open a Demo', group: 'window', description: 'Opens one of the three demos as a new document.', when: 'To open a demo by its id.', mutates: true,
      params: { id: { type: 'string', required: true, enum: DEMOS.map((d) => d.id), description: 'the demo to open' } },
      run: (p) => { const d = DEMOS.find((x) => x.id === p?.id); if (d) return open(d.id); toast('Choose a demo: magazine, event or report.', 'error'); return undefined; } },
  );
  ws.addMenuItems({ menu: 'Help', items: [{ label: 'Demos', items: DEMOS.map((d) => d.command) }], at: 'end' });
}
