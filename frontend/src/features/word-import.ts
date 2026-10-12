import '../styles/review-tools.css';
import { editor, friendly } from '../editor/editor';
import { api } from '../engine/client';
import { Dialog } from '../ui/dialogs';
import { el, patchList, q, setChecked, setDisabled, setHidden, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

type Mode = 'existing' | 'new' | 'word';
interface Pick { mode: Mode; name?: string }
interface WordStyle { id: string; name: string; count: number }
interface Options { removeStyles: boolean; preserveOverrides: boolean; footnotes: boolean; quotes: boolean }
interface Scan {
  file: string; paragraph: WordStyle[]; character: WordStyle[]; footnotes: number; straightQuotes: number;
  document: { paragraph: string[]; character: string[] };
  remembered: { styleMap?: { paragraph?: Record<string, Pick>; character?: Record<string, Pick> }; options?: Partial<Options> };
  defaults: Options;
}
interface Placed { file: string; placed: string; story?: number; pending?: number; applied?: { applied: boolean; runs?: number; footnotes?: number; reason?: string }; report: { renamed: number; newStyles: Array<{ kind: string; name: string }> } }
type Kind = 'paragraph' | 'character';
interface Row { kind: Kind; style: WordStyle }

const WORD = '@word';
const NEW = '@new';
const SKIP_PARA = new Set(['[No Paragraph Style]']);

function syncOptions(sel: HTMLSelectElement, names: string[], extra: string): void {
  const want = ['@word', '@new', ...names.map((n) => `=${n}`)];
  const have = Array.from(sel.options).map((o) => o.dataset.k ?? '');
  if (want.length === have.length && want.every((w, i) => w === have[i])) return;
  const cur = sel.value;
  const keep = new Map(Array.from(sel.options).map((o) => [o.dataset.k ?? '', o]));
  let cursor: Element | null = sel.firstElementChild;
  for (const k of want) {
    let o = keep.get(k);
    if (!o) {
      o = document.createElement('option');
      o.dataset.k = k;
      o.value = k;
      o.textContent = k === WORD ? `Use ${extra} style` : k === NEW ? 'New style…' : k.slice(1);
    }
    if (o !== cursor) sel.insertBefore(o, cursor); else cursor = cursor.nextElementSibling;
  }
  for (const [k, o] of keep) if (!want.includes(k)) o.remove();
  if (Array.from(sel.options).some((o) => o.value === cur)) sel.value = cur;
}

/** The Microsoft Word Import Options dialog: built once; its two style tables are patched for each file. */
class WordImportDialog {
  private readonly d = new Dialog('Microsoft Word Import Options', 700);
  private readonly head = el('<p class="cv-note fwi-head"></p>');
  private readonly pHead = el('<h3 class="frt-h">Paragraph styles</h3>');
  private readonly pList = el('<div class="fwi-table" role="list" aria-label="Paragraph styles"></div>');
  private readonly cHead = el('<h3 class="frt-h">Character styles</h3>');
  private readonly cList = el('<div class="fwi-table" role="list" aria-label="Character styles"></div>');
  private readonly opts = el(`<div class="fwi-opts">
    <label class="fwi-check"><input type="checkbox" data-o="removeStyles"><span>Remove styles and formatting from text and tables</span></label>
    <label class="fwi-check"><input type="checkbox" data-o="preserveOverrides"><span>Preserve local overrides (bold and italic set directly in Word)</span></label>
    <label class="fwi-check"><input type="checkbox" data-o="footnotes"><span>Import footnotes</span><em class="fwi-n"></em></label>
    <label class="fwi-check"><input type="checkbox" data-o="quotes"><span>Convert straight quotes to typographer's quotes</span><em class="fwi-q"></em></label>
  </div>`);
  private readonly note = el('<p class="cv-note fwi-foot"></p>');
  private readonly ok: HTMLButtonElement;
  private scan: Scan | null = null;
  private picks: Record<Kind, Record<string, Pick>> = { paragraph: {}, character: {} };
  private resolve: ((v: boolean) => void) | null = null;
  private path = '';
  private finishTimer = 0;
  private finishUntil = 0;

  constructor(private readonly ws: Workspace) {
    q<HTMLElement>(this.d.scrim, '.cv-dialog').classList.add('fwi-dialog');
    this.d.body.append(this.head, this.pHead, this.pList, this.cHead, this.cList, this.opts, this.note);
    this.d.button('Cancel', 'ghost', () => this.d.close());
    this.ok = this.d.button('Place', 'primary', () => void this.place());
    this.d.onClose = () => { const r = this.resolve; this.resolve = null; r?.(true); };
    this.opts.addEventListener('change', (e) => {
      const t = e.target as HTMLInputElement;
      if (t.dataset.o === 'removeStyles') this.paintRemove();
    });
    for (const list of [this.pList, this.cList]) {
      list.addEventListener('change', (e) => this.changed(e.target as HTMLElement));
      list.addEventListener('input', (e) => { if ((e.target as HTMLElement).classList.contains('fwi-newname')) this.changed(e.target as HTMLElement); });
    }
    editor.on('change', () => { if (this.finishUntil > Date.now()) this.scheduleFinish(); });
  }

  /** Ask for the options; resolves true when the file was handled here (placed, loaded or cancelled), false to let the plain Place go ahead. */
  async ask(path: string): Promise<boolean> {
    this.path = path;
    let scan: Scan;
    try { scan = await api<Scan>('/place/word/scan', { json: { path } }); } catch (err) { toast(friendly((err as Error).message), 'error'); return false; }
    this.scan = scan;
    this.picks = {
      paragraph: { ...(scan.remembered.styleMap?.paragraph ?? {}) },
      character: { ...(scan.remembered.styleMap?.character ?? {}) },
    };
    const o: Options = { ...scan.defaults, ...(scan.remembered.options ?? {}) } as Options;
    for (const inp of Array.from(this.opts.querySelectorAll<HTMLInputElement>('input[data-o]'))) setChecked(inp, !!o[inp.dataset.o as keyof Options]);
    setText(this.head, `${scan.file}: ${scan.paragraph.length} paragraph style${scan.paragraph.length === 1 ? '' : 's'}, ${scan.character.length} character style${scan.character.length === 1 ? '' : 's'}. Map each Word style to a style in this document, make a new one, or keep Word's.`);
    setText(q(this.opts, '.fwi-n'), scan.footnotes ? ` (${scan.footnotes} in the file)` : ' (none in the file)');
    setText(q(this.opts, '.fwi-q'), scan.straightQuotes ? ` (${scan.straightQuotes} found)` : '');
    this.paintRemove();
    this.paint('paragraph', this.pList, scan.paragraph);
    this.paint('character', this.cList, scan.character);
    setHidden(this.pHead, !scan.paragraph.length);
    setHidden(this.pList, !scan.paragraph.length);
    setHidden(this.cHead, !scan.character.length);
    setHidden(this.cList, !scan.character.length);
    setText(this.note, scan.character.length || scan.footnotes ? 'Character styles and footnotes are applied after the text is in a frame. With no frame selected they are applied when you place the loaded text.' : 'Select a text frame first to place into it; otherwise the text loads into the Place cursor.');
    this.d.open(this.ok);
    return new Promise<boolean>((res) => { this.resolve = res; });
  }

  private paintRemove(): void {
    const off = q<HTMLInputElement>(this.opts, '[data-o="removeStyles"]').checked;
    for (const list of [this.pList, this.cList]) list.classList.toggle('is-off', off);
    for (const s of Array.from(this.d.body.querySelectorAll<HTMLSelectElement | HTMLInputElement>('.fwi-pick, .fwi-newname'))) s.disabled = off;
    const po = q<HTMLInputElement>(this.opts, '[data-o="preserveOverrides"]');
    if (po.disabled !== off) po.disabled = off;
  }

  private paint(kind: Kind, host: HTMLElement, styles: WordStyle[]): void {
    const doc = this.scan?.document[kind] ?? [];
    const names = kind === 'paragraph' ? doc.filter((n) => !SKIP_PARA.has(n)) : doc.filter((n) => n !== '[None]');
    const rows: Row[] = styles.map((style) => ({ kind, style }));
    patchList<Row>(host, rows, {
      key: (r) => `${r.kind}:${r.style.name}`,
      create: (r) => {
        const n = el('<div class="fwi-row" role="listitem"><span class="fwi-name"></span><span class="fwi-count"></span><select class="fwi-pick cv-select" aria-label="Map to"></select><input class="fwi-newname" type="text" spellcheck="false" maxlength="80" aria-label="New style name" hidden></div>');
        this.row(n, r, names);
        return n;
      },
      update: (n, r) => this.row(n, r, names),
    });
  }

  private row(n: HTMLElement, r: Row, names: string[]): void {
    setText(q(n, '.fwi-name'), r.style.name);
    setText(q(n, '.fwi-count'), `${r.style.count}`);
    const sel = q<HTMLSelectElement>(n, '.fwi-pick');
    syncOptions(sel, names, 'Word');
    const inp = q<HTMLInputElement>(n, '.fwi-newname');
    const pick = this.picks[r.kind][r.style.name];
    const exact = names.find((x) => x.toLowerCase() === r.style.name.toLowerCase());
    let v = WORD;
    if (pick?.mode === 'existing' && pick.name && names.includes(pick.name)) v = `=${pick.name}`;
    else if (pick?.mode === 'new') v = NEW;
    else if (!pick && exact) v = `=${exact}`;
    if (sel.value !== v) sel.value = v;
    if (pick?.mode === 'new' && document.activeElement !== inp && inp.value !== (pick.name ?? '')) inp.value = pick.name ?? '';
    else if (!inp.value) inp.value = r.style.name;
    setHidden(inp, v !== NEW);
  }

  private changed(t: HTMLElement): void {
    const row = t.closest<HTMLElement>('[data-key]');
    if (!row) return;
    const [kind, ...rest] = (row.dataset.key ?? '').split(':');
    const name = rest.join(':');
    const sel = q<HTMLSelectElement>(row, '.fwi-pick');
    const inp = q<HTMLInputElement>(row, '.fwi-newname');
    const v = sel.value;
    setHidden(inp, v !== NEW);
    this.picks[kind as Kind][name] = v === WORD ? { mode: 'word' } : v === NEW ? { mode: 'new', name: inp.value.trim() || name } : { mode: 'existing', name: v.slice(1) };
    if (t === sel && v === NEW) inp.focus();
  }

  private collect(): { styleMap: Record<Kind, Record<string, Pick>>; options: Options } {
    const options = {} as Options;
    for (const inp of Array.from(this.opts.querySelectorAll<HTMLInputElement>('input[data-o]'))) options[inp.dataset.o as keyof Options] = inp.checked;
    const styleMap: Record<Kind, Record<string, Pick>> = { paragraph: {}, character: {} };
    for (const kind of ['paragraph', 'character'] as Kind[]) {
      for (const s of this.scan?.[kind] ?? []) {
        const row = (kind === 'paragraph' ? this.pList : this.cList).querySelector<HTMLElement>(`[data-key="${CSS.escape(`${kind}:${s.name}`)}"]`);
        if (!row) continue;
        const v = q<HTMLSelectElement>(row, '.fwi-pick').value;
        styleMap[kind][s.name] = v === WORD ? { mode: 'word' } : v === NEW ? { mode: 'new', name: q<HTMLInputElement>(row, '.fwi-newname').value.trim() || s.name } : { mode: 'existing', name: v.slice(1) };
      }
    }
    return { styleMap, options };
  }

  private async place(): Promise<void> {
    const { styleMap, options } = this.collect();
    setDisabled(this.ok, true);
    try {
      const r = await api<Placed>('/place/word', { json: { path: this.path, styleMap, options } });
      this.d.close();
      editor.refreshMeta();
      this.ws.viewport.refreshOverlays();
      const bits: string[] = [];
      if (r.report.renamed) bits.push(`${r.report.renamed} style${r.report.renamed === 1 ? '' : 's'} mapped`);
      if (r.report.newStyles.length) bits.push(`${r.report.newStyles.length} new`);
      if (r.applied?.applied) bits.push(`${r.applied.runs ?? 0} character style run${r.applied.runs === 1 ? '' : 's'}, ${r.applied.footnotes ?? 0} footnote${r.applied.footnotes === 1 ? '' : 's'}`);
      if (r.applied && !r.applied.applied) toast(r.applied.reason ?? 'Character styles and footnotes were not applied', 'error');
      if (r.placed === 'cursor') {
        this.ws.setHint('Click or drag to place the loaded content');
        if (r.pending) { this.finishUntil = Date.now() + 30 * 60 * 1000; }
      }
      toast(`Placed ${r.file}${bits.length ? `: ${bits.join(', ')}` : ''}`);
    } catch (err) { toast(friendly((err as Error).message), 'error'); }
    setDisabled(this.ok, false);
  }

  private scheduleFinish(): void {
    window.clearTimeout(this.finishTimer);
    this.finishTimer = window.setTimeout(async () => {
      try {
        const r = await api<{ applied: unknown[]; pending: number }>('/place/word/finish', { method: 'POST' });
        if (r.applied.length) { editor.refreshMeta(); toast('Character styles and footnotes applied'); }
        if (!r.pending) this.finishUntil = 0;
      } catch { }
    }, 400);
  }
}

export function installWordImport(ws: Workspace): void {
  let dlg: WordImportDialog | null = null;
  ws.onPlaceFile(async (path) => {
    if (!/\.docx$/i.test(path) || !editor.hasDoc) return false;
    return (dlg ??= new WordImportDialog(ws)).ask(path);
  });
}
