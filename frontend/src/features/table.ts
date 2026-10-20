import '../styles/table.css';
import { editor } from '../editor/editor';
import type { ChangeEvent } from '../engine/client';
import { promptName } from '../ui/dialogs';
import { el, q, setChecked, setDisabled, setHidden, setText, setValue } from '../ui/dom';
import type { Workspace } from '../ui/workspace';
import { fmt, parseLength, readUnits } from './document/units';

interface Cell { row: number; col: number; fill: string; insets: number[]; vj: string; style: string }
interface Row { height: number; kind: string; mode: string }
interface TableInfo {
  table: number; story: number; rows: Row[]; columns: Array<{ width: number }>; cells: Cell[]; headerRows: number; footerRows: number; style: string;
  range: { r0: number; r1: number; c0: number; c1: number };
  options: { border?: { weight?: number }; altRows?: unknown };
}

const VJ: Array<[string, string]> = [['top', 'Top'], ['center', 'Centre'], ['bottom', 'Bottom'], ['justify', 'Justify']];
const ACTIONS: Array<[string, string]> = [
  ['table.insertRowAbove', 'Row above'], ['table.insertRowBelow', 'Row below'], ['table.insertColumnLeft', 'Column left'], ['table.insertColumnRight', 'Column right'],
  ['table.deleteRow', 'Delete row'], ['table.deleteColumn', 'Delete column'], ['table.merge', 'Merge'], ['table.unmerge', 'Unmerge'],
  ['table.distributeColumns', 'Distribute'], ['table.sortRows', 'Sort rows'],
];

function num(label: string, cls: string): string {
  return `<div class="fp-num ${cls}"><span class="fp-num-lab ftb-lab">${label}</span><input class="fp-num-in" type="text" spellcheck="false" autocomplete="off"></div>`;
}

export class TablePanel {
  readonly root = el(`<div class="ftb-panel">
    <div class="ftb-none">
      <p class="ftb-note">Click into text with the Type tool to insert a table there, or into a table to edit it.</p>
      <div class="ftb-grid3">${num('Rows', 'ftb-new-rows')}${num('Cols', 'ftb-new-cols')}${num('Head', 'ftb-new-head')}</div>
      <button class="cv-btn ftb-insert" type="button">Insert Table</button>
    </div>
    <div class="ftb-in" hidden>
      <div class="ftb-sum"></div>
      <div class="ftb-sec">Rows and columns</div>
      <div class="ftb-grid2">${num('Head', 'ftb-head')}${num('Foot', 'ftb-foot')}</div>
      <div class="ftb-grid2"><select class="cv-select ftb-mode" aria-label="Row height mode"><option value="atLeast">At Least</option><option value="exactly">Exactly</option></select>${num('H', 'ftb-rowh')}</div>
      <div class="ftb-grid2">${num('W', 'ftb-colw')}<span></span></div>
      <div class="ftb-actions"></div>
      <div class="ftb-sec">Cell</div>
      <div class="ftb-grid2"><select class="cv-select ftb-fill" aria-label="Cell fill"></select>${num('Inset', 'ftb-inset')}</div>
      <div class="cv-seg ftb-vj" role="radiogroup" aria-label="Vertical justification"></div>
      <div class="ftb-sec">Table</div>
      <div class="ftb-grid2">${num('Border', 'ftb-border')}<label class="ftb-check"><input type="checkbox" class="ftb-alt"><span>Alternating rows</span></label></div>
      <div class="ftb-grid2"><select class="cv-select ftb-tstyle" aria-label="Table style"></select><select class="cv-select ftb-cstyle" aria-label="Cell style"></select></div>
      <div class="ftb-grid2"><button class="cv-btn ftb-act ftb-newt" type="button">New table style</button><button class="cv-btn ftb-act ftb-newc" type="button">New cell style</button></div>
    </div>
  </div>`);
  private info: TableInfo | null = null;
  private timer = 0;
  private visible = false;

  constructor(private readonly ws: Workspace) {
    const acts = q(this.root, '.ftb-actions');
    for (const [cmd, label] of ACTIONS) {
      const b = el<HTMLButtonElement>(`<button class="cv-btn ftb-act" type="button"></button>`);
      setText(b, label);
      b.addEventListener('click', () => void this.run(cmd, {}));
      acts.appendChild(b);
    }
    const vj = q(this.root, '.ftb-vj');
    for (const [v, label] of VJ) {
      const b = el<HTMLButtonElement>(`<button type="button" role="radio" data-vj="${v}"></button>`);
      setText(b, label);
      b.addEventListener('click', () => void this.run('table.setCell', { vj: v }));
      vj.appendChild(b);
    }
    for (const [v, n] of [['.ftb-new-rows', 4], ['.ftb-new-cols', 3], ['.ftb-new-head', 1]] as const) setValue(this.input(v), String(n));
    q(this.root, '.ftb-insert').addEventListener('click', () => void this.insert());
    this.field('.ftb-head', (v) => this.run('table.options', { headerRows: Math.max(0, Math.round(v)) }), false);
    this.field('.ftb-foot', (v) => this.run('table.options', { footerRows: Math.max(0, Math.round(v)) }), false);
    this.field('.ftb-rowh', (v) => this.run('table.setRowHeight', { height: v, mode: q<HTMLSelectElement>(this.root, '.ftb-mode').value }), true);
    q<HTMLSelectElement>(this.root, '.ftb-mode').addEventListener('change', () => {
      const h = this.info ? this.info.rows[this.info.range.r0]?.height : null;
      if (h != null) void this.run('table.setRowHeight', { height: h, mode: q<HTMLSelectElement>(this.root, '.ftb-mode').value });
    });
    this.field('.ftb-colw', (v) => this.run('table.setColumnWidth', { width: v }), true);
    this.field('.ftb-inset', (v) => this.run('table.setCell', { insets: v }), true);
    this.field('.ftb-border', (v) => this.run('table.options', { border: { weight: v } }), false);
    q<HTMLSelectElement>(this.root, '.ftb-fill').addEventListener('change', (e) => void this.run('table.setCell', { fill: (e.target as HTMLSelectElement).value }));
    q<HTMLInputElement>(this.root, '.ftb-alt').addEventListener('change', (e) => void this.run('table.options', {
      altRows: (e.target as HTMLInputElement).checked ? { first: 1, firstColor: '[Black]', firstTint: 0.08, next: 1, nextColor: '[None]', nextTint: 1 } : null,
    }));
    q<HTMLSelectElement>(this.root, '.ftb-tstyle').addEventListener('change', (e) => void this.run('style.table.apply', { name: (e.target as HTMLSelectElement).value }));
    q<HTMLSelectElement>(this.root, '.ftb-cstyle').addEventListener('change', (e) => void this.run('style.cell.apply', { name: (e.target as HTMLSelectElement).value }));
    q(this.root, '.ftb-newt').addEventListener('click', () => void this.newStyle('table'));
    q(this.root, '.ftb-newc').addEventListener('click', () => void this.newStyle('cell'));
    editor.on('selection', () => this.schedule(60));
    editor.on('change', (e: ChangeEvent) => this.schedule(e?.docChanged ? 120 : 400));
    window.addEventListener('formdesign:units', () => this.render());
  }

  onShow(): void { this.visible = true; this.schedule(0); }

  private input(sel: string): HTMLInputElement { return q<HTMLInputElement>(this.root, `${sel} input`); }

  private field(sel: string, apply: (v: number) => Promise<unknown>, length: boolean): void {
    const inp = this.input(sel);
    const commit = () => {
      const v = length ? parseLength(inp.value, readUnits()) : Number(inp.value);
      if (v == null || !Number.isFinite(v)) { this.render(); return; }
      void apply(v);
    };
    inp.addEventListener('change', commit);
    inp.addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); commit(); } });
  }

  private schedule(ms: number): void {
    if (!this.visible) return;
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => void this.load(), ms);
  }

  private inTable(): boolean {
    const t = editor.selection?.text as { cell?: unknown } | undefined;
    return !!t?.cell;
  }

  private async load(): Promise<void> {
    this.info = editor.hasDoc && this.inTable() ? (await editor.exec<TableInfo>('table.get', {}, true).catch(() => undefined)) ?? null : null;
    if (this.info) await this.lists();
    this.render();
  }

  private async lists(): Promise<void> {
    const [styles, ins] = await Promise.all([
      editor.exec<{ table?: string[]; cell?: string[] }>('style.list', {}, true).catch(() => undefined),
      editor.inspect().catch(() => null) as Promise<{ swatches?: string[] } | null>,
    ]);
    this.options('.ftb-fill', ins?.swatches ?? ['[None]']);
    this.options('.ftb-tstyle', styles?.table ?? []);
    this.options('.ftb-cstyle', styles?.cell ?? []);
  }

  private options(sel: string, names: string[]): void {
    const s = q<HTMLSelectElement>(this.root, sel);
    const have = Array.from(s.options).map((o) => o.value);
    if (have.length === names.length && have.every((v, i) => v === names[i])) return;
    s.replaceChildren(...names.map((n) => new Option(n, n)));
  }

  private render(): void {
    const t = this.info, u = readUnits();
    setHidden(q(this.root, '.ftb-in'), !t);
    setHidden(q(this.root, '.ftb-none'), !!t);
    setDisabled(q<HTMLButtonElement>(this.root, '.ftb-insert'), !editor.wantsText);
    if (!t) return;
    const r = t.range;
    const cell = t.cells.find((c) => c.row === r.r0 && c.col === r.c0);
    const row = t.rows[r.r0];
    const body = t.rows.filter((x) => x.kind === 'body').length;
    const cellText = r.r0 === r.r1 && r.c0 === r.c1 ? `row ${r.r0 + 1}, column ${r.c0 + 1}` : `rows ${r.r0 + 1} to ${r.r1 + 1}, columns ${r.c0 + 1} to ${r.c1 + 1}`;
    setText(q(this.root, '.ftb-sum'), `${body} body rows, ${t.columns.length} columns · ${cellText}`);
    setValue(this.input('.ftb-head'), String(t.headerRows));
    setValue(this.input('.ftb-foot'), String(t.footerRows));
    setValue(q<HTMLSelectElement>(this.root, '.ftb-mode'), row?.mode ?? 'atLeast');
    setValue(this.input('.ftb-rowh'), row ? fmt(row.height, u) : '');
    setValue(this.input('.ftb-colw'), t.columns[r.c0] ? fmt(t.columns[r.c0].width, u) : '');
    setValue(this.input('.ftb-inset'), cell ? fmt(cell.insets[0] ?? 0, u) : '');
    setValue(q<HTMLSelectElement>(this.root, '.ftb-fill'), cell?.fill ?? '[None]');
    for (const b of this.root.querySelectorAll<HTMLButtonElement>('.ftb-vj button')) b.classList.toggle('is-on', b.dataset.vj === cell?.vj);
    setValue(this.input('.ftb-border'), String(t.options.border?.weight ?? 0));
    setChecked(q<HTMLInputElement>(this.root, '.ftb-alt'), !!t.options.altRows);
    setValue(q<HTMLSelectElement>(this.root, '.ftb-tstyle'), t.style || '[Basic Table]');
    setValue(q<HTMLSelectElement>(this.root, '.ftb-cstyle'), cell?.style || '[None]');
  }

  private async insert(): Promise<void> {
    const n = (sel: string, d: number) => { const v = Math.round(Number(this.input(sel).value)); return Number.isFinite(v) && v >= 0 ? v : d; };
    await this.run('table.insert', { rows: Math.max(1, n('.ftb-new-rows', 4)), cols: Math.max(1, n('.ftb-new-cols', 3)), headerRows: n('.ftb-new-head', 1) });
  }

  private async newStyle(kind: 'table' | 'cell'): Promise<void> {
    const t = this.info;
    if (!t) return;
    const taken = Array.from(q<HTMLSelectElement>(this.root, kind === 'table' ? '.ftb-tstyle' : '.ftb-cstyle').options).map((o) => o.value);
    let i = 1;
    while (taken.includes(`${kind === 'table' ? 'Table' : 'Cell'} Style ${i}`)) i++;
    const name = await promptName(kind === 'table' ? 'New Table Style' : 'New Cell Style', `${kind === 'table' ? 'Table' : 'Cell'} Style ${i}`, 'Create');
    if (!name) return;
    if (kind === 'cell') {
      if (await this.run('style.cell.create', { name, fromSelection: true }) !== undefined) await this.run('style.cell.apply', { name });
      return;
    }
    const alt = t.options.altRows as { first?: number; firstColor?: string; next?: number; nextColor?: string } | undefined;
    const params: Record<string, unknown> = { name, border: { weight: t.options.border?.weight ?? 1, color: '[Black]' } };
    if (alt) params.altRows = { first: alt.first ?? 1, firstColor: alt.firstColor ?? '[Black]', next: alt.next ?? 1, nextColor: alt.nextColor ?? '[None]' };
    if (await this.run('style.table.create', params) !== undefined) await this.run('style.table.apply', { name });
  }

  private async run(cmd: string, params: Record<string, unknown>): Promise<unknown> {
    const r = await editor.exec(cmd, params);
    this.ws.viewport.refreshOverlays();
    this.schedule(30);
    return r;
  }
}

export function installTable(ws: Workspace): void {
  let panel: TablePanel | null = null;
  ws.registerPanel({
    id: 'table', label: 'Table', group: 'swatches', column: 'dock', order: 30,
    create: () => (panel = new TablePanel(ws)).root,
    onShow: () => panel?.onShow(),
  });
  ws.addCommands({ id: 'ui.table', label: 'Table Panel', keys: ['Shift+F9'], run: () => ws.showPanel('table') });
  ws.addMenuItems({ menu: 'Table', items: ['ui.table'], at: 'start' });
}
