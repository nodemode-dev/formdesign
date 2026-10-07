import '../styles/reader.css';
import { api } from '../engine/client';
import { friendly } from '../editor/editor';
import { delayedIndicator, q } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Feature, Workspace } from '../ui/workspace';
import { ReaderView, type PdfInfo } from './reader/view';

export const feature: Feature = {
  install(ws: Workspace) {
    let view: ReaderView | null = null;
    const get = (): ReaderView => {
      if (!view) {
        view = new ReaderView(q(ws.root, '.cv-stage-col'));
        view.onExtract = (info) => void extract(info);
        view.onClose = () => ws.setHint('');
      }
      return view;
    };

    const openFile = async (file: File): Promise<void> => {
      const fd = new FormData();
      fd.append('file', file, file.name);
      const done = delayedIndicator(() => ws.setHint(`Opening ${file.name}`), () => ws.setHint(''));
      try {
        const info = await api<PdfInfo>('/reader/upload', { method: 'POST', body: fd });
        await get().open(info);
        ws.setHint(`${info.name} · ${info.pages} pages`);
      } catch (err) {
        toast(friendly((err as Error).message), 'error');
      } finally { done(); }
    };

    const pick = async (): Promise<void> => {
      const [f] = await ws.pickFiles('.pdf,application/pdf', false);
      if (f) await openFile(f);
    };

    const extract = async (info: PdfInfo): Promise<void> => {
      const done = delayedIndicator(() => ws.setHint('Reading the PDF into a content pack'), () => undefined);
      try {
        const s = await api<{ pack: string; title?: string; facts?: { words?: number; images?: number } }>(`/reader/${info.id}/extract`, { json: { brief: '' } });
        get().close();
        window.dispatchEvent(new CustomEvent('formdes:make-pack', { detail: { pack: s.pack, title: s.title } }));
        ws.showPanel('make');
        toast(`Extracted ${s.facts?.words ?? 0} words and ${s.facts?.images ?? 0} images from the PDF`);
      } catch (err) {
        toast(friendly((err as Error).message), 'error');
      } finally { done(); ws.setHint(''); }
    };

    ws.addCommands(
      { id: 'ui.reader', label: 'Open PDF in Reader…', keys: ['Alt+Mod+O'], run: () => void pick() },
      { id: 'ui.reader.file', label: 'Open PDF…', run: () => void pick() },
      { id: 'ui.reader.close', label: 'Close the Reader', enabled: () => !!view?.isOpen, run: () => view?.close() },
    );
    ws.addMenuItems({ menu: 'File', items: ['ui.reader'], at: 'start' });
    ws.onDropFiles((files) => {
      if (files.length === 1 && /\.pdf$/i.test(files[0].name) && view?.isOpen) { void openFile(files[0]); return true; }
      return false;
    });
  },
};
