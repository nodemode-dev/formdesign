import { api } from '../engine/client';
import { editor } from '../editor/editor';
import { el, patchList, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import { icon } from '../ui/icons';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Step { command: string; params: Record<string, unknown> }
interface Saved { name: string; steps: Step[] }

const SKIP = /^(selection\.|edit\.(undo|redo)|view\.|file\.(save|saveAs|open|close|export))/;

class ActionsPanel {
  readonly root = el(`<div class="fx-actions">
    <div class="fx-bar">
      <button class="cv-btn fx-rec" type="button" title="Record the changes you make as an action">Record</button>
      <span class="fx-state cv-note">Not recording</span>
    </div>
    <div class="fx-save" hidden>
      <input class="cv-input fx-name" type="text" placeholder="Name this action" aria-label="Action name" spellcheck="false">
      <button class="cv-btn cv-btn-primary fx-keep" type="button">Save</button>
      <button class="cv-btn fx-drop" type="button">Discard</button>
    </div>
    <div class="fx-list" role="list"></div>
    <p class="cv-note fx-empty">Record a series of changes once, then play it on any selection or document. Each playback is one undo step.</p>
  </div>`);
  private readonly list = q(this.root, '.fx-list');
  private readonly rec = q<HTMLButtonElement>(this.root, '.fx-rec');
  private recording = false;
  private steps: Step[] = [];
  private saved: Saved[] = [];
  private loaded = false;

  constructor() {
    this.rec.addEventListener('click', () => (this.recording ? this.stop() : this.start()));
    q(this.root, '.fx-keep').addEventListener('click', () => void this.keep());
    q(this.root, '.fx-drop').addEventListener('click', () => this.reset());
    q<HTMLInputElement>(this.root, '.fx-name').addEventListener('keydown', (e) => { e.stopPropagation(); if (e.key === 'Enter') void this.keep(); });
    editor.onExec = (command, params) => {
      if (!this.recording || SKIP.test(command) || editor.command(command)?.undoable === false) return;
      this.steps.push({ command, params: JSON.parse(JSON.stringify(params ?? {})) });
      this.state();
    };
  }

  onShow(): void { if (!this.loaded) { this.loaded = true; void this.load(); } }

  start(): void {
    this.recording = true;
    this.steps = [];
    setHidden(q(this.root, '.fx-save'), true);
    this.state();
  }

  stop(): void {
    this.recording = false;
    this.state();
    if (this.steps.length) { setHidden(q(this.root, '.fx-save'), false); q<HTMLInputElement>(this.root, '.fx-name').focus(); }
  }

  private reset(): void {
    this.steps = [];
    q<HTMLInputElement>(this.root, '.fx-name').value = '';
    setHidden(q(this.root, '.fx-save'), true);
    this.state();
  }

  private state(): void {
    setText(this.rec, this.recording ? 'Stop' : 'Record');
    setClass(this.rec, 'is-rec', this.recording);
    setText(q(this.root, '.fx-state'), this.recording ? `Recording: ${this.steps.length} step${this.steps.length === 1 ? '' : 's'}` : this.steps.length ? `${this.steps.length} steps recorded` : 'Not recording');
  }

  private async load(): Promise<void> {
    try { this.saved = (await api<{ actions: Saved[] }>('/actionsets')).actions; } catch { this.saved = []; }
    this.render();
  }

  private async keep(): Promise<void> {
    const input = q<HTMLInputElement>(this.root, '.fx-name');
    const name = input.value.trim();
    if (!name) { input.focus(); return; }
    try {
      await api('/actionsets', { json: { name, steps: this.steps } });
      toast(`Saved the action “${name}”`);
      this.reset();
      await this.load();
    } catch (e) { toast((e as Error).message, 'error'); }
  }

  async play(name: string): Promise<void> {
    const a = this.saved.find((s) => s.name === name);
    if (!a) return;
    const r = await editor.batch(a.steps);
    if (r !== undefined) toast(`Played “${name}” (${a.steps.length} steps)`);
  }

  private async remove(name: string): Promise<void> {
    try { await api(`/actionsets/${encodeURIComponent(name)}`, { method: 'DELETE' }); await this.load(); } catch (e) { toast((e as Error).message, 'error'); }
  }

  private render(): void {
    setHidden(q(this.root, '.fx-empty'), this.saved.length > 0);
    patchList<Saved>(this.list, this.saved, {
      key: (s) => s.name,
      create: (s) => {
        const n = el(`<div class="fx-row" role="listitem"><span class="fx-row-name" data-no-i18n></span><span class="fx-row-count cv-note"></span>
          <button class="cv-icon-btn fx-play" type="button" title="Play this action" aria-label="Play">${icon('play', 14)}</button>
          <button class="cv-icon-btn fx-del" type="button" title="Delete this action" aria-label="Delete">${icon('delete', 14)}</button></div>`);
        q(n, '.fx-play').addEventListener('click', () => void this.play(n.dataset.key as string));
        q(n, '.fx-del').addEventListener('click', () => void this.remove(n.dataset.key as string));
        this.fill(n, s);
        return n;
      },
      update: (n, s) => this.fill(n, s),
    });
  }

  private fill(n: HTMLElement, s: Saved): void {
    setText(q(n, '.fx-row-name'), s.name);
    setText(q(n, '.fx-row-count'), `${s.steps.length} steps`);
    setDisabled(q<HTMLButtonElement>(n, '.fx-play'), !editor.docs.length);
  }
}

export function installActions(ws: Workspace): void {
  let panel: ActionsPanel | null = null;
  const make = (): ActionsPanel => (panel ??= new ActionsPanel());
  ws.registerPanel({ id: 'actions', label: 'Actions', group: 'connect', column: 'left', order: 30, create: () => make().root, onShow: () => panel?.onShow() });
  ws.addCommands(
    { id: 'ui.actions', label: 'Actions', group: 'window', description: 'Shows the Actions panel, where recorded series of changes are kept and played.', run: () => ws.showPanel('actions') },
    { id: 'actions.record', label: 'Start Recording', group: 'actions', description: 'Starts recording the changes made to the document as a new action.', mutates: false, run: () => { ws.showPanel('actions'); make().start(); } },
    { id: 'actions.stop', label: 'Stop Recording', group: 'actions', description: 'Stops recording and asks for a name to save the action under.', mutates: false, run: () => make().stop() },
    { id: 'actions.play', label: 'Play Action', group: 'actions', description: 'Plays a saved action by name on the open document as one undoable step.', when: 'Use when the person asks to repeat a recorded action.', mutates: true,
      params: { name: { type: 'string', required: true, description: 'the saved action name' } }, run: (p) => make().play(String(p?.name ?? '')) },
  );
  ws.addMenuItems({ menu: 'Window', items: ['ui.actions'], at: 'end' });
}
