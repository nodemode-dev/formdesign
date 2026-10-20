import '../styles/review.css';
import '../styles/xml.css';
import { editor } from '../editor/editor';
import { promptName } from '../ui/dialogs';
import { el, patchList, q, setClass, setDisabled, setHidden, setStyleVar, setText } from '../ui/dom';
import { icon } from '../ui/icons';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';
import { LivePanel } from './review';

interface Tag { name: string; color: number[] }
interface Tags { root: string; tags: Tag[]; styleMap: Array<[string, string]>; dtd: boolean }
interface Element { tag: string; id: number; kind: string; text?: string | null }
interface Structure { root: string; elements: Element[] }

const rgb = (c: number[] | undefined) => (c && c.length >= 3 ? `rgb(${c[0]}, ${c[1]}, ${c[2]})` : 'var(--text-muted)');
const excerpt = (t: string | null | undefined, n = 60) => { const s = (t ?? '').replace(/\s+/g, ' ').trim(); return s.length > n ? `${s.slice(0, n - 1)}…` : s; };

async function readFile(ws: Workspace, accept: string): Promise<string | null> {
  const [f] = await ws.pickFiles(accept);
  if (!f) return null;
  return f.text();
}

export class TagsPanel extends LivePanel {
  readonly root = el(`<div class="frv-panel fx-tags">
    <p class="frv-hint fx-hint"></p>
    <div class="frv-list fx-tlist" role="listbox" aria-label="Tags"></div>
    <div class="frv-foot"><span class="frv-count"></span><button class="cv-btn frv-btn fx-new" type="button">New</button><button class="cv-btn frv-btn fx-untag" type="button" title="Remove the tag from the selection">Untag</button><button class="cv-btn frv-btn fx-del" type="button" title="Delete the picked tag; its objects are untagged">Delete</button></div>
    <button class="fx-tog" type="button" aria-expanded="false">${icon('chevronRight', 12)}<span>Map styles to tags</span></button>
    <div class="fx-map" hidden><p class="frv-hint">Paragraphs in a mapped style export as that element and come back into that style on import.</p><div class="fx-maplist"></div></div>
    <div class="fx-dtd"><span class="fx-dtdlab"></span><button class="cv-btn frv-btn fx-loaddtd" type="button">Load DTD</button><button class="cv-btn frv-btn fx-deldtd" type="button">Delete DTD</button></div>
  </div>`);
  private data: Tags = { root: 'Root', tags: [], styleMap: [], dtd: false };
  private styles: string[] = [];
  private current = '';
  private picked = '';

  constructor(ws: Workspace) {
    super(ws);
    q(this.root, '.fx-new').addEventListener('click', () => void this.create());
    q(this.root, '.fx-untag').addEventListener('click', () => void this.apply(null));
    q(this.root, '.fx-del').addEventListener('click', () => { if (this.picked) void this.run('xml.deleteTag', { name: this.picked }); });
    q(this.root, '.fx-tog').addEventListener('click', () => {
      const m = q(this.root, '.fx-map');
      setHidden(m, !m.hidden);
      q(this.root, '.fx-tog').setAttribute('aria-expanded', String(!m.hidden));
      setClass(q(this.root, '.fx-tog'), 'is-open', !m.hidden);
    });
    q(this.root, '.fx-loaddtd').addEventListener('click', async () => {
      const text = await readFile(ws, '.dtd,.txt');
      if (text === null) return;
      const r = await this.run('xml.loadDtd', { text }) as { elements?: number } | undefined;
      if (r) toast(`DTD loaded: ${r.elements ?? 0} elements, each now a tag.`);
    });
    q(this.root, '.fx-deldtd').addEventListener('click', () => void this.run('xml.deleteDtd', {}));
  }

  protected async load(): Promise<void> {
    if (!editor.hasDoc) { this.data = { root: 'Root', tags: [], styleMap: [], dtd: false }; this.render(); return; }
    const [t, s, ins] = await Promise.all([
      editor.exec<Tags>('xml.tags', {}, true),
      editor.exec<Structure>('xml.structure', {}, true),
      editor.inspect().catch(() => null) as Promise<{ paragraphStyles?: string[] } | null>,
    ]);
    this.data = t ?? this.data;
    this.styles = (ins?.paragraphStyles ?? []).filter((x) => x !== '[No Paragraph Style]');
    const sel = editor.selection?.items ?? [];
    this.current = sel.length === 1 ? s?.elements.find((e) => e.id === sel[0])?.tag ?? '' : '';
    this.render();
  }

  private target(): 'text' | 'frames' | null {
    const t = editor.selection?.text as { anchor: number; focus: number } | undefined;
    if (t && t.anchor !== t.focus) return 'text';
    return (editor.selection?.items ?? []).length ? 'frames' : null;
  }

  private async apply(tag: string | null): Promise<void> {
    const t = this.target();
    if (!t) { toast('Select a frame, or select some text, to tag it.', 'error'); return; }
    await this.run(t === 'text' ? 'xml.tagText' : 'xml.tag', { tag });
  }

  private async create(): Promise<void> {
    let i = 1;
    while (this.data.tags.some((x) => x.name === `Tag${i}`)) i++;
    const name = await promptName('New Tag', `Tag${i}`, 'Create');
    if (!name) return;
    if (!/^[A-Za-z_][A-Za-z0-9_.-]*$/.test(name)) { toast('A tag name starts with a letter or underscore and has no spaces.', 'error'); return; }
    await this.run('xml.newTag', { name });
    this.picked = name;
  }

  private render(): void {
    const t = this.target();
    setText(q(this.root, '.fx-hint'), !editor.hasDoc ? 'Open a document to tag its content.' : t === 'text' ? 'Click a tag to make the selected text an element.' : t === 'frames' ? 'Click a tag to tag the selected frame.' : 'Select a frame or some text, then click a tag.');
    patchList<Tag>(q(this.root, '.fx-tlist'), this.data.tags, {
      key: (x) => x.name,
      create: (x) => {
        const r = el('<div class="frv-row fx-trow" role="option"><span class="fx-dot"></span><span class="frv-text fx-tname"></span></div>');
        r.addEventListener('click', () => { this.picked = r.dataset.key as string; this.render(); if (this.target()) void this.apply(this.picked); });
        this.row(r, x);
        return r;
      },
      update: (r, x) => this.row(r, x),
    });
    setText(q(this.root, '.frv-count'), editor.hasDoc ? `${this.data.tags.length} ${this.data.tags.length === 1 ? 'tag' : 'tags'} · root ${this.data.root}` : '');
    setDisabled(q<HTMLButtonElement>(this.root, '.fx-new'), !editor.hasDoc);
    setDisabled(q<HTMLButtonElement>(this.root, '.fx-untag'), !t);
    setDisabled(q<HTMLButtonElement>(this.root, '.fx-del'), !this.picked || !this.data.tags.some((x) => x.name === this.picked));
    setText(q(this.root, '.fx-dtdlab'), this.data.dtd ? 'DTD loaded' : 'No DTD');
    setDisabled(q<HTMLButtonElement>(this.root, '.fx-loaddtd'), !editor.hasDoc);
    setDisabled(q<HTMLButtonElement>(this.root, '.fx-deldtd'), !this.data.dtd);
    const map = new Map(this.data.styleMap);
    patchList<string>(q(this.root, '.fx-maplist'), this.styles, {
      key: (s) => s,
      create: (s) => {
        const r = el('<label class="fx-mrow"><span class="fx-mstyle"></span><select class="cv-select fx-msel"></select></label>');
        setText(q(r, '.fx-mstyle'), s);
        q<HTMLSelectElement>(r, '.fx-msel').addEventListener('change', (e) => void this.run('xml.mapStyle', { style: r.dataset.key, tag: (e.target as HTMLSelectElement).value || null }));
        this.mrow(r, map.get(s) ?? '');
        return r;
      },
      update: (r, s) => this.mrow(r, map.get(s) ?? ''),
    });
  }

  private row(r: HTMLElement, x: Tag): void {
    setText(q(r, '.fx-tname'), x.name);
    setStyleVar(r, '--fx-dot', rgb(x.color));
    setClass(r, 'is-on', x.name === this.current);
    setClass(r, 'is-picked', x.name === this.picked && x.name !== this.current);
    r.setAttribute('aria-selected', String(x.name === this.current));
  }

  private mrow(r: HTMLElement, tag: string): void {
    const sel = q<HTMLSelectElement>(r, '.fx-msel');
    const names = ['', ...this.data.tags.map((t) => t.name)];
    const key = names.join('|');
    if (sel.dataset.key !== key) { sel.dataset.key = key; sel.replaceChildren(...names.map((n) => new Option(n || 'Not mapped', n))); }
    if (document.activeElement !== sel) sel.value = tag;
  }
}

export class StructurePanel extends LivePanel {
  readonly root = el(`<div class="frv-panel fx-struct">
    <div class="fx-rootrow"><span class="fx-rootlab">Root</span><span class="fx-rootname"></span></div>
    <div class="frv-list fx-elist" role="tree" aria-label="Structure"></div>
    <p class="frv-empty fx-empty" hidden></p>
    <div class="fx-problems" hidden></div>
    <div class="frv-foot"><span class="frv-count"></span><button class="cv-btn frv-btn fx-validate" type="button">Validate</button><button class="cv-btn frv-btn fx-import" type="button">Import</button><button class="cv-btn frv-btn fx-export" type="button">Export</button></div>
  </div>`);
  private data: Structure = { root: 'Root', elements: [] };
  private colors = new Map<string, number[]>();
  private dtd = false;
  private problems: Array<{ path: string; message: string }> | null = null;

  constructor(ws: Workspace) {
    super(ws);
    q(this.root, '.fx-validate').addEventListener('click', async () => {
      const r = await editor.exec<{ valid: boolean; problems: Array<{ path: string; message: string }> }>('xml.validate', {});
      if (!r) return;
      this.problems = r.problems ?? [];
      this.render();
      if (r.valid) toast('The structure is valid against the DTD.');
    });
    q(this.root, '.fx-import').addEventListener('click', async () => {
      const text = await readFile(ws, '.xml');
      if (text === null) return;
      const r = await this.run('file.importXml', { text }) as { placed?: number; unmatched?: number } | undefined;
      if (r) toast(`Placed ${r.placed ?? 0} ${r.placed === 1 ? 'element' : 'elements'}${r.unmatched ? `; ${r.unmatched} had no tagged frame to go into` : ''}.`);
    });
    q(this.root, '.fx-export').addEventListener('click', async () => {
      const r = await editor.exec<{ text: string }>('file.exportXml', {});
      if (!r?.text) return;
      const a = document.createElement('a');
      a.href = URL.createObjectURL(new Blob([r.text], { type: 'application/xml' }));
      a.download = `${(editor.doc?.title ?? 'FormDesign').replace(/[\\/:*?"<>|]+/g, ' ').trim() || 'FormDesign'}.xml`;
      a.click();
      window.setTimeout(() => URL.revokeObjectURL(a.href), 2000);
    });
  }

  protected async load(): Promise<void> {
    if (!editor.hasDoc) { this.data = { root: 'Root', elements: [] }; this.render(); return; }
    const [s, t] = await Promise.all([editor.exec<Structure>('xml.structure', {}, true), editor.exec<Tags>('xml.tags', {}, true)]);
    this.data = s ?? this.data;
    this.colors = new Map((t?.tags ?? []).map((x) => [x.name, x.color]));
    if (this.dtd !== !!t?.dtd) this.problems = null;
    this.dtd = !!t?.dtd;
    this.render();
  }

  private render(): void {
    setText(q(this.root, '.fx-rootname'), this.data.root);
    const sel = new Set(editor.selection?.items ?? []);
    patchList<Element>(q(this.root, '.fx-elist'), this.data.elements, {
      key: (e) => String(e.id),
      create: (e) => {
        const r = el('<button class="frv-row fx-erow" type="button" role="treeitem"><span class="fx-chip"></span><span class="frv-text fx-etext"></span><span class="fx-ekind"></span></button>');
        r.addEventListener('click', () => void this.pick(Number(r.dataset.key)));
        this.erow(r, e, sel.has(e.id));
        return r;
      },
      update: (r, e) => this.erow(r, e, sel.has(e.id)),
    });
    const empty = q(this.root, '.fx-empty');
    setHidden(empty, this.data.elements.length > 0);
    setText(empty, editor.hasDoc ? 'Nothing is tagged yet. Tag frames or text in the Tags panel.' : 'Open a document to see its structure.');
    setText(q(this.root, '.frv-count'), this.data.elements.length ? `${this.data.elements.length} elements` : '');
    const pb = q(this.root, '.fx-problems');
    setHidden(pb, this.problems === null);
    if (this.problems !== null) {
      pb.replaceChildren(...(this.problems.length ? this.problems.map((p) => { const d = el('<p class="fx-prob"><span class="fx-ppath"></span><span class="fx-pmsg"></span></p>'); setText(q(d, '.fx-ppath'), p.path); setText(q(d, '.fx-pmsg'), p.message); return d; }) : [el('<p class="fx-ok">Valid against the DTD.</p>')]));
    }
    setDisabled(q<HTMLButtonElement>(this.root, '.fx-validate'), !this.dtd);
    for (const c of ['.fx-import', '.fx-export']) setDisabled(q<HTMLButtonElement>(this.root, c), !editor.hasDoc);
  }

  private erow(r: HTMLElement, e: Element, on: boolean): void {
    setText(q(r, '.fx-chip'), e.tag);
    setStyleVar(r, '--fx-dot', rgb(this.colors.get(e.tag)));
    setText(q(r, '.fx-etext'), excerpt(e.text) || (e.kind === 'graphic' ? 'Image' : 'Object'));
    setText(q(r, '.fx-ekind'), e.kind);
    setClass(r, 'is-on', on);
  }

  private async pick(id: number): Promise<void> {
    await editor.exec('selection.set', { ids: [id] }, true);
    editor.schedule();
    this.ws.viewport.refreshOverlays();
  }
}

export function installXml(ws: Workspace): void {
  let tags: TagsPanel | null = null, struct: StructurePanel | null = null;
  ws.registerPanel({ id: 'structure', label: 'Structure', group: 'xml', column: 'dock', order: 10, create: () => (struct = new StructurePanel(ws)).root, onShow: () => struct?.onShow() });
  ws.registerPanel({ id: 'tags', label: 'Tags', group: 'xml', column: 'dock', order: 20, create: () => (tags = new TagsPanel(ws)).root, onShow: () => tags?.onShow() });
  ws.addCommands(
    { id: 'ui.structure', label: 'XML Structure Panel', keys: ['Alt+Mod+1'], run: () => ws.showPanel('structure') },
    { id: 'ui.tags', label: 'XML Tags Panel', run: () => ws.showPanel('tags') },
  );
  ws.addMenuItems({ menu: 'Window', items: ['ui.structure', 'ui.tags'], at: 'end' });
}
