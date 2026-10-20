import '../styles/review.css';
import '../styles/library.css';
import { editor } from '../editor/editor';
import { api } from '../engine/client';
import { confirmAction, promptName } from '../ui/dialogs';
import { enhanceSelect } from '../ui/dropdown';
import { el, patchList, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import { icon } from '../ui/icons';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Lib { name: string; file: string; items: number }
interface Item { index: number; name: string; description: string; thumb: string }

const LIB_KEY = 'formdesign.library.current';
const DRAG_TYPE = 'application/x-formdesign-library';

export class LibraryPanel {
  readonly root = el(`<div class="frv-panel flb">
    <div class="flb-bar"><select class="cv-select flb-sel" aria-label="Library"></select><button class="cv-icon-btn flb-newlib" type="button" title="New library" aria-label="New library">${icon('plus', 15)}</button><button class="cv-icon-btn flb-dellib" type="button" title="Delete this library" aria-label="Delete library">${icon('delete', 15)}</button></div>
    <input class="flb-search" type="search" placeholder="Search items" aria-label="Search items" spellcheck="false">
    <div class="flb-grid" role="listbox" aria-label="Library items"></div>
    <p class="frv-empty flb-empty" hidden></p>
    <div class="frv-foot"><span class="frv-count"></span><button class="cv-btn frv-btn flb-add" type="button" title="Add the selected objects to this library">Add</button><button class="cv-btn frv-btn flb-place" type="button" title="Place the picked item where it was">Place</button><button class="cv-btn frv-btn flb-del" type="button">Delete</button></div>
  </div>`);
  private readonly sel = q<HTMLSelectElement>(this.root, '.flb-sel');
  private readonly search = q<HTMLInputElement>(this.root, '.flb-search');
  private libs: Lib[] = [];
  private items: Item[] = [];
  private picked = -1;
  private folder = '';

  constructor(private readonly ws: Workspace) {
    enhanceSelect(this.sel);
    this.sel.addEventListener('change', () => { try { localStorage.setItem(LIB_KEY, this.sel.value); } catch { } this.picked = -1; void this.loadItems(); });
    q(this.root, '.flb-newlib').addEventListener('click', () => void this.newLib());
    q(this.root, '.flb-dellib').addEventListener('click', () => void this.delLib());
    q(this.root, '.flb-add').addEventListener('click', () => void this.add());
    q(this.root, '.flb-place').addEventListener('click', () => { if (this.picked >= 0) void this.place(this.picked); });
    q(this.root, '.flb-del').addEventListener('click', () => void this.remove());
    this.search.addEventListener('input', () => this.render());
    this.search.addEventListener('keydown', (e) => e.stopPropagation());
    editor.on('selection', () => this.buttons());
    editor.on('docs', () => this.buttons());
    const stage = ws.viewport.stage;
    stage.addEventListener('dragover', (e) => { if (e.dataTransfer?.types.includes(DRAG_TYPE)) { e.preventDefault(); e.dataTransfer.dropEffect = 'copy'; } });
    stage.addEventListener('drop', (e) => {
      const raw = e.dataTransfer?.getData(DRAG_TYPE);
      if (!raw) return;
      e.preventDefault();
      e.stopPropagation();
      const r = stage.getBoundingClientRect();
      void this.place(Number(raw), ws.viewport.toCanvas(e.clientX - r.left, e.clientY - r.top));
    }, true);
  }

  onShow(): void { void this.loadLibs(); }

  private get name(): string { return this.sel.value; }

  private async loadLibs(want?: string): Promise<void> {
    try { const r = await api<{ folder: string; libraries: Lib[] }>('/libraries'); this.libs = r.libraries ?? []; this.folder = r.folder; } catch { this.libs = []; }
    let pick = want ?? '';
    if (!pick) { try { pick = localStorage.getItem(LIB_KEY) ?? ''; } catch { pick = ''; } }
    this.sel.replaceChildren(...(this.libs.length ? this.libs.map((l) => new Option(`${l.name} (${l.items})`, l.name)) : [new Option('No libraries yet', '')]));
    this.sel.value = this.libs.some((l) => l.name === pick) ? pick : (this.libs[0]?.name ?? '');
    if (want) { try { localStorage.setItem(LIB_KEY, want); } catch { } }
    await this.loadItems();
  }

  private async loadItems(): Promise<void> {
    this.items = [];
    if (this.name) { try { this.items = (await api<{ items: Item[] }>(`/libraries/items?name=${encodeURIComponent(this.name)}`)).items ?? []; } catch { this.items = []; } }
    if (this.picked >= this.items.length) this.picked = -1;
    this.render();
  }

  private render(): void {
    const term = this.search.value.trim().toLowerCase();
    const shown = term ? this.items.filter((i) => `${i.name} ${i.description}`.toLowerCase().includes(term)) : this.items;
    patchList<Item>(q(this.root, '.flb-grid'), shown, {
      key: (i) => `${this.name}#${i.index}#${i.thumb.length}`,
      create: (i) => {
        const c = el<HTMLButtonElement>('<button class="flb-card" type="button" role="option" draggable="true"><span class="flb-thumb"><img alt="" draggable="false"></span><span class="flb-name"></span></button>');
        const idx = () => Number((c.dataset.key ?? '').split('#')[1]);
        c.addEventListener('click', () => { this.picked = idx(); this.render(); });
        c.addEventListener('dblclick', () => void this.rename(idx()));
        c.addEventListener('dragstart', (e) => { e.dataTransfer?.setData(DRAG_TYPE, String(idx())); if (e.dataTransfer) e.dataTransfer.effectAllowed = 'copy'; });
        const img = q<HTMLImageElement>(c, 'img');
        if (i.thumb) img.src = i.thumb; else setHidden(img, true);
        this.card(c, i);
        return c;
      },
      update: (c, i) => this.card(c, i),
    });
    const empty = q(this.root, '.flb-empty');
    setHidden(empty, shown.length > 0);
    setText(empty, !this.libs.length ? 'Make a library with +, then select objects on the page and Add them. Drag an item onto a page to place it.' : this.items.length ? 'No items match.' : 'Select objects on the page and Add them to this library.');
    setText(q(this.root, '.frv-count'), this.items.length ? `${this.items.length} ${this.items.length === 1 ? 'item' : 'items'}` : '');
    this.root.title = this.folder ? `Libraries are kept in ${this.folder}` : '';
    this.buttons();
  }

  private card(c: HTMLElement, i: Item): void {
    setText(q(c, '.flb-name'), i.name);
    c.title = i.description ? `${i.name}\n${i.description}` : i.name;
    setClass(c, 'is-on', i.index === this.picked);
    c.setAttribute('aria-selected', String(i.index === this.picked));
  }

  private buttons(): void {
    setDisabled(q<HTMLButtonElement>(this.root, '.flb-add'), !this.name || !editor.hasDoc || !(editor.selection?.items ?? []).length);
    setDisabled(q<HTMLButtonElement>(this.root, '.flb-place'), !editor.hasDoc || this.picked < 0);
    setDisabled(q<HTMLButtonElement>(this.root, '.flb-del'), this.picked < 0);
    setDisabled(q<HTMLButtonElement>(this.root, '.flb-dellib'), !this.name);
  }

  private async newLib(): Promise<void> {
    const name = await promptName('New Library', this.libs.length ? `Library ${this.libs.length + 1}` : 'My library', 'Create');
    if (!name) return;
    try { await api('/libraries', { json: { name } }); await this.loadLibs(name); } catch (err) { toast((err as Error).message, 'error'); }
  }

  private async delLib(): Promise<void> {
    if (!this.name || !(await confirmAction('Delete Library', `Delete the library "${this.name}" and its ${this.items.length} items? Objects already placed stay.`, 'Delete', true))) return;
    try { await api(`/libraries?name=${encodeURIComponent(this.name)}`, { method: 'DELETE' }); await this.loadLibs(); } catch (err) { toast((err as Error).message, 'error'); }
  }

  private async add(): Promise<void> {
    const ids = editor.selection?.items ?? [];
    if (!this.name || !ids.length) return;
    try {
      const r = await api<{ index: number }>('/libraries/items', { json: { name: this.name, ids } });
      this.picked = r.index;
      await this.loadItems();
      await this.loadLibsCount();
    } catch (err) { toast((err as Error).message, 'error'); }
  }

  private async loadLibsCount(): Promise<void> {
    const cur = this.name;
    try { this.libs = (await api<{ libraries: Lib[] }>('/libraries')).libraries ?? this.libs; } catch { return; }
    for (const o of Array.from(this.sel.options)) { const l = this.libs.find((x) => x.name === o.value); if (l) o.textContent = `${l.name} (${l.items})`; }
    this.sel.value = cur;
  }

  private async rename(index: number): Promise<void> {
    const it = this.items.find((i) => i.index === index);
    if (!it) return;
    const title = await promptName('Item Name', it.name, 'Rename');
    if (!title || title === it.name) return;
    try { await api('/libraries/items', { method: 'PATCH', json: { name: this.name, index, title } }); await this.loadItems(); } catch (err) { toast((err as Error).message, 'error'); }
  }

  private async remove(): Promise<void> {
    if (this.picked < 0) return;
    try {
      await api(`/libraries/items?name=${encodeURIComponent(this.name)}&index=${this.picked}`, { method: 'DELETE' });
      this.picked = -1;
      await this.loadItems();
      await this.loadLibsCount();
    } catch (err) { toast((err as Error).message, 'error'); }
  }

  private async place(index: number, at?: [number, number]): Promise<void> {
    if (!editor.hasDoc || !this.name) return;
    const body: Record<string, unknown> = { name: this.name, index };
    if (at) {
      const slots = editor.layout?.slots ?? [];
      const inside = slots.find((s) => at[0] >= s.bounds[0] && at[0] <= s.bounds[2] && at[1] >= s.bounds[1] && at[1] <= s.bounds[3]) ?? slots[0];
      if (inside) {
        const [a, b, c, d, e, f] = inside.xf as unknown as number[];
        const det = a * d - b * c || 1;
        const X = at[0] - e, Y = at[1] - f;
        body.x = (d * X - c * Y) / det;
        body.y = (-b * X + a * Y) / det;
        body.spread = (inside as unknown as { spread: unknown }).spread;
      }
    }
    try {
      await api('/libraries/place', { json: body });
      editor.schedule();
      this.ws.viewport.refreshOverlays();
    } catch (err) { toast((err as Error).message, 'error'); }
  }
}

export function installLibrary(ws: Workspace): void {
  let panel: LibraryPanel | null = null;
  ws.registerPanel({ id: 'library', label: 'Library', group: 'library', column: 'dock', order: 10, create: () => (panel = new LibraryPanel(ws)).root, onShow: () => panel?.onShow() });
  ws.addCommands({ id: 'ui.library', label: 'Library Panel', run: () => ws.showPanel('library') });
  ws.addMenuItems({ menu: 'Window', items: ['ui.library'], at: 'end' });
}
