import { editor } from '../editor/editor';
import { engine, type ChangeEvent } from '../engine/client';
import { Dialog, promptName } from '../ui/dialogs';
import { el, patchList, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

const BUILTIN = new Set(['[None]', '[Basic Graphics Frame]', '[Basic Text Frame]']);

interface ObjStyle { name: string; fill?: { swatch?: string } | null; stroke?: { swatch?: string; weight?: number } | null; paragraphStyle?: string | null }

export class ObjectStylesPanel {
  readonly root = el(`<div class="fp-panel fp-stp fo-panel"><div class="fp-stcur"><span class="fp-stcur-lab">Applied</span><span class="fp-stcur-name"></span></div><div class="fp-listwrap"><div class="fp-list fp-stlist" role="listbox" aria-label="Object styles"></div><p class="fp-list-empty" hidden></p></div><div class="fp-foot"><span class="fp-foot-meta"></span><span class="fp-grow"></span><button class="cv-btn fo-new" type="button">New</button><button class="cv-btn fo-redef" type="button" title="Redefine the picked style from the selected object">Redefine</button><button class="cv-btn fo-opts" type="button">Options</button><button class="cv-btn fo-del" type="button">Delete</button></div></div>`);
  private readonly list = q(this.root, '.fp-stlist');
  private readonly newBtn = q<HTMLButtonElement>(this.root, '.fo-new');
  private names: string[] = [];
  private styles: ObjStyle[] = [];
  private applied = '';
  private picked = '';
  private timer = 0;
  private visible = false;

  constructor(private readonly ws: Workspace) {
    this.newBtn.addEventListener('click', () => void this.create());
    q(this.root, '.fo-opts').addEventListener('click', () => { if (this.target()) void this.options(this.target()); });
    q(this.root, '.fo-redef').addEventListener('click', () => { if (this.target()) void this.edit({ edit: this.target(), redefine: true }); });
    q(this.root, '.fo-del').addEventListener('click', () => { if (this.target()) void this.edit({ delete: this.target() }); });
    editor.on('selection', () => this.schedule(60));
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
    if (!editor.hasDoc) { this.names = []; this.styles = []; this.render(); return; }
    const r = await engine.call<{ styles: ObjStyle[]; selected: string[] }>('objectStyles', {}).catch(() => undefined);
    this.styles = r?.result?.styles ?? [];
    this.names = this.styles.map((s) => s.name);
    const sel = [...new Set(r?.result?.selected ?? [])];
    this.applied = sel.length === 1 ? sel[0] : '';
    this.render();
  }

  private target(): string {
    const t = this.picked && this.names.includes(this.picked) ? this.picked : this.applied;
    return t && !BUILTIN.has(t) ? t : '';
  }

  private async edit(params: Record<string, unknown>): Promise<void> {
    try { await engine.call('objectStyles', params); } catch (err) { toast((err as Error).message, 'error'); return; }
    this.picked = typeof params.to === 'string' ? params.to : this.picked;
    editor.schedule();
    this.ws.viewport.refreshOverlays();
    await this.load();
  }

  private async options(name: string): Promise<void> {
    const os = this.styles.find((s) => s.name === name);
    if (!os) return;
    const ins = await editor.inspect().catch(() => null) as { swatches?: string[]; paragraphStyles?: string[] } | null;
    const swatches = ins?.swatches ?? [];
    const paras = ins?.paragraphStyles ?? [];
    const d = new Dialog('Object Style Options', 440);
    const body = el(`<div class="fo-form">
      <label class="fo-field"><span>Style name</span><input class="fo-name" type="text" spellcheck="false"></label>
      <label class="fo-field"><span>Fill</span><select class="cv-select fo-fill"></select></label>
      <label class="fo-field"><span>Stroke</span><select class="cv-select fo-stroke"></select></label>
      <label class="fo-field"><span>Stroke weight (pt)</span><input class="fo-weight" type="number" min="0" step="0.25"></label>
      <label class="fo-field"><span>Paragraph style</span><select class="cv-select fo-para"></select></label>
      <p class="fo-note">Changes apply to every object that uses this style. "Not set" leaves that attribute to each object.</p>
    </div>`);
    const opt = (sel: HTMLSelectElement, names: string[], cur: string | null) => {
      sel.replaceChildren(new Option('Not set', ''), ...names.map((n) => new Option(n, n)));
      sel.value = cur ?? '';
    };
    q<HTMLInputElement>(body, '.fo-name').value = os.name;
    opt(q<HTMLSelectElement>(body, '.fo-fill'), swatches, os.fill?.swatch ?? null);
    opt(q<HTMLSelectElement>(body, '.fo-stroke'), swatches, os.stroke?.swatch ?? null);
    q<HTMLInputElement>(body, '.fo-weight').value = os.stroke ? String(os.stroke.weight ?? 1) : '';
    opt(q<HTMLSelectElement>(body, '.fo-para'), paras, os.paragraphStyle ?? null);
    d.body.appendChild(body);
    const ok = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">Save</button>');
    const cancel = el<HTMLButtonElement>('<button class="cv-btn" type="button">Cancel</button>');
    d.foot.append(cancel, ok);
    cancel.addEventListener('click', () => d.close());
    ok.addEventListener('click', async () => {
      const fill = q<HTMLSelectElement>(body, '.fo-fill').value;
      const stroke = q<HTMLSelectElement>(body, '.fo-stroke').value;
      const weight = Number(q<HTMLInputElement>(body, '.fo-weight').value);
      const para = q<HTMLSelectElement>(body, '.fo-para').value;
      const to = q<HTMLInputElement>(body, '.fo-name').value.trim();
      d.close();
      await this.edit({
        edit: name, ...(to && to !== name ? { to } : {}), fill: fill || null,
        stroke: stroke ? { swatch: stroke, ...(Number.isFinite(weight) && weight >= 0 ? { weight } : {}) } : null,
        paragraphStyle: para || null,
      });
    });
    d.onClose = () => d.scrim.remove();
    d.open(q(body, '.fo-name'));
  }

  private render(): void {
    const items = editor.selection?.items ?? [];
    patchList<string>(this.list, this.names, {
      key: (n) => n,
      create: (n) => {
        const r = el('<div class="fp-strow" role="option" tabindex="-1"><span class="fp-stname"></span><span class="fp-stplus"></span></div>');
        r.addEventListener('click', () => { this.picked = r.dataset.key as string; void this.apply(this.picked); });
        r.addEventListener('dblclick', () => { const k = r.dataset.key as string; if (!BUILTIN.has(k)) void this.options(k); });
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
    const t = this.target();
    setDisabled(q<HTMLButtonElement>(this.root, '.fo-opts'), !t);
    setDisabled(q<HTMLButtonElement>(this.root, '.fo-del'), !t);
    setDisabled(q<HTMLButtonElement>(this.root, '.fo-redef'), !t || items.length !== 1);
  }

  private row(r: HTMLElement, name: string): void {
    setText(q(r, '.fp-stname'), name);
    setClass(r, 'is-picked', this.picked === name && this.applied !== name);
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
