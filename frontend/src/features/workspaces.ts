import { api } from '../engine/client';
import { confirmAction, Dialog, promptName } from '../ui/dialogs';
import type { PanelLayout } from '../ui/dock';
import { enhanceSelect } from '../ui/dropdown';
import { el, q, setDisabled } from '../ui/dom';
import type { SubMenu } from '../ui/menu';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Spec { name: string; left?: string[]; side: string[]; dock: string[]; open: string[]; hidden?: string[]; tabs?: Record<string, string> }
const LEFT_DEFAULT = ['pages', 'make', 'terminal'];
interface Saved { name: string; layout: PanelLayout }

const BUILTIN: Spec[] = [
  { name: 'Essentials', side: ['props', 'typeextra', 'review', 'interactive'], dock: ['make', 'swatches', 'pages', 'preflight', 'xml', 'library'], open: ['props', 'make', 'pages'], tabs: { props: 'properties', make: 'make', pages: 'pages' } },
  { name: 'Advanced', side: ['props', 'typeextra', 'review', 'interactive'], dock: ['pages', 'swatches', 'make', 'preflight', 'library', 'xml'], open: ['props', 'pages', 'swatches', 'preflight'], tabs: { props: 'properties', pages: 'layers', swatches: 'styles', preflight: 'preflight' } },
  { name: 'Book', side: ['props', 'review'], dock: ['pages', 'swatches', 'preflight', 'make'], open: ['props', 'pages', 'swatches', 'preflight'], hidden: ['typeextra', 'interactive', 'xml', 'library'], tabs: { props: 'paragraph', pages: 'pages', swatches: 'styles', preflight: 'preflight' } },
  { name: 'Digital Publishing', side: ['props', 'interactive', 'typeextra'], dock: ['pages', 'library', 'swatches', 'make', 'preflight', 'xml'], open: ['props', 'interactive', 'pages', 'library'], hidden: ['review'], tabs: { props: 'properties', interactive: 'transitions', pages: 'pages' } },
  { name: 'Interactive for PDF', side: ['props', 'interactive'], dock: ['pages', 'swatches', 'preflight', 'make', 'library'], open: ['props', 'interactive', 'pages', 'swatches'], hidden: ['typeextra', 'review', 'xml'], tabs: { props: 'properties', interactive: 'buttons', pages: 'pages', swatches: 'swatches' } },
  { name: 'Printing and Proofing', side: ['props', 'review'], dock: ['preflight', 'pages', 'swatches', 'make'], open: ['props', 'preflight', 'pages', 'swatches'], hidden: ['typeextra', 'interactive', 'xml', 'library'], tabs: { props: 'properties', preflight: 'separations', pages: 'links', swatches: 'swatches' } },
  { name: 'Typography', side: ['props', 'typeextra', 'review'], dock: ['swatches', 'pages', 'make', 'preflight'], open: ['props', 'typeextra', 'swatches'], hidden: ['interactive', 'xml', 'library'], tabs: { props: 'character', typeextra: 'glyphs', swatches: 'styles' } },
];

const KEY = 'formdes.workspace';
const cmdId = (name: string) => `ui.workspace.${name.toLowerCase().replace(/[^a-z0-9]+/g, '-')}`;

export function installWorkspaces(ws: Workspace): void {
  let saved: Saved[] = [];
  let current = 'Essentials';
  try { current = localStorage.getItem(KEY) || 'Essentials'; } catch { }
  const sub: SubMenu = { label: 'Workspace', items: [] };
  const select = el<HTMLSelectElement>('<select class="cv-tt-select fd-ws-select" aria-label="Workspace" title="Workspace"></select>');

  const layoutOf = (name: string): PanelLayout | null => {
    const user = saved.find((s) => s.name === name);
    if (user) return user.layout;
    const b = BUILTIN.find((s) => s.name === name);
    if (!b) return null;
    const groups: PanelLayout['groups'] = {};
    for (const gid of Object.keys(ws.layout().groups)) if (!/^g[0-9a-z]+$/.test(gid)) groups[gid] = { tab: b.tabs?.[gid], min: !b.open.includes(gid) };
    const left = (b.left ?? LEFT_DEFAULT).filter((g) => !(b.hidden ?? []).includes(g));
    return { dock: { left, side: b.side.filter((g) => !left.includes(g)), dock: b.dock.filter((g) => !left.includes(g)), hidden: b.hidden ?? [], heights: {}, custom: [], width: null, collapsed: [] }, groups, homes: {} };
  };

  const remember = (name: string): void => { current = name; try { localStorage.setItem(KEY, name); } catch { } if (select.value !== name) select.value = name; };

  const use = (name: string): void => {
    const l = layoutOf(name);
    if (!l) { toast(`No workspace named ${name}`, 'error'); return; }
    ws.applyLayout(l);
    remember(name);
  };

  const sync = (): void => {
    for (const c of ws.commands.all()) if (c.id.startsWith('ui.workspace.') && !['ui.workspace.new', 'ui.workspace.delete', 'ui.workspace.reset'].includes(c.id) && ![...BUILTIN, ...saved].some((s) => cmdId(s.name) === c.id)) ws.commands.remove(c.id);
    for (const s of [...BUILTIN, ...saved]) {
      const id = cmdId(s.name);
      if (!ws.commands.get(id)) ws.commands.add({ id, label: s.name, checked: () => current === s.name, run: () => use(s.name) });
    }
    sub.items = [...BUILTIN.map((s) => cmdId(s.name)), ...(saved.length ? ['-', ...saved.map((s) => cmdId(s.name))] : []), '-', 'ui.workspace.new', 'ui.workspace.delete', 'ui.workspace.reset'];
    const key = JSON.stringify(saved.map((s) => s.name));
    if (select.dataset.key !== key) {
      select.dataset.key = key;
      const opt = (n: string) => new Option(n, n);
      const built = document.createElement('optgroup');
      built.label = 'Built in';
      built.append(...BUILTIN.map((s) => opt(s.name)));
      const mine = document.createElement('optgroup');
      mine.label = 'Yours';
      mine.append(...saved.map((s) => opt(s.name)));
      select.replaceChildren(built, ...(saved.length ? [mine] : []));
    }
    if (!layoutOf(current)) current = 'Essentials';
    select.value = current;
  };

  const load = async (): Promise<void> => {
    try { saved = (await api<{ items: Saved[] }>('/prefs/workspaces')).items ?? []; } catch { saved = []; }
    sync();
  };

  const create = async (): Promise<void> => {
    const name = (await promptName('New Workspace', current === 'Essentials' ? 'My Workspace' : `${current} copy`, 'Save'))?.trim();
    if (!name) return;
    if (BUILTIN.some((b) => b.name.toLowerCase() === name.toLowerCase())) { toast(`${name} is built in; choose another name.`, 'error'); return; }
    if (saved.some((s) => s.name.toLowerCase() === name.toLowerCase()) && !(await confirmAction('Replace Workspace', `Replace the workspace ${name} with the current arrangement?`, 'Replace'))) return;
    try {
      const r = await api<Saved>('/prefs/workspaces', { json: { name, layout: ws.layout() } });
      saved = [...saved.filter((s) => s.name.toLowerCase() !== name.toLowerCase()), r].sort((a, b) => a.name.localeCompare(b.name));
      sync();
      remember(r.name);
      toast(`Saved the workspace ${r.name}`);
    } catch (err) { toast((err as Error).message, 'error'); }
  };

  const remove = (): void => {
    if (!saved.length) { toast('There are no workspaces of yours to delete; the built-in ones stay.'); return; }
    const d = new Dialog('Delete Workspace', 400);
    const pick = el<HTMLSelectElement>('<select class="cv-select fd-ws-del" aria-label="Workspace to delete"></select>');
    pick.append(...saved.map((s) => new Option(s.name, s.name)));
    if (saved.some((s) => s.name === current)) pick.value = current;
    d.body.append(el('<label class="fd-field"><span>Workspace</span></label>'));
    q(d.body, '.fd-field').append(pick);
    enhanceSelect(pick);
    d.button('Cancel', '', () => d.close());
    const ok = d.button('Delete', 'danger', async () => {
      const name = pick.value;
      setDisabled(ok, true);
      try {
        await api(`/prefs/workspaces?name=${encodeURIComponent(name)}`, { method: 'DELETE' });
        saved = saved.filter((s) => s.name !== name);
        if (current === name) remember('Essentials');
        sync();
        d.close();
      } catch (err) { toast((err as Error).message, 'error'); setDisabled(ok, false); }
    });
    d.onClose = () => d.scrim.remove();
    d.open(pick);
  };

  ws.addCommands(
    { id: 'ui.workspace.new', label: 'New Workspace…', run: () => void create() },
    { id: 'ui.workspace.delete', label: 'Delete Workspace…', run: remove },
    { id: 'ui.workspace.reset', label: () => `Reset ${current}`, run: () => use(current) },
  );
  ws.addMenuItems({ menu: 'Window', items: [sub], at: 'top' });
  const host = q(ws.root, '.fd-title-tools'), before = host.querySelector('[data-cmd="settings"]');
  host.insertBefore(select, before);
  enhanceSelect(select);
  select.addEventListener('change', () => { if (select.value && select.value !== current) use(select.value); });
  sync();
  void load();
}
