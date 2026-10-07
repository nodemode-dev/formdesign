import { editor } from '../../editor/editor';
import { engine, type ChangeEvent } from '../../engine/client';
import { el, patchList, q, setClass, setHidden, setText, setTitle } from '../../ui/dom';
import { toast } from '../../ui/toast';
import type { Workspace } from '../../ui/workspace';
import { glyph } from './glyphs';
import { Later, isShown, openMenu } from './shared';

export interface LinkUse { id: number; page: string; ppi?: number }
export interface LinkInfo { asset: number; name: string; path: string; status: string; pixels?: number[]; uses: LinkUse[] }

const STATUS: Record<string, string> = { ok: 'OK', modified: 'Modified', missing: 'Missing', embedded: 'Embedded' };

export class LinksPanel {
  readonly root: HTMLElement;
  private readonly list: HTMLElement;
  private readonly empty: HTMLElement;
  private readonly info: HTMLElement;
  private readonly footInfo: HTMLElement;
  private links: LinkInfo[] = [];
  private uid = -1;
  private rev = -1;
  private picked = new Map<number, number>();
  private readonly later = new Later(() => void this.refresh());

  constructor(private readonly ws: Workspace) {
    this.root = el(`<div class="lk-panel" tabindex="-1">
<div class="lk-cols"><span></span><span>Name</span><span></span><span class="lk-c-page">Page</span><span class="lk-c-ppi">PPI</span></div>
<div class="lk-scroll"><div class="lk-list"></div><div class="pg-empty lk-empty" hidden><span class="pg-empty-icon">${glyph('relink', 22)}</span><span class="lk-empty-text"></span></div></div>
<dl class="lk-info" hidden>
<dt>Status</dt><dd data-f="status"></dd><dt>Page</dt><dd data-f="page"></dd><dt>Pixels</dt><dd data-f="pixels"></dd>
<dt>Effective PPI</dt><dd data-f="ppi"></dd><dt>Placed</dt><dd data-f="uses"></dd><dt>Path</dt><dd data-f="path" class="lk-path"></dd>
</dl>
<footer class="pg-foot lk-foot"><span class="pg-foot-info"></span>
<button class="pg-icon" type="button" data-act="relink" title="Relink…">${glyph('relink', 16)}</button>
<button class="pg-icon" type="button" data-act="goto" title="Go to Link">${glyph('goTo', 16)}</button>
<button class="pg-icon" type="button" data-act="update" title="Update Link">${glyph('update', 16)}</button></footer>
</div>`);
    this.list = q(this.root, '.lk-list');
    this.empty = q(this.root, '.lk-empty');
    this.info = q(this.root, '.lk-info');
    this.footInfo = q(this.root, '.pg-foot-info');
    q(this.root, '[data-act="relink"]').addEventListener('click', () => this.ws.commands.run('links.relinkPicked'));
    q(this.root, '[data-act="goto"]').addEventListener('click', () => this.ws.commands.run('links.goToPicked'));
    q(this.root, '[data-act="update"]').addEventListener('click', () => this.ws.commands.run('links.updatePicked'));
    this.root.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      const row = (e.target as HTMLElement).closest<HTMLElement>('.lk-row');
      if (row) this.pick(Number(row.dataset.asset));
      openMenu(this.ws, this.menu(), e.clientX, e.clientY);
    });
    editor.on('docs', () => this.later.run(40));
    editor.on('change', (e: ChangeEvent) => { if (e.docChanged) this.later.run(300); });
    new ResizeObserver(() => { if (isShown(this.root) && this.rev !== engine.rev) this.later.run(0); }).observe(this.root);
  }

  onShow(): void { this.later.run(0); }

  menu(): string[] { return ['links.relinkPicked', 'links.goToPicked', 'links.updatePicked', 'links.updateAll', '-', 'links.embedPicked']; }

  get pickedLink(): LinkInfo | null {
    const a = this.picked.get(this.uid);
    return this.links.find((l) => l.asset === a) ?? null;
  }

  pick(asset: number): void {
    this.picked.set(this.uid, asset);
    this.patch();
  }

  async refresh(force = false): Promise<void> {
    if (!editor.hasDoc) { this.links = []; this.uid = -1; this.patch(); return; }
    if (!isShown(this.root) && !force) return;
    const uid = editor.doc?.uid ?? -1, rev = engine.rev;
    if (!force && uid === this.uid && rev === this.rev) return;
    let list: LinkInfo[];
    try { list = (await engine.exec<LinkInfo[]>('links.list', {})) ?? []; } catch { return; }
    if ((editor.doc?.uid ?? -1) !== uid) return;
    this.links = list;
    this.uid = uid;
    this.rev = rev;
    this.patch();
  }

  private patch(): void {
    const has = editor.hasDoc;
    const links = has ? this.links : [];
    const picked = this.pickedLink;
    patchList(this.list, links, {
      key: (l) => `A${l.asset}`,
      create: (l) => {
        const r = el(`<div class="lk-row" role="button" tabindex="-1"><span class="lk-kind">${glyph('image', 14)}</span><span class="lk-name"></span><span class="lk-status"><span class="lk-s lk-s-modified">${glyph('modified', 13)}</span><span class="lk-s lk-s-missing">${glyph('missing', 13)}</span><span class="lk-s lk-s-embedded">${glyph('embedded', 13)}</span></span><span class="lk-page"></span><span class="lk-ppi"></span></div>`);
        r.addEventListener('click', () => this.pick(Number(r.dataset.asset)));
        r.addEventListener('dblclick', () => { this.pick(Number(r.dataset.asset)); this.ws.commands.run('links.goToPicked'); });
        this.patchRow(r, l, picked);
        return r;
      },
      update: (r, l) => this.patchRow(r, l, picked),
    });
    setHidden(this.empty, links.length > 0);
    setText(q(this.empty, '.lk-empty-text'), has ? 'No placed images or files' : 'No document open');
    setClass(this.root, 'is-empty', links.length === 0);
    const missing = links.filter((l) => l.status === 'missing').length, modified = links.filter((l) => l.status === 'modified').length;
    const extra = [missing ? `${missing} missing` : '', modified ? `${modified} modified` : ''].filter(Boolean).join(', ');
    setText(this.footInfo, has ? `${links.length} ${links.length === 1 ? 'Link' : 'Links'}${extra ? ` · ${extra}` : ''}` : '');
    setClass(this.footInfo, 'is-warn', !!extra);
    setHidden(this.info, !picked);
    if (picked) {
      const f = (k: string) => q(this.info, `[data-f="${k}"]`);
      setText(f('status'), STATUS[picked.status] ?? picked.status);
      f('status').dataset.status = picked.status;
      setText(f('page'), [...new Set(picked.uses.map((u) => u.page))].join(', ') || 'Pasteboard');
      setText(f('pixels'), picked.pixels?.length === 2 ? `${picked.pixels[0]} × ${picked.pixels[1]}` : '');
      const ppi = this.ppi(picked);
      setText(f('ppi'), ppi ? String(Math.round(ppi)) : '');
      setClass(f('ppi'), 'is-low', !!ppi && ppi < 150);
      setText(f('uses'), `${picked.uses.length} ${picked.uses.length === 1 ? 'time' : 'times'}`);
      setText(f('path'), picked.path);
      setTitle(f('path'), picked.path);
    }
    for (const b of this.root.querySelectorAll<HTMLButtonElement>('.lk-foot .pg-icon')) b.disabled = !picked;
  }

  private ppi(l: LinkInfo): number { const v = l.uses.map((u) => u.ppi ?? 0).filter((x) => x > 0); return v.length ? Math.min(...v) : 0; }

  private patchRow(r: HTMLElement, l: LinkInfo, picked: LinkInfo | null): void {
    r.dataset.asset = String(l.asset);
    setText(q(r, '.lk-name'), l.name);
    const pages = [...new Set(l.uses.map((u) => u.page))];
    setText(q(r, '.lk-page'), pages.length > 1 ? `${pages[0]}+` : pages[0] ?? 'PB');
    const ppi = this.ppi(l);
    setText(q(r, '.lk-ppi'), ppi ? String(Math.round(ppi)) : '');
    setClass(q(r, '.lk-ppi'), 'is-low', !!ppi && ppi < 150);
    r.dataset.status = l.status;
    setTitle(r, `${l.name} · ${STATUS[l.status] ?? l.status}${ppi ? ` · ${Math.round(ppi)} ppi` : ''}`);
    setClass(r, 'is-picked', picked?.asset === l.asset);
  }

  async goTo(): Promise<void> {
    const l = this.pickedLink;
    if (!l) return;
    await editor.exec('links.goTo', { asset: l.asset });
    const page = l.uses[0]?.page;
    const p = editor.layout?.pages.find((x) => x.name === page);
    if (p) this.ws.viewport.showPage(p.abs);
  }

  async relink(): Promise<void> {
    const l = this.pickedLink;
    if (!l) return;
    const files = await this.ws.pickFiles('image/*,.pdf,.ai,.eps,.psd,.tif,.tiff,.svg', false);
    if (!files.length) return;
    let path: string;
    try { [path] = await this.ws.upload(files); } catch (err) { toast((err as Error).message, 'error'); return; }
    const r = await editor.exec('links.relink', { asset: l.asset, path });
    if (r !== undefined) toast(`Relinked ${l.name} to ${files[0].name}`);
    await this.refresh(true);
  }

  async update(all: boolean): Promise<void> {
    const l = this.pickedLink;
    await editor.exec('links.update', all || !l ? {} : { asset: l.asset });
    await this.refresh(true);
  }

  async embed(): Promise<void> {
    const l = this.pickedLink;
    if (!l) return;
    await editor.exec('links.embed', { asset: l.asset });
    await this.refresh(true);
  }
}
