import { api, engine } from '../engine/client';
import { editor, friendly, type CoreCommand, type DocInfo } from '../editor/editor';
import { Input } from '../editor/input';
import { Viewport } from '../editor/viewport';
import { Commands, comboOf, type Command } from './commands';
import { askSaveChanges, Dialog } from './dialogs';
import { el, escapeHtml, isMac, patchList, q, setClass, setHidden, setText, setTitle } from './dom';
import { HomeScreen } from './home';
import { icon } from './icons';
import { Dock, type PanelLayout } from './dock';
import { MenuBar, PopupMenu, type MenuDef, type MenuItem } from './menu';
import { PanelGroup, type GroupTab } from './panel-group';
import { toast } from './toast';
import { toolIcon } from './tool-icons';
import type { MenuAddition, MenuName, PanelSpec, Workspace } from './workspace';
import { VERSION } from '../version';

const MENUS: MenuName[] = ['File', 'Edit', 'Layout', 'Type', 'Object', 'Table', 'View', 'Window', 'Help'];
const LAYOUT_EXTS = /\.(fdes|fdoc|idml)$/i;

export function coreCombo(shortcut: string | null): string | null {
  if (!shortcut) return null;
  const parts = shortcut.split('+');
  const key = parts.pop() ?? '';
  if (!key) return null;
  const has = (m: string) => parts.some((p) => p.toLowerCase() === m);
  const out: string[] = [];
  if (has('alt') || has('option') || has('opt')) out.push('Alt');
  if (has('shift')) out.push('Shift');
  if (has('cmd') || has('mod') || (!isMac && has('ctrl'))) out.push('Mod');
  const k = key.length === 1 ? key.toUpperCase() : key;
  out.push(k === 'Esc' ? 'Escape' : k);
  return out.join('+');
}

export class Shell implements Workspace {
  readonly commands = new Commands();
  readonly viewport: Viewport;
  readonly input: Input;
  readonly editor = editor;
  private readonly panels: PanelSpec[] = [];
  private readonly groups = new Map<string, PanelGroup>();
  private readonly panelBodies = new Map<string, HTMLElement>();
  private readonly additions: MenuAddition[] = [];
  private readonly overrides = new Map<string, (params?: Record<string, unknown>) => void>();
  private readonly controls = new Map<string, { node: HTMLElement; priority: number }>();
  private readonly dropHandlers: Array<(files: File[]) => boolean> = [];
  private menubar: MenuBar | null = null;
  private home: HomeScreen;
  private flyout: HTMLElement | null = null;
  private coreLoaded = false;
  private readonly fileInput = el<HTMLInputElement>('<input type="file" hidden>');
  private readonly stage: HTMLElement;
  private dock: Dock | null = null;
  private groupMenu: PopupMenu | null = null;
  private menuGroup = '';

  constructor(readonly root: HTMLElement) {
    this.stage = q(root, '.fd-stage');
    this.viewport = new Viewport(this.stage, q(root, '.fd-tiles'), q(root, '.fd-hilite'), q(root, '.fd-overlay'));
    this.input = new Input(this.viewport, this.stage);
    this.input.onKeyCommand = (e) => this.onKeyCommand(e);
    root.appendChild(this.fileInput);
    this.home = new HomeScreen(this.stage.parentElement as HTMLElement, {
      run: (cmd) => this.commands.run(cmd),
      recent: () => this.recent,
      openRecent: (path) => void this.openPath(path),
    }, [
      { label: 'Make a layout…', command: 'ui.make', icon: 'sparkle', key: 'M' },
      { label: 'New from template…', command: 'ui.templates', icon: 'newFile' },
      { label: 'Open…', command: 'ui.open', icon: 'folderOpen', key: 'O' },
      { label: 'Open a PDF in the Reader…', command: 'ui.reader', icon: 'fileImage' },
    ]);
    this.uiCommands();
    this.wire();
  }

  private recent: Array<{ name: string; path: string; modified?: number }> = [];

  registerPanel(spec: PanelSpec): void {
    if (this.panels.some((p) => p.id === spec.id)) return;
    this.panels.push(spec);
    if (this.groups.size) this.mountPanel(spec);
  }

  showPanel(id: string): void {
    const spec = this.panels.find((p) => p.id === id);
    if (!spec) return;
    const g = [...PanelGroup.all.values()].find((x) => x.root.querySelector(`[data-panel="${id}"]`));
    if (g) { this.dock?.show(g.id); g.show(id); g.setMinimised(false); }
    spec.onShow?.();
  }

  private panelOpen(id: string): boolean {
    const g = [...PanelGroup.all.values()].find((x) => x.has(id));
    return !!g && !this.dock?.isHidden(g.id) && !g.minimised && g.current === id;
  }

  layout(): PanelLayout {
    const groups: PanelLayout['groups'] = {}, homes: Record<string, string> = {};
    for (const [gid, g] of PanelGroup.all) { groups[gid] = { tab: g.current, min: g.minimised }; for (const t of g.tabIds) homes[t] = gid; }
    const dock = this.dock?.snapshot ?? { side: [], dock: [], hidden: [], heights: {}, custom: [], width: null };
    return { dock, groups, homes };
  }

  applyLayout(l: PanelLayout): void {
    if (!this.dock) return;
    const custom = l.dock?.custom ?? [];
    this.dock.setState({ ...l.dock, custom });
    for (const p of this.panels) {
      const want = PanelGroup.all.get(l.homes?.[p.id] ?? '') ?? PanelGroup.all.get(p.group), from = [...PanelGroup.all.values()].find((g) => g.has(p.id));
      if (want && from && want !== from && !want.full) PanelGroup.moveTab(p.id, from, want);
    }
    PanelGroup.saveHomes();
    for (const g of [...PanelGroup.all.values()]) if (/^g[0-9a-z]+$/.test(g.id) && g.size === 0 && !custom.some((c) => c.id === g.id)) g.remove();
    for (const [gid, g] of PanelGroup.all) {
      const st = l.groups?.[gid];
      if (st?.tab && g.has(st.tab)) g.show(st.tab);
      g.setMinimised(st?.min ?? PanelGroup.startMin.has(gid));
    }
    this.dock.refresh();
  }

  menuAreas(): Map<string, string> {
    const out = new Map<string, string>();
    for (const m of MENUS) {
      const walk = (items: MenuItem[]): void => { for (const it of items) { if (typeof it !== 'string') walk(it.items); else if (it !== '-' && !out.has(it)) out.set(it, m); } };
      walk(this.menuItems(m));
    }
    return out;
  }

  addCommands(...cmds: Command[]): void { this.commands.add(...cmds); }
  addMenuItems(add: MenuAddition): void { this.additions.push(add); }
  override(coreId: string, run: (params?: Record<string, unknown>) => void): void { this.overrides.set(coreId, run); }
  onDropFiles(handler: (files: File[]) => boolean): void { this.dropHandlers.push(handler); }
  setHint(text: string): void { setText(q(this.root, '.fd-st-hint'), text); }
  showHome(): void { this.home.show(); }

  setControl(id: string, node: HTMLElement | null, priority = 0): void {
    const host = q(this.root, '.fd-control');
    const had = this.controls.get(id);
    if (!node) {
      if (had) { had.node.remove(); this.controls.delete(id); }
      return;
    }
    if (had?.node === node) return;
    had?.node.remove();
    this.controls.set(id, { node, priority });
    const list = [...this.controls.entries()].sort((a, b) => b[1].priority - a[1].priority).map(([, v]) => v.node);
    for (const n of list) host.appendChild(n);
  }

  async runCore(coreId: string, params: Record<string, unknown> = {}): Promise<unknown> {
    try {
      const r = await engine.call('exec', { command: coreId, params });
      editor.handleRequests(r.meta?.requests);
      editor.schedule();
      return r.result;
    } catch (err) {
      const msg = friendly((err as Error).message);
      const cmd = editor.command(coreId);
      if (cmd && /missing|needs|expected|required|no `/i.test(msg) && cmd.params && cmd.params !== '{}') this.paramsDialog(cmd, msg, params);
      else toast(msg, 'error');
      return undefined;
    }
  }

  pickFiles(accept: string, multiple = false): Promise<File[]> {
    return new Promise((resolve) => {
      const inp = this.fileInput;
      inp.value = '';
      inp.accept = accept;
      inp.multiple = multiple;
      const done = () => { inp.removeEventListener('change', done); resolve(Array.from(inp.files ?? [])); };
      inp.addEventListener('change', done);
      inp.click();
    });
  }

  async upload(files: File[]): Promise<string[]> {
    if (!files.length) return [];
    const fd = new FormData();
    for (const f of files) fd.append('files', f, f.name);
    const r = await api<{ files: Array<{ path: string }> }>('/files', { method: 'POST', body: fd });
    return r.files.map((f) => f.path);
  }

  private paramsDialog(cmd: CoreCommand, why: string, params: Record<string, unknown>): void {
    const d = new Dialog(cmd.label.replace(/…$/, ''), 520);
    d.body.append(el(`<div class="fd-cmd-dialog"><p class="fd-cmd-why"></p><p class="fd-cmd-doc"></p><label class="fd-field"><span>Parameters (JSON)</span><textarea class="cv-search-input fd-cmd-json" rows="6" spellcheck="false"></textarea></label></div>`));
    setText(q(d.body, '.fd-cmd-why'), why);
    setText(q(d.body, '.fd-cmd-doc'), `${cmd.id}  ${cmd.params}`);
    const ta = q<HTMLTextAreaElement>(d.body, '.fd-cmd-json');
    ta.value = JSON.stringify(params, null, 1);
    const run = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">Run</button>');
    const cancel = el<HTMLButtonElement>('<button class="cv-btn" type="button">Cancel</button>');
    cancel.addEventListener('click', () => d.close());
    run.addEventListener('click', () => {
      let p: Record<string, unknown>;
      try { p = JSON.parse(ta.value || '{}'); } catch { toast('The parameters are not valid JSON', 'error'); return; }
      d.close();
      void this.runCore(cmd.id, p);
    });
    d.foot.append(cancel, run);
    d.onClose = () => d.scrim.remove();
    d.open(ta);
  }

  private uiCommands(): void {
    const vp = this.viewport;
    const has = () => editor.hasDoc;
    const toggle = (key: 'guides' | 'frames' | 'baseline' | 'preview' | 'threads') => { vp.opts[key] = !vp.opts[key]; if (key === 'preview') vp.invalidateAll(); vp.drawOverlays(); vp.refreshOverlays(); };
    this.commands.add(
      { id: 'ui.home', label: 'Home', run: () => this.home.show() },
      { id: 'ui.open', label: 'Open…', keys: ['Mod+O'], run: () => void this.openDialog() },
      { id: 'ui.save', label: 'Save', keys: ['Mod+S'], enabled: has, run: () => void this.save(false) },
      { id: 'ui.saveAs', label: 'Save As…', keys: ['Shift+Mod+S'], enabled: has, run: () => void this.save(true) },
      { id: 'ui.close', label: 'Close', keys: ['Mod+W'], enabled: has, run: () => void this.closeDoc(editor.active ?? 0) },
      { id: 'ui.place', label: 'Place…', keys: ['Mod+D'], enabled: has, run: () => void this.place() },
      { id: 'ui.undo', label: () => (editor.doc?.undoLabel ? `Undo ${editor.doc.undoLabel}` : 'Undo'), keys: ['Mod+Z'], enabled: () => !!editor.doc?.canUndo, run: () => void editor.exec('edit.undo') },
      { id: 'ui.redo', label: () => (editor.doc?.redoLabel ? `Redo ${editor.doc.redoLabel}` : 'Redo'), keys: ['Shift+Mod+Z'], enabled: () => !!editor.doc?.canRedo, run: () => void editor.exec('edit.redo') },
      { id: 'ui.zoomIn', label: 'Zoom In', keys: ['Mod+=', 'Mod++'], enabled: has, run: () => vp.zoomAt(1.25) },
      { id: 'ui.zoomOut', label: 'Zoom Out', keys: ['Mod+-'], enabled: has, run: () => vp.zoomAt(0.8) },
      { id: 'ui.fitPage', label: 'Fit Page in Window', keys: ['Mod+0'], enabled: has, run: () => vp.fitPage() },
      { id: 'ui.fitSpread', label: 'Fit Spread in Window', keys: ['Alt+Mod+0'], enabled: has, run: () => vp.fitSpread() },
      { id: 'ui.actualSize', label: 'Actual Size', keys: ['Mod+1'], enabled: has, run: () => vp.zoomTo(1) },
      { id: 'ui.fitAll', label: 'Entire Pasteboard', keys: ['Alt+Shift+Mod+0'], enabled: has, run: () => vp.fitAll() },
      { id: 'ui.guides', label: 'Guides and Margins', keys: ['Mod+;'], checked: () => vp.opts.guides, run: () => toggle('guides') },
      { id: 'ui.frames', label: 'Frame Edges', keys: ['Alt+Mod+H'], checked: () => vp.opts.frames, run: () => toggle('frames') },
      { id: 'ui.baseline', label: 'Baseline Grid', keys: ["Alt+Mod+'"], checked: () => vp.opts.baseline, run: () => toggle('baseline') },
      { id: 'ui.threads', label: 'Text Threads', keys: ['Alt+Mod+Y'], checked: () => vp.opts.threads, run: () => toggle('threads') },
      { id: 'ui.preview', label: 'Preview', keys: ['W'], checked: () => vp.opts.preview, run: () => toggle('preview') },
      { id: 'ui.quality', label: () => (vp.opts.quality === 'high' ? 'Typical Display' : 'High Quality Display'), run: () => { vp.opts.quality = vp.opts.quality === 'high' ? 'typical' : 'high'; vp.invalidateAll(); } },
      { id: 'ui.prevPage', label: 'Previous Page', keys: ['Shift+PageUp'], enabled: has, run: () => this.stepPage(-1) },
      { id: 'ui.nextPage', label: 'Next Page', keys: ['Shift+PageDown'], enabled: has, run: () => this.stepPage(1) },
      { id: 'ui.panels', label: 'Hide Panels', keys: ['Tab'], run: () => this.root.classList.toggle('is-panels-hidden') },
      { id: 'ui.about', label: 'About formdes', run: () => toast(`formdes ${VERSION} · layout core ${editor.coreVersion || 'offline'} · Apache-2.0`) },
      { id: 'ui.group.close', label: 'Close Group', run: () => { if (this.menuGroup) this.dock?.hide(this.menuGroup); } },
      { id: 'ui.group.minimise', label: () => (PanelGroup.all.get(this.menuGroup)?.minimised ? 'Expand Group' : 'Minimise Group'), run: () => { const g = PanelGroup.all.get(this.menuGroup); g?.setMinimised(!g.minimised); } },
    );
  }

  private stepPage(d: number): void {
    const n = editor.layout?.pages.length ?? 0;
    if (!n) return;
    const cur = this.viewport.currentPage();
    this.viewport.showPage(Math.max(0, Math.min(n - 1, cur + d)));
  }

  private onKeyCommand(e: KeyboardEvent): boolean {
    if (this.menubar?.isOpen) return false;
    const combo = comboOf(e);
    if (editor.wantsText && !/Mod\+|(^|\+)F\d{1,2}$/.test(combo) && combo !== 'Escape') return false;
    if (combo === 'Tab' && editor.wantsText) return false;
    const cmd = this.commands.forCombo(combo);
    if (!cmd) return this.commands.released(combo);
    const core = editor.command(cmd.id);
    if (core && !core.enabled && !this.overrides.has(cmd.id)) { void this.refreshEnabled().then(() => { this.commands.run(cmd.id); }); return true; }
    return this.commands.run(cmd.id) || true;
  }

  private addCoreCommands(): void {
    if (this.coreLoaded || !editor.commands.length) return;
    this.coreLoaded = true;
    const taken = new Set(this.commands.all().flatMap((c) => c.keys ?? []));
    for (const c of editor.commands) {
      if (this.commands.get(c.id)) continue;
      const combo = coreCombo(c.shortcut);
      const keys = combo && !taken.has(combo) ? [combo] : [];
      for (const k of keys) taken.add(k);
      this.commands.add({
        id: c.id,
        label: c.label,
        keys,
        enabled: () => (editor.command(c.id)?.enabled ?? true),
        run: () => { const o = this.overrides.get(c.id); if (o) o(); else void this.runCore(c.id); },
      });
    }
  }

  private menuItems(name: MenuName): MenuItem[] {
    const top: MenuItem[] = [];
    const pre: MenuItem[] = [];
    const post: MenuItem[] = [];
    for (const a of this.additions.filter((x) => x.menu === name)) (a.at === 'top' ? top : a.at === 'start' ? pre : post).push(...a.items, '-');
    const core = this.coreTree(name);
    const ui = this.uiMenu(name);
    const out = [...top, ...ui.pre, ...(ui.pre.length ? ['-'] : []), ...pre, ...core, ...(post.length ? ['-', ...post] : []), ...(ui.post.length ? ['-', ...ui.post] : [])] as MenuItem[];
    const clean: MenuItem[] = [];
    for (const it of out) {
      if (it === '-' && (!clean.length || clean[clean.length - 1] === '-')) continue;
      clean.push(it);
    }
    while (clean.length && clean[clean.length - 1] === '-') clean.pop();
    return clean;
  }

  private uiMenu(name: MenuName): { pre: MenuItem[]; post: MenuItem[] } {
    switch (name) {
      case 'File': return { pre: ['ui.open', 'ui.close', 'ui.save', 'ui.saveAs', '-', 'ui.place'], post: [] };
      case 'Edit': return { pre: ['ui.undo', 'ui.redo'], post: ['ui.shortcuts'] };
      case 'View': return { pre: ['ui.zoomIn', 'ui.zoomOut', 'ui.fitPage', 'ui.fitSpread', 'ui.actualSize', 'ui.fitAll', '-', 'ui.preview', 'ui.quality', '-', 'ui.rulers', 'ui.guides', 'ui.frames', 'ui.baseline', 'ui.threads', '-', 'ui.prevPage', 'ui.nextPage'], post: [] };
      case 'Window': return { pre: this.panels.map((p) => `ui.panel.${p.id}`), post: ['ui.panels', 'ui.home'] };
      case 'Help': return { pre: ['ui.about'], post: [] };
      default: return { pre: [], post: [] };
    }
  }

  private coreTree(name: MenuName): MenuItem[] {
    const skip = new Set(['file.open', 'file.save', 'file.saveAs', 'file.close', 'edit.undo', 'edit.redo', 'file.place']);
    const root: MenuItem[] = [];
    const subs = new Map<string, { label: string; items: MenuItem[] }>();
    for (const c of editor.commands) {
      if (!c.menu?.length || c.menu[0] !== name || skip.has(c.id)) continue;
      if (c.menu.length === 1) { root.push(c.id); continue; }
      const path = c.menu.slice(1);
      let items = root;
      let key = name;
      for (const part of path) {
        key += '>' + part;
        let s = subs.get(key);
        if (!s) { s = { label: part, items: [] }; subs.set(key, s); items.push(s); }
        items = s.items;
      }
      items.push(c.id);
    }
    return root;
  }

  private buildMenus(): void {
    if (this.menubar) return;
    for (const p of this.panels) {
      if (!this.commands.get(`ui.panel.${p.id}`)) this.commands.add({ id: `ui.panel.${p.id}`, label: p.label, checked: () => this.panelOpen(p.id), run: () => this.showPanel(p.id) });
    }
    const defs: MenuDef[] = MENUS.map((m) => ({ label: m, items: () => this.menuItems(m) }));
    this.menubar = new MenuBar(q(this.root, '.cv-menubar'), this.commands, defs);
    this.menubar.onBeforeOpen = () => void this.refreshEnabled();
  }

  private enabling: Promise<void> | null = null;
  private async refreshEnabled(): Promise<void> {
    if (this.enabling) return this.enabling;
    this.enabling = (async () => {
      try {
        const r = await engine.call<CoreCommand[] | { commands: CoreCommand[] }>('commands', {});
        const list = Array.isArray(r.result) ? r.result : r.result.commands;
        for (const c of list) { const known = editor.command(c.id); if (known) known.enabled = c.enabled; }
      } catch { }
    })().finally(() => { this.enabling = null; });
    return this.enabling;
  }

  mountPanels(): void {
    const side = q(this.root, '.cv-props-col'), dock = q(this.root, '.cv-layers');
    PanelGroup.startMin = new Set(['swatches', 'preflight', 'typeextra', 'review', 'interactive', 'xml', 'library']);
    const byGroup = new Map<string, PanelSpec[]>();
    for (const p of [...this.panels].sort((a, b) => (a.order ?? 50) - (b.order ?? 50))) {
      const list = byGroup.get(p.group) ?? [];
      list.push(p);
      byGroup.set(p.group, list);
    }
    for (const [gid, list] of byGroup) {
      const tabs: GroupTab[] = list.map((p) => {
        const body = p.create();
        body.dataset.panel = p.id;
        this.panelBodies.set(p.id, body);
        return { id: p.id, label: p.label, body };
      });
      const g = new PanelGroup(list[0].column === 'side' ? side : dock, gid, tabs, list.map((p) => p.label).join(', '));
      g.onTab = (id) => this.panels.find((p) => p.id === id)?.onShow?.();
      this.groupMenuFor(g);
      this.groups.set(gid, g);
    }
    const makeGroup = (host: HTMLElement, id: string, label: string): PanelGroup => {
      const g = new PanelGroup(host, id, [], label);
      g.onTab = (tid) => this.panels.find((p) => p.id === tid)?.onShow?.();
      this.groupMenuFor(g);
      return g;
    };
    const dk = new Dock(this.root, { side, dock }, q(this.root, '.cv-resizer'), makeGroup);
    this.dock = dk;
    PanelGroup.restoreHomes();
    PanelGroup.onMoved = () => dk.refresh();
    PanelGroup.onGapHint = (x, y) => { if (x === null) dk.hideGap(); else dk.showGap(x, y, 'New group'); };
    PanelGroup.onDetach = (tid, from, x, y) => {
      const at = dk.gapAt(x, y);
      if (!at || from.size < 2) return false;
      const id = `g${Date.now().toString(36)}`, label = from.labelOf(tid), g = makeGroup(at.host, id, label);
      dk.addCustom(id, label, at.col, at.index);
      PanelGroup.moveTab(tid, from, g);
      return true;
    };
    dk.refresh();
  }

  private groupMenuFor(g: PanelGroup): void {
    q(g.root, '.cv-group-head').addEventListener('contextmenu', (e) => {
      e.preventDefault();
      this.menuGroup = g.id;
      this.groupMenu ??= new PopupMenu(this.commands, null, true);
      this.groupMenu.setItems(['ui.group.close', 'ui.group.minimise']);
      this.groupMenu.openAt(e.clientX, e.clientY);
    });
  }

  private mountPanel(spec: PanelSpec): void {
    const g = this.groups.get(spec.group);
    const body = spec.create();
    body.dataset.panel = spec.id;
    this.panelBodies.set(spec.id, body);
    if (g) g.addTab(spec.id, spec.label, body);
    else this.groups.set(spec.group, new PanelGroup(q(this.root, spec.column === 'side' ? '.cv-props-col' : '.cv-layers'), spec.group, [{ id: spec.id, label: spec.label, body }], spec.label));
  }

  private wire(): void {
    const root = this.root;
    root.addEventListener('click', (e) => {
      const b = (e.target as HTMLElement).closest<HTMLElement>('[data-cmd]');
      if (!b || !root.contains(b) || b.closest('.cv-group')) return;
      const id = b.dataset.cmd as string;
      const map: Record<string, string> = { home: 'ui.home', undo: 'ui.undo', redo: 'ui.redo', 'zoom-in': 'ui.zoomIn', 'zoom-out': 'ui.zoomOut', 'fit-spread': 'ui.fitSpread', make: 'ui.make', settings: 'ui.prefs', new: 'ui.templates' };
      if (map[id]) this.commands.run(map[id]);
    });
    editor.on('commands', () => { this.addCoreCommands(); this.buildMenus(); });
    editor.on('docs', () => { this.syncTabs(); this.home.sync(editor.docs.length); this.syncTitle(); if (!editor.docs.length) void this.loadRecent(); });
    editor.on('tool', () => this.syncTools());
    editor.on('commands', () => this.renderTools());
    editor.on('connection', (on: boolean) => this.syncStatus(on));
    editor.on('layout', () => this.syncPageLabel());
    editor.on('request', (r: any) => this.onRequest(r));
    this.viewport.onView = () => { this.syncZoom(); this.syncPageLabel(); };
    engine.on('close', () => this.syncStatus(false));
    this.syncStatus(false);
    setText(q(root, '.fd-st-version'), `formdes ${VERSION}`);
    window.addEventListener('dragover', (e) => { if (e.dataTransfer?.types.includes('Files')) { e.preventDefault(); setHidden(q(root, '.cv-drop'), false); } });
    window.addEventListener('dragleave', (e) => { if (!e.relatedTarget) setHidden(q(root, '.cv-drop'), true); });
    window.addEventListener('drop', (e) => {
      e.preventDefault();
      setHidden(q(root, '.cv-drop'), true);
      const files = Array.from(e.dataTransfer?.files ?? []);
      if (files.length) void this.dropFiles(files);
    });
    window.addEventListener('paste', (e) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA')) return;
      const text = e.clipboardData?.getData('text/plain') ?? '';
      if (editor.wantsText && text) { e.preventDefault(); this.input.paste(text); }
    });
    window.addEventListener('beforeunload', (e) => { if (editor.docs.some((d) => d.dirty) && !('__TAURI_INTERNALS__' in window)) e.preventDefault(); });
    void this.loadRecent();
  }

  private async loadRecent(): Promise<void> {
    try { this.recent = await api('/documents/recent'); } catch { this.recent = []; }
    this.home.sync(editor.docs.length);
  }

  private async dropFiles(files: File[]): Promise<void> {
    const layout = files.filter((f) => LAYOUT_EXTS.test(f.name));
    if (layout.length === files.length) {
      for (const f of layout) await this.openFile(f);
      return;
    }
    for (const h of this.dropHandlers) if (h(files)) return;
    toast('Drop files on the Make panel to lay them out');
  }

  private async openFile(f: File): Promise<void> {
    const fd = new FormData();
    fd.append('file', f, f.name);
    try { await api('/documents/upload', { method: 'POST', body: fd }); editor.refreshMeta(); } catch (err) { toast(friendly((err as Error).message), 'error'); }
  }

  private async openPath(path: string): Promise<void> {
    try { await api('/documents/open', { json: { path } }); editor.refreshMeta(); } catch (err) { toast(friendly((err as Error).message), 'error'); }
  }

  private async openDialog(): Promise<void> {
    const files = await this.pickFiles('.fdes,.fdoc,.idml,.pdf', true);
    for (const f of files) {
      if (/\.pdf$/i.test(f.name)) { this.commands.run('ui.reader.file') || toast('The Reader is not available'); continue; }
      await this.openFile(f);
    }
  }

  private async save(as: boolean): Promise<void> {
    const doc = editor.doc;
    if (!doc) return;
    let path: string | undefined;
    if (as || !doc.path || !/\.fdes$/i.test(doc.path)) {
      const d = new Dialog(as ? 'Save As' : 'Save', 460);
      d.body.append(el('<label class="fd-field"><span>Save to</span><input class="cv-search-input fd-save-path" type="text" spellcheck="false"></label><p class="fd-note">A .fdes file keeps the layout, its slots and the content it was made from. End the name with .idml for InDesign or .fdoc.</p>'));
      const inp = q<HTMLInputElement>(d.body, '.fd-save-path');
      let dir = '';
      try { dir = ((await api<{ paths: { projects: string } }>('/capabilities')).paths?.projects) ?? ''; } catch { }
      const base = (doc.title || 'Untitled').replace(/\.(fdes|fdoc|idml)$/i, '');
      inp.value = doc.path && /\.fdes$/i.test(doc.path) ? doc.path : `${dir ? dir.replace(/[\\/]$/, '') + (dir.includes('\\') ? '\\' : '/') : ''}${base}.fdes`;
      const ok = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">Save</button>');
      const cancel = el<HTMLButtonElement>('<button class="cv-btn" type="button">Cancel</button>');
      d.foot.append(cancel, ok);
      const answer = await new Promise<string | null>((resolve) => {
        cancel.addEventListener('click', () => { d.close(); resolve(null); });
        ok.addEventListener('click', () => { const v = inp.value.trim(); d.close(); resolve(v || null); });
        inp.addEventListener('keydown', (e) => { if (e.key === 'Enter') ok.click(); });
        d.onClose = () => { d.scrim.remove(); resolve(null); };
        d.open(inp);
      });
      if (!answer) return;
      path = answer;
    }
    try {
      const r = await api<{ path: string }>('/documents/save', { json: path ? { path } : {} });
      toast(`Saved ${r.path}`);
      editor.refreshMeta();
    } catch (err) { toast(friendly((err as Error).message), 'error'); }
  }

  private async closeDoc(index: number): Promise<void> {
    const d = editor.docs[index];
    if (!d) return;
    if (d.dirty) {
      if (editor.active !== index) await editor.activate(index);
      const a = await askSaveChanges(d.title);
      if (a === 'cancel') return;
      if (a === 'save') await this.save(false);
    }
    try { await api('/documents/close', { json: { index } }); } catch (err) { toast(friendly((err as Error).message), 'error'); }
    editor.refreshMeta();
  }

  private async place(): Promise<void> {
    const files = await this.pickFiles('image/*,.pdf,.txt,.md,.docx,.rtf,.csv,.xlsx,.idml,.html', false);
    if (!files.length) return;
    try {
      const [path] = await this.upload(files);
      await this.runCore('file.place', { path });
      this.setHint('Click or drag to place the loaded content');
    } catch (err) { toast(friendly((err as Error).message), 'error'); }
  }

  private onRequest(r: any): void {
    const kind = r?.kind ?? Object.keys(r ?? {})[0];
    if (kind === 'pick' && (r.purpose ?? r.pick?.purpose) === 'place') { void this.place(); return; }
    if (kind === 'view') {
      const p = r.params ?? {};
      if (typeof p.page === 'number') this.viewport.showPage(p.page);
      return;
    }
    if (kind === 'dialog') {
      const id = String(r.id ?? '');
      const cmd = editor.commands.find((c) => c.id === id || c.id.endsWith(`.${id}`));
      if (cmd) this.paramsDialog(cmd, 'Set the options, then run.', (r.params ?? {}) as Record<string, unknown>);
    }
  }

  private renderTools(): void {
    const host = q(this.root, '.cv-rail-tools');
    if (host.childElementCount || !editor.tools.length) return;
    editor.tools.forEach((group, gi) => {
      if (!group.length) { host.appendChild(el('<span class="cv-rail-sep" aria-hidden="true"></span>')); return; }
      const first = group[0];
      const b = el<HTMLButtonElement>(`<button class="cv-tool${group.length > 1 ? ' has-group' : ''}" type="button">${toolIcon(first.icon, 20)}</button>`);
      b.dataset.group = String(gi);
      b.dataset.tool = first.id;
      setTitle(b, first.shortcut ? `${first.label} (${first.shortcut})` : first.label);
      b.addEventListener('click', () => void editor.setTool(b.dataset.tool as string));
      let timer = 0;
      const openFly = () => this.toolFlyout(b, group);
      b.addEventListener('pointerdown', () => { if (group.length > 1) timer = window.setTimeout(openFly, 380); });
      b.addEventListener('pointerup', () => window.clearTimeout(timer));
      b.addEventListener('pointerleave', () => window.clearTimeout(timer));
      b.addEventListener('contextmenu', (e) => { e.preventDefault(); if (group.length > 1) openFly(); });
      host.appendChild(b);
    });
    const fs = q(this.root, '.fd-fillstroke');
    if (!fs.childElementCount) fs.append(el(`<span class="fd-fs" title="Fill and stroke of the selection">${toolIcon('fill-stroke', 22)}</span>`));
    this.syncTools();
  }

  private toolFlyout(btn: HTMLButtonElement, group: Array<{ id: string; label: string; icon: string; shortcut: string | null }>): void {
    this.flyout?.remove();
    const f = el('<div class="cv-flyout" role="menu"></div>');
    for (const t of group) {
      const it = el<HTMLButtonElement>(`<button class="cv-flyout-item" type="button">${toolIcon(t.icon, 16)}<span></span><span class="cv-flyout-key"></span></button>`);
      setText(it.querySelectorAll('span')[0], t.label);
      setText(q(it, '.cv-flyout-key'), t.shortcut ?? '');
      it.addEventListener('click', () => {
        btn.dataset.tool = t.id;
        btn.innerHTML = toolIcon(t.icon, 20);
        setTitle(btn, t.shortcut ? `${t.label} (${t.shortcut})` : t.label);
        void editor.setTool(t.id);
        f.remove();
        this.flyout = null;
      });
      f.appendChild(it);
    }
    document.getElementById('overlay-root')?.appendChild(f);
    const r = btn.getBoundingClientRect();
    f.style.left = `${Math.round(r.right + 6)}px`;
    f.style.top = `${Math.round(r.top)}px`;
    this.flyout = f;
    const away = (e: PointerEvent) => { if (!f.contains(e.target as Node)) { f.remove(); this.flyout = null; document.removeEventListener('pointerdown', away, true); } };
    window.setTimeout(() => document.addEventListener('pointerdown', away, true), 0);
  }

  private syncTools(): void {
    const host = q(this.root, '.cv-rail-tools');
    for (const b of Array.from(host.querySelectorAll<HTMLButtonElement>('.cv-tool'))) {
      const group = editor.tools[Number(b.dataset.group)] ?? [];
      const hit = group.find((t) => t.id === editor.tool);
      if (hit && b.dataset.tool !== hit.id) {
        b.dataset.tool = hit.id;
        b.innerHTML = toolIcon(hit.icon, 20);
        setTitle(b, hit.shortcut ? `${hit.label} (${hit.shortcut})` : hit.label);
      }
      setClass(b, 'is-active', !!hit);
    }
    const t = editor.tools.flat().find((x) => x.id === editor.tool);
    this.setHint(t ? t.label : '');
  }

  private syncTabs(): void {
    const strip = q(this.root, '.cv-tab-strip');
    patchList<DocInfo>(strip, editor.docs, {
      key: (d) => String(d.uid),
      create: (d) => {
        const n = el(`<div class="cv-tab" role="tab"><span class="cv-tab-dot"></span><span class="cv-tab-name"></span><button class="cv-tab-close" type="button" title="Close" aria-label="Close">${icon('close', 12)}</button></div>`);
        n.addEventListener('click', (e) => {
          const idx = editor.docs.findIndex((x) => String(x.uid) === n.dataset.key);
          if ((e.target as HTMLElement).closest('.cv-tab-close')) { void this.closeDoc(idx); return; }
          if (idx >= 0 && idx !== editor.active) void editor.activate(idx);
        });
        this.updateTab(n, d);
        return n;
      },
      update: (n, d) => this.updateTab(n, d),
    });
  }

  private updateTab(n: HTMLElement, d: DocInfo): void {
    setText(q(n, '.cv-tab-name'), d.title || 'Untitled');
    setTitle(n, d.path ?? d.title);
    setClass(n, 'is-dirty', d.dirty);
    setClass(n, 'is-active', editor.docs[editor.active ?? -1]?.uid === d.uid);
  }

  private syncTitle(): void {
    const d = editor.doc;
    document.title = d ? `${d.dirty ? '• ' : ''}${d.title} · formdes` : 'formdes';
  }

  private syncZoom(): void {
    setText(q(this.root, '.fd-zoom-label'), editor.hasDoc ? `${Math.round(this.viewport.view.zoom * 100)}%` : '');
  }

  private syncPageLabel(): void {
    const lay = editor.layout;
    const el2 = q(this.root, '.fd-st-page');
    if (!lay || !lay.pages.length) { setText(el2, ''); return; }
    const abs = this.viewport.currentPage();
    const p = lay.pages.find((x) => x.abs === abs);
    setText(el2, !p ? '' : p.name === String(p.abs + 1) ? `Page ${p.name} of ${lay.pages.length}` : `Page ${p.name} (${p.abs + 1} of ${lay.pages.length})`);
  }

  private syncStatus(on: boolean): void {
    const s = q(this.root, '.fd-st-engine');
    if (!on) setText(s, engine.connected ? 'Engine running' : 'Connecting to the engine (./formdes boot)');
    else setText(s, editor.coreVersion ? `Layout core ${editor.coreVersion}` : 'Engine ready');
    setClass(this.root, 'is-offline', !on);
  }
}
