import '../styles/document.css';
import { editor } from '../editor/editor';
import { api } from '../engine/client';
import { toast } from '../ui/toast';
import type { Feature, Workspace } from '../ui/workspace';
import { CORE_EXPORTS, ExportDialog, type ExportFormat } from './document/export';
import { InfoPanel } from './document/info';
import { NewDocumentDialog } from './document/new-doc';
import { PackageDialog } from './document/package';
import { PreferencesDialog } from './document/prefs';
import { PreflightPanel } from './document/preflight';
import { PublishDialog } from './document/publish';
import { DocumentSetupDialog } from './document/setup';

function lazy<T>(make: () => T): () => T {
  let v: T | null = null;
  return () => (v ??= make());
}

export const feature: Feature = {
  install(ws: Workspace) {
    const newDoc = lazy(() => new NewDocumentDialog(ws));
    const exporter = lazy(() => new ExportDialog());
    const publisher = lazy(() => new PublishDialog());
    const pack = lazy(() => new PackageDialog(ws));
    const setup = lazy(() => new DocumentSetupDialog(ws));
    const prefs = lazy(() => new PreferencesDialog());
    const has = () => editor.hasDoc;

    ws.addCommands(
      { id: 'ui.new', label: 'New Document…', keys: ['Mod+N'], run: () => newDoc().open() },
      { id: 'ui.export', label: 'Export…', keys: ['Mod+E'], enabled: has, run: () => void exporter().open() },
      { id: 'ui.exportWeb', label: 'Export Website…', enabled: has, run: () => void exporter().open('web') },
      { id: 'ui.publish', label: 'Publish Online…', enabled: has, run: () => void publisher().open() },
      { id: 'ui.prefs', label: 'Preferences…', keys: ['Mod+,'], run: () => prefs().open() },
      { id: 'ui.recover', label: 'Recover Unsaved Documents', run: () => void recover() },
    );
    const recover = async () => {
      try {
        const caps = await api<{ dataDir: string }>('/capabilities');
        const dir = `${caps.dataDir}/.cache/recovery`;
        const found = await ws.runCore('file.recovery.list', { dir }) as unknown[] | undefined;
        if (!found?.length) { toast('There are no unsaved documents to recover'); return; }
        await ws.runCore('file.recovery.open', { dir });
        editor.refreshMeta();
        toast(`Recovered ${found.length} document${found.length === 1 ? '' : 's'}; save them to keep them`);
      } catch (err) { toast((err as Error).message, 'error'); }
    };
    ws.override('file.new', () => newDoc().open());
    for (const [id, fmt] of Object.entries(CORE_EXPORTS)) ws.override(id, () => void exporter().open(fmt as ExportFormat));
    ws.override('file.package', () => void pack().open());
    ws.override('layout.documentSetup', () => void setup().open());
    ws.addMenuItems({ menu: 'File', items: ['ui.export', 'ui.publish'], at: 'start' });
    ws.addMenuItems({ menu: 'File', items: ['ui.recover'], at: 'end' });
    ws.addMenuItems({ menu: 'Edit', items: ['ui.prefs'], at: 'end' });

    let preflight: PreflightPanel | null = null;
    let info: InfoPanel | null = null;
    ws.registerPanel({
      id: 'preflight', label: 'Preflight', group: 'preflight', column: 'dock', order: 10,
      create: () => (preflight ??= new PreflightPanel(ws)).root,
      onShow: () => preflight?.onShow(),
    });
    ws.registerPanel({
      id: 'info', label: 'Info', group: 'preflight', column: 'dock', order: 20,
      create: () => (info ??= new InfoPanel(ws)).root,
      onShow: () => info?.onShow(),
    });
  },
};
