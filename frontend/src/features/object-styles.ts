import { editor } from '../editor/editor';
import type { ChangeEvent } from '../engine/client';
import { promptName } from '../ui/dialogs';
import { el, patchList, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import type { Workspace } from '../ui/workspace';

const BUILTIN = new Set(['[None]', '[Basic Graphics Frame]', '[Basic Text Frame]']);

export class ObjectStylesPanel {
  readonly root = el(`<div class="fp-panel fp-stp fo-panel"><div class="fp-stcur"><span class="fp-stcur-lab">Applied</span><span class="fp-stcur-name"></span></div><div class="fp-listwrap"><div class="fp-list fp-stlist" role="listbox" aria-label="Object styles"></div><p class="fp-list-empty" hidden></p></div><div class="fp-foot"><span class="fp-foot-meta"></span><span class="fp-grow"></span><button class="cv-btn fo-new" type="button">New from selection</button></div></div>`);
  private readonly list = q(this.root, '.fp-stlist');
  private readonly newBtn = q<HTMLButtonElement>(this.root, '.fo-new');
  private names: string[] = [];
  private applied = '';
  private timer = 0;
  private visible = false;

  constructor(private readonly ws: Workspace) {
    this.newBtn.addEventListener('click', () => void this.create());
    editor.on('selection', () => { this.applied = ''; this.schedule(60); });
    editor.on('docs', () => this.schedule(60));
    editor.on('change', (e: ChangeEvent) => { if (e?.docChanged) this.schedule(200); });
  }

  onShow(): void { this.visible = true; this.schedule(0); }

  private schedule(ms: number): void {
    if (!this.visible) return;
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => void this.load(), ms);
  }

  private async load(): Promise<void> {
    if (!editor.hasDoc) { this.names = []; this.render(); return; }
    const r = await editor.exec<{ object?: string[] }>('style.list', {}, true).catch(() => undefined);
    this.names = r?.object ?? [];
    this.render();
  }

  private render(): void {
    const items = editor.selection?.items ?? [];
    patchList<string>(this.list, this.names, {
      key: (n) => n,
      create: (n) => {
        const r = el('<div class="fp-strow" role="option" tabindex="-1"><span class="fp-stname"></span><span class="fp-stplus"></span></div>');
        r.addEventListener('click', () => void this.apply(r.dataset.key as string));
        this.row(r, n);
        return r;
      },
      update: (r, n) => this.row(r, n),
    });
    const empty = q(this.root, '.fp-list-empty');
    setHidden(empty, this.names.length > 0);
    setText(empty, editor.hasDoc ? 'No object styles yet.' : 'Open a document to see its object styles.');
    setText(q(this.root, '.fp-stcur-name'), items.length ? this.applied || 'Click a style to apply it' : 'No object selected');
    setClass(q(this.root, '.fp-stcur'), 'is-none', !this.applied);
    setText(q(this.root, '.fp-foot-meta'), editor.hasDoc ? `${this.names.length} styles` : '');
    setDisabled(this.newBtn, items.length !== 1);
  }

  private row(r: HTMLElement, name: string): void {
    setText(q(r, '.fp-stname'), name);
    setClass(r, 'is-applied', this.applied === name);
    setClass(r, 'is-builtin', BUILTIN.has(name));
    r.setAttribute('aria-selected', String(this.applied === name));
  }

  private async apply(name: string): Promise<void> {
    if (!(editor.selection?.items ?? []).length) return;
    const ok = await editor.exec('style.object.apply', { name });
    if (ok !== undefined) { this.applied = name; this.render(); this.ws.viewport.refreshOverlays(); }
  }

  private async create(): Promise<void> {
    let i = 1;
    while (this.names.includes(`Object Style ${i}`)) i++;
    const name = await promptName('New Object Style', `Object Style ${i}`, 'Create');
    if (!name) return;
    const ok = await editor.exec('style.object.create', { name, fromSelection: true });
    if (ok === undefined) return;
    await editor.exec('style.object.apply', { name });
    this.applied = name;
    await this.load();
  }
}
