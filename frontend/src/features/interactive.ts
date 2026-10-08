import '../styles/review.css';
import '../styles/interactive.css';
import { editor } from '../editor/editor';
import { enhanceSelect } from '../ui/dropdown';
import { el, patchList, q, setChecked, setClass, setDisabled, setHidden, setText, setValue } from '../ui/dom';
import type { Workspace } from '../ui/workspace';
import { LivePanel } from './review';

interface States { states: string[]; active: number }
interface Transition { spread: number; kind: string; duration: number; horizontal: boolean }
interface ButtonInfo { id: number; name: string; action: { kind: string; page?: number; url?: string } }
interface FieldInfo { id: number; kind: string; name: string; value: string; options: string[]; multiline?: boolean; required?: boolean; fontSize?: number }

const KINDS = ['none', 'fade', 'dissolve', 'wipe', 'push', 'cover', 'uncover', 'split', 'blinds', 'box', 'comb', 'zoomIn', 'zoomOut'];
const KIND_LABEL: Record<string, string> = { none: 'None', zoomIn: 'Zoom In', zoomOut: 'Zoom Out' };
const label = (k: string) => KIND_LABEL[k] ?? k.charAt(0).toUpperCase() + k.slice(1);
const ACTIONS: Array<[string, string]> = [['page', 'Go To Page'], ['firstPage', 'Go To First Page'], ['lastPage', 'Go To Last Page'], ['nextPage', 'Go To Next Page'], ['previousPage', 'Go To Previous Page'], ['url', 'Go To URL']];
const ACTION_OF: Record<string, string> = { goToPage: 'page', goToFirstPage: 'firstPage', goToLastPage: 'lastPage', goToNextPage: 'nextPage', goToPreviousPage: 'previousPage', goToUrl: 'url' };
const TYPES: Array<[string, string]> = [['none', 'Object (not interactive)'], ['button', 'Button'], ['textField', 'Text Field'], ['checkBox', 'Check Box'], ['comboBox', 'Combo Box'], ['listBox', 'List Box'], ['signature', 'Signature Field']];

export class StatesPanel extends LivePanel {
  readonly root = el(`<div class="frv-panel fin-states">
    <p class="frv-hint fin-what"></p>
    <div class="frv-list fin-list" role="listbox" aria-label="States"></div>
    <div class="frv-foot"><span class="frv-count"></span><button class="cv-btn frv-btn fin-new" type="button" title="Make the selected objects the states of one object">New</button><button class="cv-btn frv-btn fin-release" type="button" title="Turn the states back into ordinary objects">Release</button></div>
  </div>`);
  private id: number | null = null;
  private data: States = { states: [], active: 0 };
  private editing = -1;

  constructor(ws: Workspace) {
    super(ws);
    q(this.root, '.fin-new').addEventListener('click', () => void this.run('states.create', { ids: editor.selection?.items ?? [] }));
    q(this.root, '.fin-release').addEventListener('click', () => { if (this.id !== null) void this.run('states.release', { id: this.id }); });
  }

  protected async load(): Promise<void> {
    const items = editor.selection?.items ?? [];
    this.id = null;
    this.data = { states: [], active: 0 };
    if (editor.hasDoc && items.length === 1) {
      const r = await editor.exec<States>('states.list', { id: items[0] }, true);
      if (r && r.states?.length) { this.id = items[0]; this.data = r; }
    }
    this.render();
  }

  private render(): void {
    const items = editor.selection?.items ?? [];
    const names = this.data.states.map((n, i) => ({ n, i }));
    patchList<{ n: string; i: number }>(q(this.root, '.fin-list'), names, {
      key: (x) => String(x.i),
      create: (x) => {
        const r = el('<div class="frv-row fin-row" role="option"><span class="fin-dot"></span><span class="frv-text fin-name"></span><input class="fin-rename" type="text" spellcheck="false" hidden aria-label="State name"></div>');
        r.addEventListener('click', () => { if (this.id !== null && this.editing < 0) void this.run('states.show', { id: this.id, index: Number(r.dataset.key) }); });
        r.addEventListener('dblclick', () => this.rename(r));
        const inp = q<HTMLInputElement>(r, '.fin-rename');
        inp.addEventListener('keydown', (e) => {
          e.stopPropagation();
          if (e.key === 'Enter') { e.preventDefault(); inp.blur(); }
          if (e.key === 'Escape') { inp.value = this.data.states[Number(r.dataset.key)] ?? ''; inp.blur(); }
        });
        inp.addEventListener('blur', () => void this.commit(r));
        this.row(r, x);
        return r;
      },
      update: (r, x) => this.row(r, x),
    });
    const what = q(this.root, '.fin-what');
    setText(what, this.id !== null ? 'Click a state to show it; double-click to rename it.' : items.length > 1 ? 'Make the selected objects one object with a state each: New.' : 'Select a multi-state object, or several objects to make one.');
    setText(q(this.root, '.frv-count'), this.id !== null ? `${this.data.states.length} states` : '');
    setDisabled(q<HTMLButtonElement>(this.root, '.fin-new'), !editor.hasDoc || items.length < 2);
    setDisabled(q<HTMLButtonElement>(this.root, '.fin-release'), this.id === null);
  }

  private row(r: HTMLElement, x: { n: string; i: number }): void {
    setText(q(r, '.fin-name'), x.n);
    setClass(r, 'is-on', x.i === this.data.active);
    r.setAttribute('aria-selected', String(x.i === this.data.active));
  }

  private rename(r: HTMLElement): void {
    const inp = q<HTMLInputElement>(r, '.fin-rename');
    this.editing = Number(r.dataset.key);
    inp.value = this.data.states[this.editing] ?? '';
    setHidden(q(r, '.fin-name'), true);
    setHidden(inp, false);
    inp.focus();
    inp.select();
  }

  private async commit(r: HTMLElement): Promise<void> {
    const inp = q<HTMLInputElement>(r, '.fin-rename');
    const i = Number(r.dataset.key);
    const name = inp.value.trim();
    setHidden(inp, true);
    setHidden(q(r, '.fin-name'), false);
    this.editing = -1;
    if (this.id !== null && name && name !== this.data.states[i]) await this.run('states.rename', { id: this.id, index: i, name });
  }
}

export class TransitionsPanel extends LivePanel {
  readonly root = el(`<div class="frv-panel fin-trans">
    <div class="fin-form">
      <label class="fin-f"><span>Transition</span><select class="cv-select fin-kind" aria-label="Transition"></select></label>
      <label class="fin-f"><span>Speed (seconds)</span><input class="fin-in fin-dur" type="number" min="0.1" max="10" step="0.1"></label>
      <label class="frv-check fin-hz"><input type="checkbox" class="fin-horiz"><span>Horizontal</span></label>
    </div>
    <p class="frv-hint fin-spread"></p>
    <div class="frv-list fin-tlist" role="list" aria-label="Spreads with transitions"></div>
    <div class="frv-foot"><span class="frv-count"></span><button class="cv-btn frv-btn fin-all" type="button">Apply to All Spreads</button><button class="cv-btn frv-btn fin-clear" type="button">Clear All</button></div>
  </div>`);
  private readonly kind = q<HTMLSelectElement>(this.root, '.fin-kind');
  private list: Transition[] = [];
  private spread = 0;

  constructor(ws: Workspace) {
    super(ws);
    for (const k of KINDS) this.kind.append(new Option(label(k), k));
    enhanceSelect(this.kind);
    const apply = () => void this.set({ spread: this.spread });
    this.kind.addEventListener('change', apply);
    q(this.root, '.fin-dur').addEventListener('change', apply);
    q(this.root, '.fin-horiz').addEventListener('change', apply);
    q(this.root, '.fin-all').addEventListener('click', () => void this.set({ all: true }));
    q(this.root, '.fin-clear').addEventListener('click', () => void this.run('page.transition', { all: true, kind: 'none' }));
    ws.viewport.onView = ((prev) => () => { prev?.(); const s = ws.viewport.currentSpread(); if (s !== this.spread) { this.spread = s; this.render(); } })(ws.viewport.onView);
  }

  private async set(target: Record<string, unknown>): Promise<void> {
    const d = Number(q<HTMLInputElement>(this.root, '.fin-dur').value);
    await this.run('page.transition', { ...target, kind: this.kind.value, duration: Number.isFinite(d) && d > 0 ? d : 1, horizontal: q<HTMLInputElement>(this.root, '.fin-horiz').checked });
  }

  protected async load(): Promise<void> {
    this.spread = this.ws.viewport.currentSpread();
    this.list = editor.hasDoc ? ((await editor.exec<Transition[]>('page.transitions', {}, true)) ?? []) : [];
    this.render();
  }

  private render(): void {
    const t = this.list.find((x) => x.spread === this.spread);
    setValue(this.kind, t?.kind ?? 'none');
    setValue(q<HTMLInputElement>(this.root, '.fin-dur'), String(t?.duration ?? 1));
    setChecked(q<HTMLInputElement>(this.root, '.fin-horiz'), !!t?.horizontal);
    const pages = editor.layout?.pages.filter((p) => p.spread === this.spread).map((p) => p.name) ?? [];
    setText(q(this.root, '.fin-spread'), editor.hasDoc ? `Spread ${pages.join('-') || this.spread + 1}: the transition plays when an interactive PDF turns to it.` : 'Open a document to set page transitions.');
    patchList<Transition>(q(this.root, '.fin-tlist'), this.list.filter((x) => x.kind !== 'none'), {
      key: (x) => String(x.spread),
      create: (x) => {
        const r = el('<button class="frv-row fin-trow" type="button"><span class="frv-text fin-tsp"></span><span class="fin-tk"></span></button>');
        r.addEventListener('click', () => { const s = Number(r.dataset.key); const p = editor.layout?.pages.find((pg) => pg.spread === s); if (p) this.ws.viewport.showPage(p.abs); });
        this.trow(r, x);
        return r;
      },
      update: (r, x) => this.trow(r, x),
    });
    const n = this.list.filter((x) => x.kind !== 'none').length;
    setText(q(this.root, '.frv-count'), n ? `${n} set` : 'None yet');
    for (const c of ['.fin-all', '.fin-clear']) setDisabled(q<HTMLButtonElement>(this.root, c), !editor.hasDoc);
  }

  private trow(r: HTMLElement, x: Transition): void {
    const pages = editor.layout?.pages.filter((p) => p.spread === x.spread).map((p) => p.name) ?? [];
    setText(q(r, '.fin-tsp'), `Spread ${pages.join('-') || x.spread + 1}`);
    setText(q(r, '.fin-tk'), `${label(x.kind)} · ${x.duration}s`);
    setClass(r, 'is-on', x.spread === this.spread);
  }
}

export class InteractivePanel extends LivePanel {
  readonly root = el(`<div class="frv-panel fin-ia">
    <p class="frv-hint fin-iwhat"></p>
    <div class="fin-form fin-body">
      <label class="fin-f fin-wide"><span>Type</span><select class="cv-select fin-type" aria-label="Type"></select></label>
      <label class="fin-f fin-wide fin-b"><span>On release</span><select class="cv-select fin-action" aria-label="Action"></select></label>
      <label class="fin-f fin-b fin-pagef"><span>Page</span><input class="fin-in fin-page" type="number" min="1" step="1"></label>
      <label class="fin-f fin-wide fin-b fin-urlf"><span>URL</span><input class="fin-in fin-url" type="url" spellcheck="false" placeholder="https://"></label>
      <label class="fin-f fin-wide fin-x"><span>Field name</span><input class="fin-in fin-name" type="text" spellcheck="false"></label>
      <label class="fin-f fin-wide fin-x fin-valf"><span>Default value</span><input class="fin-in fin-value" type="text" spellcheck="false"></label>
      <label class="fin-f fin-wide fin-x fin-optf"><span>Choices, one per line</span><textarea class="fin-in fin-opts" rows="3" spellcheck="false"></textarea></label>
      <label class="fin-f fin-x"><span>Font size (0 = auto)</span><input class="fin-in fin-fs" type="number" min="0" step="1"></label>
      <div class="fin-checks fin-x"><label class="frv-check fin-mlf"><input type="checkbox" class="fin-ml"><span>Multiline</span></label><label class="frv-check"><input type="checkbox" class="fin-req"><span>Required</span></label></div>
    </div>
  </div>`);
  private id: number | null = null;
  private button: ButtonInfo | null = null;
  private field: FieldInfo | null = null;

  constructor(ws: Workspace) {
    super(ws);
    const type = q<HTMLSelectElement>(this.root, '.fin-type'), action = q<HTMLSelectElement>(this.root, '.fin-action');
    for (const [v, l] of TYPES) type.append(new Option(l, v));
    for (const [v, l] of ACTIONS) action.append(new Option(l, v));
    enhanceSelect(type);
    enhanceSelect(action);
    type.addEventListener('change', () => void this.applyType());
    for (const c of ['.fin-action', '.fin-page', '.fin-url']) q(this.root, c).addEventListener('change', () => void this.applyButton());
    for (const c of ['.fin-name', '.fin-value', '.fin-opts', '.fin-fs', '.fin-ml', '.fin-req']) q(this.root, c).addEventListener('change', () => void this.applyField());
    for (const c of ['.fin-page', '.fin-url', '.fin-name', '.fin-value', '.fin-fs']) q(this.root, c).addEventListener('keydown', (e) => { e.stopPropagation(); if ((e as KeyboardEvent).key === 'Enter') (e.target as HTMLElement).blur(); });
    q(this.root, '.fin-opts').addEventListener('keydown', (e) => e.stopPropagation());
  }

  protected async load(): Promise<void> {
    const items = editor.selection?.items ?? [];
    this.id = editor.hasDoc && items.length === 1 && !editor.selection?.text ? items[0] : null;
    this.button = null;
    this.field = null;
    if (this.id !== null) {
      const [bs, fs] = await Promise.all([editor.exec<ButtonInfo[]>('button.list', {}, true), editor.exec<FieldInfo[]>('form.list', {}, true)]);
      this.button = bs?.find((b) => b.id === this.id) ?? null;
      this.field = fs?.find((f) => f.id === this.id) ?? null;
    }
    this.render();
  }

  private type(): string { return this.field ? this.field.kind : this.button ? 'button' : 'none'; }

  private render(): void {
    const has = this.id !== null;
    setText(q(this.root, '.fin-iwhat'), has ? 'Buttons and form fields work in interactive PDF exports.' : 'Select one object to make it a button or a form field.');
    setHidden(q(this.root, '.fin-body'), !has);
    if (!has) return;
    const t = this.type();
    setValue(q<HTMLSelectElement>(this.root, '.fin-type'), t);
    const isButton = t === 'button', isField = t !== 'none' && !isButton;
    for (const n of this.root.querySelectorAll<HTMLElement>('.fin-b')) setHidden(n, !isButton);
    for (const n of this.root.querySelectorAll<HTMLElement>('.fin-x')) setHidden(n, !isField);
    if (isButton) {
      const act = ACTION_OF[this.button?.action?.kind ?? ''] ?? 'nextPage';
      setValue(q<HTMLSelectElement>(this.root, '.fin-action'), act);
      setValue(q<HTMLInputElement>(this.root, '.fin-page'), String((this.button?.action?.page ?? 0) + 1));
      setValue(q<HTMLInputElement>(this.root, '.fin-url'), this.button?.action?.url ?? '');
      setHidden(q(this.root, '.fin-pagef'), act !== 'page');
      setHidden(q(this.root, '.fin-urlf'), act !== 'url');
    }
    if (isField && this.field) {
      setValue(q<HTMLInputElement>(this.root, '.fin-name'), this.field.name);
      setValue(q<HTMLInputElement>(this.root, '.fin-value'), this.field.value ?? '');
      const opts = q<HTMLTextAreaElement>(this.root, '.fin-opts');
      if (document.activeElement !== opts) opts.value = (this.field.options ?? []).join('\n');
      setValue(q<HTMLInputElement>(this.root, '.fin-fs'), String(this.field.fontSize ?? 0));
      setChecked(q<HTMLInputElement>(this.root, '.fin-ml'), !!this.field.multiline);
      setChecked(q<HTMLInputElement>(this.root, '.fin-req'), !!this.field.required);
      setHidden(q(this.root, '.fin-optf'), t !== 'comboBox' && t !== 'listBox');
      setHidden(q(this.root, '.fin-mlf'), t !== 'textField');
      setHidden(q(this.root, '.fin-valf'), t === 'signature');
    }
  }

  private async applyType(): Promise<void> {
    if (this.id === null) return;
    const t = q<HTMLSelectElement>(this.root, '.fin-type').value;
    const ids = [this.id];
    if (this.field && t !== this.field.kind) await editor.exec('form.clear', { ids }, true);
    if (this.button && t !== 'button') await editor.exec('button.clear', { ids }, true);
    if (t === 'button') await this.run('button.set', { ids, action: 'nextPage' });
    else if (t !== 'none') await this.run('form.set', { ids, kind: t, name: this.field?.name || `field${this.id}` });
    else this.schedule(0);
  }

  private async applyButton(): Promise<void> {
    if (this.id === null) return;
    const action = q<HTMLSelectElement>(this.root, '.fin-action').value;
    const params: Record<string, unknown> = { ids: [this.id], action };
    if (action === 'page') params.page = Math.max(1, Number(q<HTMLInputElement>(this.root, '.fin-page').value) || 1);
    if (action === 'url') { const u = q<HTMLInputElement>(this.root, '.fin-url').value.trim(); if (!u) { this.render(); setHidden(q(this.root, '.fin-urlf'), false); return; } params.url = u; }
    await this.run('button.set', params);
  }

  private async applyField(): Promise<void> {
    if (this.id === null || !this.field) return;
    const v = (c: string) => q<HTMLInputElement>(this.root, c).value;
    await this.run('form.set', {
      ids: [this.id], kind: this.field.kind, name: v('.fin-name').trim() || this.field.name, value: v('.fin-value'),
      options: q<HTMLTextAreaElement>(this.root, '.fin-opts').value.split('\n').map((s) => s.trim()).filter(Boolean),
      multiline: q<HTMLInputElement>(this.root, '.fin-ml').checked, required: q<HTMLInputElement>(this.root, '.fin-req').checked,
      fontSize: Math.max(0, Number(v('.fin-fs')) || 0),
    });
  }
}

export function installInteractive(ws: Workspace): void {
  let states: StatesPanel | null = null, trans: TransitionsPanel | null = null, ia: InteractivePanel | null = null;
  ws.registerPanel({ id: 'states', label: 'States', group: 'interactive', column: 'side', order: 10, create: () => (states = new StatesPanel(ws)).root, onShow: () => states?.onShow() });
  ws.registerPanel({ id: 'buttons', label: 'Buttons', group: 'interactive', column: 'side', order: 20, create: () => (ia = new InteractivePanel(ws)).root, onShow: () => ia?.onShow() });
  ws.registerPanel({ id: 'transitions', label: 'Transitions', group: 'interactive', column: 'side', order: 30, create: () => (trans = new TransitionsPanel(ws)).root, onShow: () => trans?.onShow() });
  ws.addCommands(
    { id: 'ui.states', label: 'Object States Panel', run: () => ws.showPanel('states') },
    { id: 'ui.buttons', label: 'Buttons and Forms Panel', run: () => ws.showPanel('buttons') },
    { id: 'ui.transitions', label: 'Page Transitions Panel', run: () => ws.showPanel('transitions') },
  );
  ws.addMenuItems({ menu: 'Window', items: ['ui.states', 'ui.buttons', 'ui.transitions'], at: 'end' });
}
