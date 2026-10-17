import { editor } from '../editor/editor';
import { t } from '../ui/i18n';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Artboard { index: number; name: string }

async function exportArtboards(): Promise<void> {
  const r = await editor.exec<{ artboards: Artboard[] }>('artboard.list', {}, true);
  const list = r?.artboards ?? [];
  if (!list.length) { toast(t('This document has no artboards: Object > Artboards > New Artboard makes one')); return; }
  for (const a of list) {
    const res = await fetch(`/api/core/artboard.png?artboard=${a.index}&scale=2`);
    if (!res.ok) { toast(`${a.name}: ${res.statusText}`, 'error'); continue; }
    const url = URL.createObjectURL(await res.blob());
    const link = document.createElement('a');
    link.href = url;
    link.download = `${a.name || `Artboard ${a.index + 1}`}.png`;
    link.click();
    window.setTimeout(() => URL.revokeObjectURL(url), 4000);
  }
}

async function exportArtboardsPdf(): Promise<void> {
  const r = await editor.exec<{ artboards: Artboard[] }>('artboard.list', {}, true);
  if (!r?.artboards?.length) { toast(t('This document has no artboards: Object > Artboards > New Artboard makes one')); return; }
  const out = await editor.exec<{ base64: string }>('artboard.exportPdf', {}, true);
  if (!out?.base64) return;
  const bin = atob(out.base64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const url = URL.createObjectURL(new Blob([bytes], { type: 'application/pdf' }));
  const link = document.createElement('a');
  link.href = url;
  link.download = `${editor.doc?.title || 'Untitled'} artboards.pdf`;
  link.click();
  window.setTimeout(() => URL.revokeObjectURL(url), 4000);
}

export function installArtboards(ws: Workspace): void {
  ws.addCommands({ id: 'ui.artboards.exportPng', label: 'Export Artboards as PNG...', enabled: () => !!editor.doc, run: () => void exportArtboards() });
  ws.addCommands({ id: 'ui.artboards.exportPdf', label: 'Export Artboards as PDF...', enabled: () => !!editor.doc, run: () => void exportArtboardsPdf() });
  ws.addMenuItems({ menu: 'File', items: ['ui.artboards.exportPng', 'ui.artboards.exportPdf'], at: 'end' });
}
