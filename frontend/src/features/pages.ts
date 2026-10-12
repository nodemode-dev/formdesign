import '../styles/pages.css';
import { editor, type PageInfo } from '../editor/editor';
import type { Command } from '../ui/commands';
import { confirmAction } from '../ui/dialogs';
import { toast } from '../ui/toast';
import type { Feature, Workspace } from '../ui/workspace';
import { FormDialog, LAYER_COLORS, WHERE, colorName, form } from './pages/dialogs';
import { LayersPanel } from './pages/layers-panel';
import { LinksPanel } from './pages/links-panel';
import { PagesPanel } from './pages/pages-panel';
import { PRESETS, inspectNow, intersects, onceLayout, pageSize, parentPrefix, parsePages, rangeText, round, wireGroupMenus, type InspectData, type InspectItem } from './pages/shared';

const STYLES: Array<[string, string]> = [['arabic', '1, 2, 3, 4…'], ['upperRoman', 'I, II, III, IV…'], ['lowerRoman', 'i, ii, iii, iv…'], ['upperLetters', 'A, B, C, D…'], ['lowerLetters', 'a, b, c, d…']];

function int(v: string | boolean | undefined, d: number): number { const n = Math.round(Number(v)); return Number.isFinite(n) ? n : d; }

function pagesWithContent(d: InspectData, abs: number[], pages: PageInfo[]): number[] {
  const out: number[] = [];
  for (const a of abs) {
    const p = pages[a];
    const sp = d.spreads.find((s) => s.index === p?.spread);
    const pg = sp?.pages.find((x) => x.index === p?.index);
    if (!sp || !pg) continue;
    const any = (list: InspectItem[]): boolean => list.some((it) => intersects(it.bounds, pg.bounds) || any(it.children ?? []));
    if (any(sp.items)) out.push(a);
  }
  return out;
}

function countOnLayer(d: InspectData, id: number): number {
  let n = 0;
  const walk = (list: InspectItem[]): void => { for (const it of list) { if (it.layer === id) n++; walk(it.children ?? []); } };
  for (const s of d.spreads) walk(s.items);
  return n;
}

export const feature: Feature = {
  install(ws: Workspace) {
    const pages = new PagesPanel(ws);
    const layers = new LayersPanel(ws);
    const links = new LinksPanel(ws);
    const has = (): boolean => editor.hasDoc && pages.pages.length > 0;
    const free = (...combos: string[]): string[] => combos.filter((c) => !ws.commands.forCombo(c));
    const menus = { pages: () => pagesMenu(), layers: () => layers.menu(), links: () => links.menu() };
    const wire = (): void => wireGroupMenus(ws, menus);

    const pagesMenu = (): string[] => ['pages.insert', 'pages.duplicateSpread', 'pages.delete', '-', 'pages.newParent', 'pages.editParents', 'pages.applyParent', '-', 'pages.overrideAll', 'pages.removeOverrides', 'pages.detachAll', '-', 'pages.move', 'pages.section', '-', 'pages.panelSmall', 'pages.panelMedium', 'pages.panelLarge'];

    const parentOptions = (): Array<[string, string]> => [['', '[None]'], ...pages.parentList.map((p) => [parentPrefix(p.label), p.label] as [string, string])];

    const insertAt = async (count: number, after: number, parent: string | null): Promise<number | null> => {
      const n = pages.pages.length;
      if (after < 0) {
        pages.expectRemap(n + count, (a) => (a < count ? null : a - count));
        const r = await editor.batch([{ command: 'layout.pages.insert', params: { count, after: 0, parent } }, { command: 'layout.pages.move', params: { from: 0, to: count } }]);
        if (r === undefined) return null;
        await onceLayout();
        return 0;
      }
      pages.expectRemap(n + count, (a) => (a <= after ? a : a <= after + count ? null : a - count));
      const r = await editor.exec('layout.pages.insert', { count, after, parent });
      if (r === undefined) return null;
      await onceLayout();
      return after + 1;
    };

    const insertDialog = async (): Promise<void> => {
      if (!has()) return;
      const list = pages.pages;
      const sel = pages.selectedPages();
      const at = list[sel[sel.length - 1] ?? 0];
      const d = form('insert', () => new FormDialog('Insert Pages', [
        { key: 'count', label: 'Pages', kind: 'number', min: 1, max: 500 },
        { key: 'where', label: 'Insert', kind: 'select' },
        { key: 'page', label: 'Page', kind: 'text' },
        { key: 'parent', label: 'Parent', kind: 'select' },
      ], 'Insert', 400));
      d.onInput = (v, dlg) => dlg.setOff('page', v.where === 'start' || v.where === 'end');
      const v = await d.ask({ values: { count: '1', where: 'after', page: at?.name ?? '', parent: at?.parent ?? '' }, options: { where: WHERE, parent: parentOptions() } });
      if (!v) return;
      const count = Math.max(1, Math.min(500, int(v.count, 1)));
      const parent = v.parent ? String(v.parent) : null;
      let after = list.length - 1;
      if (v.where === 'start') after = -1;
      else if (v.where !== 'end') {
        const hit = parsePages(String(v.page), list);
        if (!hit) { toast(`There is no page ${v.page}`, 'error'); return; }
        after = v.where === 'after' ? hit[0] : hit[0] - 1;
      }
      const first = await insertAt(count, after, parent);
      if (first !== null) pages.selectAbs([...Array(count).keys()].map((k) => first + k));
    };

    const insertOne = async (): Promise<void> => {
      if (!has()) return;
      const sel = pages.selectedPages();
      const after = sel[sel.length - 1] ?? pages.pages.length - 1;
      const first = await insertAt(1, after, pages.pages[after]?.parent ?? null);
      if (first === null) return;
      pages.selectAbs([first]);
      ws.viewport.showPage(first);
    };

    const deletePages = async (): Promise<void> => {
      if (!has()) return;
      const list = pages.pages;
      const sel = pages.selectedPages();
      if (!sel.length) return;
      if (sel.length >= list.length) { toast('A document keeps at least one page', 'error'); return; }
      const d = await inspectNow();
      const busy = d ? pagesWithContent(d, sel, list) : [];
      if (busy.length) {
        const what = busy.length === 1 ? `Page ${list[busy[0]].name} contains objects.` : `Pages ${rangeText(busy, list)} contain objects.`;
        const ok = await confirmAction(sel.length === 1 ? 'Delete Page' : 'Delete Pages', `${what} Delete ${sel.length === 1 ? 'it' : 'them'} anyway? You can undo this.`, 'Delete', true);
        if (!ok) return;
      }
      const keep = list.map((p) => p.abs).filter((a) => !sel.includes(a));
      pages.expectRemap(keep.length, (a) => keep[a] ?? null);
      const r = await editor.exec('layout.pages.delete', { pages: sel });
      if (r !== undefined) pages.selectAbs([]);
    };

    const duplicateSpread = async (): Promise<void> => {
      if (!has()) return;
      const list = pages.pages;
      const sel = pages.selectedPages();
      const spread = list[sel[0] ?? 0]?.spread ?? 0;
      const inSpread = list.filter((p) => p.spread === spread).map((p) => p.abs);
      const last = Math.max(...inSpread), n = inSpread.length;
      pages.expectRemap(list.length + n, (a) => (a <= last ? a : a - n));
      const r = await editor.exec('layout.pages.duplicateSpread', { spread });
      if (r === undefined) return;
      await onceLayout();
      pages.selectAbs(inSpread.map((a) => a + n));
    };

    const moveDialog = async (): Promise<void> => {
      if (!has()) return;
      const list = pages.pages;
      const sel = pages.selectedPages();
      const d = form('move', () => new FormDialog('Move Pages', [
        { key: 'pages', label: 'Move Pages', kind: 'text', placeholder: '2-3, 5' },
        { key: 'where', label: 'Destination', kind: 'select' },
        { key: 'page', label: 'Page', kind: 'text' },
      ], 'Move', 400));
      d.onInput = (v, dlg) => dlg.setOff('page', v.where === 'start' || v.where === 'end');
      const v = await d.ask({ values: { pages: rangeText(sel, list), where: 'after', page: list[list.length - 1]?.name ?? '' }, options: { where: WHERE } });
      if (!v) return;
      const moving = parsePages(String(v.pages), list);
      if (!moving) { toast(`Pages ${v.pages} are not in the document`, 'error'); return; }
      let gap = list.length;
      if (v.where === 'start') gap = 0;
      else if (v.where !== 'end') {
        const hit = parsePages(String(v.page), list);
        if (!hit) { toast(`There is no page ${v.page}`, 'error'); return; }
        gap = v.where === 'after' ? hit[0] + 1 : hit[0];
      }
      await pages.movePages(moving, gap);
    };

    const applyParentDialog = async (): Promise<void> => {
      if (!has()) return;
      const list = pages.pages;
      const sel = pages.selectedPages();
      const d = form('apply', () => new FormDialog('Apply Parent', [
        { key: 'parent', label: 'Apply Parent', kind: 'select' },
        { key: 'pages', label: 'To Pages', kind: 'text', placeholder: '2-3, 5' },
      ], 'Apply', 400));
      const cur = list[sel[0] ?? 0]?.parent ?? (pages.parentList[0] ? parentPrefix(pages.parentList[0].label) : '');
      const v = await d.ask({ values: { parent: cur, pages: rangeText(sel, list) }, options: { parent: parentOptions() } });
      if (!v) return;
      const target = parsePages(String(v.pages), list);
      if (!target) { toast(`Pages ${v.pages} are not in the document`, 'error'); return; }
      await editor.exec('layout.pages.applyParent', { pages: target, parent: v.parent ? String(v.parent) : null });
    };

    const newParentDialog = async (): Promise<void> => {
      if (!editor.hasDoc) return;
      const taken = new Set(pages.parentList.map((p) => parentPrefix(p.label)));
      const next = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ'.split('').find((c) => !taken.has(c)) ?? '';
      const d = form('parent', () => new FormDialog('New Parent', [
        { key: 'prefix', label: 'Prefix', kind: 'text' },
        { key: 'name', label: 'Name', kind: 'text' },
      ], 'Create', 380));
      const v = await d.ask({ values: { prefix: next, name: 'Parent' } });
      if (!v) return;
      const prefix = String(v.prefix).trim().slice(0, 4), name = String(v.name).trim() || 'Parent';
      if (prefix && taken.has(prefix)) { toast(`A parent with the prefix ${prefix} exists`, 'error'); return; }
      await editor.exec('layout.parents.new', prefix ? { prefix, name } : { name });
    };

    const sectionDialog = async (): Promise<void> => {
      if (!has()) return;
      const list = pages.pages;
      const sel = pages.selectedPages();
      const at = list[sel[0] ?? 0];
      const d = form('section', () => new FormDialog('Numbering & Section Options', [
        { key: 'page', label: 'Start Section at Page', kind: 'text' },
        { key: 'auto', label: 'Automatic Page Numbering', kind: 'check' },
        { key: 'start', label: 'Start Numbering at', kind: 'number', min: 1, max: 99999 },
        { key: 'style', label: 'Style', kind: 'select' },
        { key: 'prefix', label: 'Section Prefix', kind: 'text' },
        { key: 'include', label: 'Include Prefix when Numbering Pages', kind: 'check' },
        { key: 'marker', label: 'Section Marker', kind: 'text' },
      ], 'OK', 460));
      d.onInput = (v, dlg) => dlg.setOff('start', !!v.auto);
      const v = await d.ask({ values: { page: at?.name ?? '', auto: false, start: /^\d+$/.test(at?.name ?? '') ? String(at?.name) : '1', style: 'arabic', prefix: '', include: false, marker: '' }, options: { style: STYLES }, extra: { label: 'Remove Section', value: { remove: true } } });
      if (!v) return;
      const hit = parsePages(String(v.page), list);
      if (!hit) { toast(`There is no page ${v.page}`, 'error'); return; }
      const page = hit[0] + 1;
      if (v.remove === true) { await editor.exec('layout.section', { page, remove: true }); return; }
      await editor.exec('layout.section', { page, startNumber: v.auto ? null : Math.max(1, int(v.start, 1)), style: String(v.style), prefix: String(v.prefix), includePrefix: !!v.include, marker: String(v.marker) });
    };

    const perPage = async (command: string): Promise<void> => {
      if (!has()) return;
      const sel = pages.selectedPages();
      await editor.batch(sel.map((page) => ({ command, params: { page } })));
    };

    const setSize = async (w: number, h: number): Promise<void> => {
      if (!has()) return;
      const sel = pages.selectedPages();
      await editor.exec('layout.pageSize', { pages: sel.map((a) => a + 1), width: w, height: h });
    };

    const customSize = async (): Promise<void> => {
      if (!has()) return;
      const sel = pages.selectedPages();
      const p = pages.pages[sel[0] ?? 0];
      const [w, h] = p ? pageSize(p.rect) : [612, 792];
      const d = form('size', () => new FormDialog('Custom Page Size', [
        { key: 'w', label: 'Width (pt)', kind: 'number', min: 1, max: 15552, step: 0.01 },
        { key: 'h', label: 'Height (pt)', kind: 'number', min: 1, max: 15552, step: 0.01 },
      ], 'Apply', 360));
      const v = await d.ask({ values: { w: round(w), h: round(h) }, note: `Applies to ${sel.length === 1 ? `page ${p?.name ?? ''}` : `pages ${rangeText(sel, pages.pages)}`}.` });
      if (!v) return;
      const nw = Number(v.w), nh = Number(v.h);
      if (!(nw > 0 && nh > 0)) { toast('Enter a width and a height', 'error'); return; }
      await setSize(nw, nh);
    };

    const goToDialog = async (): Promise<void> => {
      if (!has()) return;
      const list = pages.pages;
      const d = form('goto', () => new FormDialog('Go to Page', [{ key: 'page', label: 'Page', kind: 'text' }], 'Go', 320));
      const cur = list[ws.viewport.currentPage()];
      const v = await d.ask({ values: { page: cur?.name ?? '' }, note: `${list[0]?.name ?? ''} to ${list[list.length - 1]?.name ?? ''}; +N for an absolute page.` });
      if (!v) return;
      const hit = parsePages(String(v.page), list);
      if (!hit) { toast(`There is no page ${v.page}`, 'error'); return; }
      ws.viewport.fitPage(hit[0]);
    };

    const stepSpread = (dir: number): void => {
      const lay = editor.layout;
      if (!lay?.slots.length) return;
      const i = Math.max(0, Math.min(lay.slots.length - 1, ws.viewport.currentSpread() + dir));
      ws.viewport.fitSpread(i);
    };

    const layerOptions = async (): Promise<void> => {
      const l = layers.activeLayerInfo;
      if (!l) return;
      const d = form('layer', () => new FormDialog('Layer Options', [
        { key: 'name', label: 'Name', kind: 'text' },
        { key: 'color', label: 'Color', kind: 'select' },
        { key: 'visible', label: 'Show Layer', kind: 'check' },
        { key: 'locked', label: 'Lock Layer', kind: 'check' },
        { key: 'printable', label: 'Print Layer', kind: 'check' },
      ], 'OK', 380));
      const name = colorName(l.color);
      const opts: Array<[string, string]> = LAYER_COLORS.map(([n]) => [n, n]);
      if (name === 'Custom') opts.unshift(['Custom', 'Custom']);
      const v = await d.ask({ values: { name: l.name, color: name, visible: l.visible, locked: l.locked, printable: l.printable }, options: { color: opts } });
      if (!v) return;
      const p: Record<string, unknown> = { id: l.id, name: String(v.name).trim() || l.name, visible: !!v.visible, locked: !!v.locked, printable: !!v.printable };
      const c = LAYER_COLORS.find(([n]) => n === v.color);
      if (c) p.color = c[1];
      await editor.exec('layer.set', p);
    };

    const deleteLayer = async (): Promise<void> => {
      const l = layers.activeLayerInfo;
      if (!l || layers.layers.length <= 1) return;
      const d = await inspectNow();
      const n = d ? countOnLayer(d, l.id) : 0;
      if (n && !(await confirmAction('Delete Layer', `${l.name} holds ${n} ${n === 1 ? 'object' : 'objects'}. Delete the layer and its objects? You can undo this.`, 'Delete', true))) return;
      await editor.exec('layer.delete', { id: l.id });
    };

    const mergeDown = async (): Promise<void> => {
      const list = layers.layers, i = list.findIndex((x) => x.id === layers.activeLayer);
      const below = list[i + 1];
      if (i < 0 || !below) return;
      await editor.exec('layer.merge', { ids: [below.id, list[i].id], into: below.id });
    };

    const pickedLink = (): boolean => !!links.pickedLink;
    const needLink = (run: () => void) => (): void => {
      if (links.pickedLink) { run(); return; }
      ws.showPanel('links');
      toast('Pick a link in the Links panel first');
    };
    const layerReady = (): boolean => editor.hasDoc && !!layers.activeLayerInfo;
    const sizeCmds: Command[] = PRESETS.map((p) => ({
      id: `pages.size.${p.id}`, label: p.label, enabled: has,
      checked: () => { const a = pages.selectedPages()[0]; const pg = pages.pages[a ?? -1]; if (!pg) return false; const [w, h] = pageSize(pg.rect); return Math.abs(w - p.w) < 0.6 && Math.abs(h - p.h) < 0.6; },
      run: () => void setSize(p.w, p.h),
    }));

    ws.addCommands(
      { id: 'pages.insert', label: 'Insert Pages…', enabled: has, run: () => void insertDialog() },
      { id: 'pages.insertOne', label: 'Insert Page', enabled: has, run: () => void insertOne() },
      { id: 'pages.delete', label: () => (pages.selectedPages().length > 1 ? 'Delete Pages' : 'Delete Page'), enabled: () => has() && pages.selectedPages().length < pages.pages.length, run: () => void deletePages() },
      { id: 'pages.duplicateSpread', label: 'Duplicate Spread', enabled: has, run: () => void duplicateSpread() },
      { id: 'pages.newParent', label: 'New Parent…', enabled: () => editor.hasDoc, run: () => void newParentDialog() },
      { id: 'pages.editParents', label: 'Edit Parents', enabled: () => editor.hasDoc, checked: () => pages.editingParents, run: () => { if (pages.editingParents) void editor.exec('layout.parents.edit', { on: false }).then(() => onceLayout()).then(() => ws.viewport.fitPage(0)); else void pages.editParent(0); } },
      { id: 'pages.applyParent', label: 'Apply Parent to Pages…', enabled: has, run: () => void applyParentDialog() },
      { id: 'pages.overrideAll', label: 'Override All Parent Page Items', enabled: has, run: () => void perPage('layout.overrideParentItems') },
      { id: 'pages.removeOverrides', label: 'Remove All Local Overrides', enabled: has, run: () => void perPage('layout.removeOverrides') },
      { id: 'pages.detachAll', label: 'Detach All Objects from Parent', enabled: has, run: () => void perPage('layout.detachAll') },
      { id: 'pages.move', label: 'Move Pages…', enabled: has, run: () => void moveDialog() },
      { id: 'pages.section', label: 'Numbering & Section Options…', enabled: has, run: () => void sectionDialog() },
      { id: 'pages.panelSmall', label: 'Small Thumbnails', checked: () => pages.thumbSize === 'small', run: () => pages.setThumbSize('small') },
      { id: 'pages.panelMedium', label: 'Medium Thumbnails', checked: () => pages.thumbSize === 'medium', run: () => pages.setThumbSize('medium') },
      { id: 'pages.panelLarge', label: 'Large Thumbnails', checked: () => pages.thumbSize === 'large', run: () => pages.setThumbSize('large') },
      ...sizeCmds,
      { id: 'pages.size.custom', label: 'Custom…', enabled: has, run: () => void customSize() },
      { id: 'pages.first', label: 'First Page', keys: free('Shift+Mod+PageUp'), enabled: has, run: () => ws.viewport.showPage(0) },
      { id: 'pages.last', label: 'Last Page', keys: free('Shift+Mod+PageDown'), enabled: has, run: () => ws.viewport.showPage(pages.pages.length - 1) },
      { id: 'pages.prevSpread', label: 'Previous Spread', keys: free('Alt+PageUp'), enabled: has, run: () => stepSpread(-1) },
      { id: 'pages.nextSpread', label: 'Next Spread', keys: free('Alt+PageDown'), enabled: has, run: () => stepSpread(1) },
      { id: 'pages.goTo', label: 'Go to Page…', keys: free('Mod+J'), enabled: has, run: () => void goToDialog() },
      { id: 'layers.new', label: 'New Layer', enabled: () => editor.hasDoc, run: () => void editor.exec('layer.new', {}) },
      { id: 'layers.delete', label: () => `Delete Layer${layers.activeLayerInfo ? ` "${layers.activeLayerInfo.name}"` : ''}`, enabled: () => layerReady() && layers.layers.length > 1, run: () => void deleteLayer() },
      { id: 'layers.options', label: () => `Layer Options${layers.activeLayerInfo ? ` for "${layers.activeLayerInfo.name}"` : ''}…`, enabled: layerReady, run: () => void layerOptions() },
      { id: 'layers.selectItems', label: 'Select Items on Layer', enabled: layerReady, run: () => void editor.exec('layer.selectItems', { id: layers.activeLayer }) },
      { id: 'layers.moveToActive', label: 'Move Selection to Active Layer', enabled: () => layerReady() && (editor.selection?.items.length ?? 0) > 0, run: () => void editor.exec('object.setLayer', { layer: layers.activeLayer, ids: editor.selection?.items ?? [] }) },
      { id: 'layers.hideOthers', label: 'Hide Others', enabled: layerReady, run: () => void editor.exec('layer.others', { id: layers.activeLayer, hide: true }) },
      { id: 'layers.lockOthers', label: 'Lock Others', enabled: layerReady, run: () => void editor.exec('layer.others', { id: layers.activeLayer, lock: true }) },
      { id: 'layers.showAll', label: 'Show All Layers', enabled: layerReady, run: () => void editor.exec('layer.others', { id: layers.activeLayer, show: true }) },
      { id: 'layers.unlockAll', label: 'Unlock All Layers', enabled: layerReady, run: () => void editor.exec('layer.others', { id: layers.activeLayer, unlock: true }) },
      { id: 'layers.mergeDown', label: 'Merge with Layer Below', enabled: () => { const l = layers.layers; const i = l.findIndex((x) => x.id === layers.activeLayer); return layerReady() && i >= 0 && i < l.length - 1; }, run: () => void mergeDown() },
      { id: 'layers.deleteUnused', label: 'Delete Unused Layers', enabled: () => layerReady() && layers.layers.length > 1, run: () => void editor.exec('layer.deleteUnused', {}) },
      { id: 'links.relinkPicked', label: 'Relink…', enabled: pickedLink, run: needLink(() => void links.relink()) },
      { id: 'links.goToPicked', label: 'Go to Link', enabled: pickedLink, run: needLink(() => void links.goTo()) },
      { id: 'links.updatePicked', label: 'Update Link', enabled: pickedLink, run: needLink(() => void links.update(false)) },
      { id: 'links.updateAll', label: 'Update All Links', enabled: () => editor.hasDoc, run: () => void links.update(true) },
      { id: 'links.embedPicked', label: 'Embed Link', enabled: () => !!links.pickedLink && links.pickedLink.status !== 'embedded', run: needLink(() => void links.embed()) },
    );

    const route: Record<string, string> = {
      'layout.pages.insert': 'pages.insert', 'layout.pages.delete': 'pages.delete', 'layout.pages.move': 'pages.move',
      'layout.pages.duplicateSpread': 'pages.duplicateSpread', 'layout.pages.applyParent': 'pages.applyParent', 'layout.parents.new': 'pages.newParent',
      'layout.section': 'pages.section', 'layout.overrideParentItems': 'pages.overrideAll', 'layout.removeOverrides': 'pages.removeOverrides',
      'layout.detachAll': 'pages.detachAll', 'layout.parents.edit': 'pages.editParents',
    };
    for (const [core, mine] of Object.entries(route)) ws.override(core, () => { ws.commands.run(mine); });
    ws.override('links.relink', needLink(() => void links.relink()));
    ws.override('links.goTo', needLink(() => void links.goTo()));
    ws.override('links.embed', needLink(() => void links.embed()));

    ws.addMenuItems({ menu: 'Layout', at: 'start', items: ['pages.first', 'ui.prevPage', 'ui.nextPage', 'pages.last', '-', 'pages.prevSpread', 'pages.nextSpread', '-', 'pages.goTo', '-', 'pages.editParents'] });

    ws.registerPanel({ id: 'pages', label: 'Pages', group: 'pages', column: 'left', order: 10, create: () => pages.root, onShow: () => { wire(); pages.onShow(); } });
    ws.registerPanel({ id: 'layers', label: 'Layers', group: 'pages', column: 'left', order: 20, create: () => layers.root, onShow: () => { wire(); layers.onShow(); } });
    ws.registerPanel({ id: 'links', label: 'Links', group: 'pages', column: 'left', order: 30, create: () => links.root, onShow: () => { wire(); links.onShow(); } });
    requestAnimationFrame(wire);
  },
};
