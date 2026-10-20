import '../styles/find.css';
import '../styles/spelling.css';
import { editor } from '../editor/editor';
import { engine } from '../engine/client';
import { enhanceSelect } from '../ui/dropdown';
import { el, patchList, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import { icon } from '../ui/icons';
import type { Workspace } from '../ui/workspace';
import { floating } from './floating';

interface Miss { story: number; start: number; end: number; word: string; lang?: string | null }
interface Lang { language: string; code: string; installed: boolean; builtin: boolean; file?: string | null; downloadable?: boolean; removable?: boolean; source?: string | null }
type Rect4 = [number, number, number, number];

const DYNAMIC_KEY = 'formdesign.dynamicSpelling';
const bytes = (s: string) => new TextEncoder().encode(s).length;

export class SpellWindow {
  readonly root = el(`<section class="ff-win fsc-win" role="dialog" aria-label="Check Spelling" hidden>
    <header class="ff-head"><span class="ff-title">Check Spelling</span><button class="cv-icon-btn ff-close" type="button" title="Done" aria-label="Done">${icon('close', 16)}</button></header>
    <div class="ff-body">
      <div class="fsc-not"><span class="ff-lab">Not in Dictionary</span><span class="fsc-word"></span></div>
      <div class="ff-field fsc-torow"><span class="ff-lab">Change To</span><input class="ff-in fsc-to" type="text" spellcheck="false" aria-label="Change to"></div>
      <span class="ff-lab">Suggested Corrections</span>
      <div class="fsc-sugg" role="listbox" aria-label="Suggested corrections"></div>
      <div class="ff-opts"><select class="cv-select fsc-scope" aria-label="Search"></select><span class="ff-lab fsc-lang">Language: English: USA</span></div>
      <div class="fsc-acts">
        <button class="cv-btn cv-btn-primary fsc-start" type="button">Start</button>
        <button class="cv-btn fsc-skip" type="button">Skip</button>
        <button class="cv-btn fsc-change" type="button">Change</button>
        <button class="cv-btn fsc-ignore" type="button" title="Skip this word everywhere until FormDesign restarts">Ignore All</button>
        <button class="cv-btn fsc-changeall" type="button">Change All</button>
        <button class="cv-btn fsc-add" type="button" title="Add the word to this document's dictionary">Add</button>
      </div>
      <div class="ff-status" role="status"></div>
      <button class="fsc-dicttog" type="button" aria-expanded="false">${icon('chevronRight', 12)}<span>Dictionary for this document</span></button>
      <div class="fsc-dict" hidden>
        <div class="fsc-dhead"><span class="ff-lab">Words</span><span class="fsc-dcount"></span></div>
        <div class="fsc-dlist fsc-words"></div>
        <div class="fsc-dadd"><input class="ff-in fsc-wordin" type="text" spellcheck="false" placeholder="Add a word" aria-label="Add a word"><button class="cv-btn ff-mini fsc-wordadd" type="button">Add</button></div>
        <div class="fsc-dhead"><span class="ff-lab">Hyphenation exceptions</span><span class="fsc-hcount"></span></div>
        <div class="fsc-dlist fsc-hyph"></div>
        <div class="fsc-dadd"><input class="ff-in fsc-hyphin" type="text" spellcheck="false" placeholder="ex~am~ple, or a word never to break" aria-label="Add a hyphenation exception"><button class="cv-btn ff-mini fsc-hyphadd" type="button">Add</button></div>
      </div>
    </div></section>`);
  private readonly to = q<HTMLInputElement>(this.root, '.fsc-to');
  private readonly scope = q<HTMLSelectElement>(this.root, '.fsc-scope');
  private readonly place = floating(this.root, q(this.root, '.ff-head'), 'formdesign.spell.pos', () => ({ x: Math.max(16, window.innerWidth - 400 - 660), y: 120 }));
  private cur: Miss | null = null;
  private suggestions: string[] = [];
  private cursor: { story: number; pos: number } | null = null;
  private range: { story: number; a: number; b: number } | null = null;
  private wrapped = false;
  private readonly ignore = new Set<string>();
  private busy = false;
  private words: string[] = [];
  private hyph: string[] = [];

  constructor(private readonly ws: Workspace) {
    document.getElementById('overlay-root')?.appendChild(this.root);
    for (const [v, l] of [['document', 'Document'], ['story', 'Story'], ['toEnd', 'To End of Story'], ['selection', 'Selection']]) this.scope.append(new Option(l, v));
    enhanceSelect(this.scope);
    q(this.root, '.ff-close').addEventListener('click', () => this.close());
    q(this.root, '.fsc-start').addEventListener('click', () => void this.start());
    q(this.root, '.fsc-skip').addEventListener('click', () => void this.act('skip'));
    q(this.root, '.fsc-change').addEventListener('click', () => void this.act('change'));
    q(this.root, '.fsc-ignore').addEventListener('click', () => void this.act('ignore'));
    q(this.root, '.fsc-changeall').addEventListener('click', () => void this.act('changeAll'));
    q(this.root, '.fsc-add').addEventListener('click', () => void this.act('add'));
    this.scope.addEventListener('change', () => { this.cur = null; this.render(); });
    this.to.addEventListener('input', () => this.render());
    q(this.root, '.fsc-dicttog').addEventListener('click', () => this.toggleDict());
    q(this.root, '.fsc-wordadd').addEventListener('click', () => void this.addWord());
    q(this.root, '.fsc-hyphadd').addEventListener('click', () => void this.addHyph());
    q(this.root, '.fsc-wordin').addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); void this.addWord(); } });
    q(this.root, '.fsc-hyphin').addEventListener('keydown', (e) => { if (e.key === 'Enter') { e.preventDefault(); e.stopPropagation(); void this.addHyph(); } });
    this.root.addEventListener('keydown', (e) => {
      if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); this.close(); return; }
      if (e.key === 'Enter' && e.target === this.to) { e.preventDefault(); e.stopPropagation(); void this.act(this.cur ? 'change' : 'start'); }
    });
    editor.on('docs', () => { if (!this.root.hidden) void this.loadDict(); });
    this.render();
  }

  open(dictionary = false): void {
    if (this.root.hidden) { this.place.place(); setHidden(this.root, false); }
    if (dictionary && q(this.root, '.fsc-dict').hidden) this.toggleDict();
    void this.loadDict();
    if (!dictionary) void this.start();
  }

  close(): void {
    if (this.root.hidden) return;
    setHidden(this.root, true);
    this.cur = null;
    if (this.root.contains(document.activeElement)) (document.activeElement as HTMLElement).blur();
  }

  private async start(): Promise<void> {
    const t = editor.selection?.text as { story: number; anchor: number; focus: number } | undefined;
    const scope = this.scope.value;
    this.range = null;
    this.wrapped = false;
    if (scope === 'selection' || scope === 'toEnd' || scope === 'story') {
      if (!t) { this.status(scope === 'selection' ? 'Select some text to check within it.' : 'Click in a story first.', true); return; }
      const a = Math.min(t.anchor, t.focus), b = Math.max(t.anchor, t.focus);
      if (scope === 'selection' && a === b) { this.status('Select some text to check within it.', true); return; }
      this.range = { story: t.story, a: scope === 'story' ? 0 : a, b: scope === 'selection' ? b : Number.MAX_SAFE_INTEGER };
    }
    this.cursor = t && scope === 'document' ? { story: t.story, pos: Math.min(t.anchor, t.focus) } : null;
    await this.next();
  }

  private langs: Lang[] | null = null;

  private async list(): Promise<Miss[]> {
    const params: Record<string, unknown> = {};
    if (this.range) params.story = this.range.story;
    if (!this.langs) this.langs = (await engine.call<{ languages: Lang[] }>('spellLanguages', {}).catch(() => undefined))?.result?.languages ?? [];
    const all = (await engine.call<Miss[]>('spellCheck', params)).result ?? [];
    return all.filter((m) => !this.ignore.has(m.word.toLowerCase()) && (!this.range || (m.start >= this.range.a && m.end <= this.range.b)));
  }

  private async next(): Promise<void> {
    this.busy = true;
    this.render();
    try {
      const list = await this.list();
      const order = [...new Set(list.map((m) => m.story))];
      const after = (m: Miss) => !this.cursor || order.indexOf(m.story) > order.indexOf(this.cursor.story) || (m.story === this.cursor.story && m.start >= this.cursor.pos) || !order.includes(this.cursor.story);
      let hit = list.find(after) ?? null;
      if (!hit && this.cursor && !this.wrapped && list.length) { hit = list[0]; this.wrapped = true; }
      this.cur = hit;
      if (!hit) {
        this.suggestions = [];
        this.to.value = '';
        this.status(list.length || !this.cursor ? 'Spell check complete: no more words to check.' : 'Spell check complete.');
        return;
      }
      const r = await engine.call<{ match: { rect?: Rect4 | null } }>('findChange', { op: 'select', target: { story: hit.story, start: hit.start, end: hit.end } });
      editor.schedule();
      this.ws.viewport.refreshOverlays();
      const rect = r.result?.match?.rect;
      if (rect) this.ws.viewport.reveal(rect);
      setText(q(this.root, '.fsc-lang'), `Language: ${this.langs?.find((l) => l.code === (hit?.lang ?? 'en_US'))?.language ?? 'English: USA'}`);
      const s = await engine.call<{ suggestions: string[] }>('spellSuggest', { word: hit.word, ...(hit.lang ? { lang: hit.lang } : {}) }).catch(() => undefined);
      this.suggestions = s?.result?.suggestions ?? [];
      this.to.value = this.suggestions[0] ?? hit.word;
      this.status(`${list.length} ${list.length === 1 ? 'word' : 'words'} not in the dictionary${this.wrapped ? '; started again from the beginning' : ''}.`);
      this.to.focus();
      this.to.select();
    } catch (err) {
      this.status((err as Error).message, true);
    } finally {
      this.busy = false;
      this.render();
    }
  }

  private async act(kind: string): Promise<void> {
    if (kind === 'start') { await this.start(); return; }
    const m = this.cur;
    if (!m || this.busy) return;
    const to = this.to.value;
    this.busy = true;
    this.render();
    try {
      if (kind === 'skip') this.cursor = { story: m.story, pos: m.end };
      else if (kind === 'ignore') { this.ignore.add(m.word.toLowerCase()); this.cursor = { story: m.story, pos: m.end }; }
      else if (kind === 'add') { await editor.exec('spelling.addWord', { word: m.word }); this.cursor = { story: m.story, pos: m.end }; await this.loadDict(); }
      else if (kind === 'change') {
        if (await editor.exec('spelling.change', { story: m.story, start: m.start, end: m.end, to }) === undefined) return;
        this.cursor = { story: m.story, pos: m.start + bytes(to) };
        if (this.range) this.range.b += bytes(to) - (m.end - m.start);
      } else if (kind === 'changeAll') {
        const r = await engine.call<{ changed: number }>('findChange', { mode: 'text', op: 'changeAll', find: m.word, change: to, caseSensitive: true, wholeWord: true });
        editor.schedule();
        this.cursor = { story: m.story, pos: m.start + bytes(to) };
        this.status(`Changed ${r.result?.changed ?? 0}.`);
      }
    } catch (err) {
      this.status((err as Error).message, true);
      return;
    } finally {
      this.busy = false;
    }
    await this.next();
  }

  private render(): void {
    const m = this.cur;
    setText(q(this.root, '.fsc-word'), m ? m.word : '');
    setClass(q(this.root, '.fsc-not'), 'is-empty', !m);
    patchList<string>(q(this.root, '.fsc-sugg'), m ? this.suggestions : [], {
      key: (s) => s,
      create: (s) => {
        const b = el<HTMLButtonElement>('<button class="fsc-s" type="button" role="option"></button>');
        b.addEventListener('click', () => { this.to.value = b.dataset.key as string; this.render(); });
        b.addEventListener('dblclick', () => { this.to.value = b.dataset.key as string; void this.act('change'); });
        setText(b, s);
        return b;
      },
      update: (b, s) => { setText(b, s); setClass(b, 'is-on', this.to.value === s); },
    });
    for (const b of this.root.querySelectorAll<HTMLElement>('.fsc-s')) setClass(b, 'is-on', this.to.value === b.dataset.key);
    const has = !!m && !this.busy;
    for (const c of ['.fsc-skip', '.fsc-ignore', '.fsc-add']) setDisabled(q<HTMLButtonElement>(this.root, c), !has);
    for (const c of ['.fsc-change', '.fsc-changeall']) setDisabled(q<HTMLButtonElement>(this.root, c), !has || !this.to.value || this.to.value === m?.word);
    setDisabled(q<HTMLButtonElement>(this.root, '.fsc-start'), this.busy || !editor.hasDoc);
    setText(q(this.root, '.fsc-start'), m ? 'Restart' : 'Start');
  }

  private status(text: string, error = false): void {
    const s = q(this.root, '.ff-status');
    setText(s, text);
    setClass(s, 'is-error', error);
  }

  private toggleDict(): void {
    const d = q(this.root, '.fsc-dict');
    setHidden(d, !d.hidden);
    const t = q(this.root, '.fsc-dicttog');
    t.setAttribute('aria-expanded', String(!d.hidden));
    setClass(t, 'is-open', !d.hidden);
    if (!d.hidden) void this.loadDict();
  }

  private async loadDict(): Promise<void> {
    if (!editor.hasDoc || q(this.root, '.fsc-dict').hidden) return;
    this.words = ((await editor.exec<string[]>('spelling.words', {}, true)) ?? []).slice().sort((a, b) => a.localeCompare(b));
    this.hyph = ((await editor.exec<string[]>('hyphenation.list', {}, true)) ?? []).slice().sort((a, b) => a.localeCompare(b));
    this.renderDict();
  }

  private renderDict(): void {
    const row = (kind: 'word' | 'hyph') => ({
      key: (s: string) => s,
      create: (s: string) => {
        const r = el(`<div class="fsc-d"><span class="fsc-dw"></span><button class="cv-icon-btn fsc-dx" type="button" title="Remove" aria-label="Remove">${icon('close', 12)}</button></div>`);
        setText(q(r, '.fsc-dw'), s);
        q(r, '.fsc-dx').addEventListener('click', () => void this.remove(kind, r.dataset.key as string));
        return r;
      },
      update: (r: HTMLElement, s: string) => setText(q(r, '.fsc-dw'), s),
    });
    patchList<string>(q(this.root, '.fsc-words'), this.words, row('word'));
    patchList<string>(q(this.root, '.fsc-hyph'), this.hyph, row('hyph'));
    setText(q(this.root, '.fsc-dcount'), this.words.length ? String(this.words.length) : 'none yet');
    setText(q(this.root, '.fsc-hcount'), this.hyph.length ? String(this.hyph.length) : 'none yet');
  }

  private async remove(kind: 'word' | 'hyph', s: string): Promise<void> {
    if (kind === 'word') await editor.exec('spelling.setWords', { words: this.words.filter((w) => w !== s) });
    else await editor.exec('hyphenation.removeException', { word: s });
    await this.loadDict();
    this.ws.viewport.refreshOverlays();
  }

  private async addWord(): Promise<void> {
    const i = q<HTMLInputElement>(this.root, '.fsc-wordin');
    const w = i.value.trim();
    if (!w) return;
    if (await editor.exec('spelling.addWord', { word: w }) !== undefined) i.value = '';
    await this.loadDict();
    this.ws.viewport.refreshOverlays();
  }

  private async addHyph(): Promise<void> {
    const i = q<HTMLInputElement>(this.root, '.fsc-hyphin');
    const w = i.value.trim();
    if (!w) return;
    if (await editor.exec('hyphenation.addException', { word: w }) !== undefined) i.value = '';
    await this.loadDict();
  }
}

export function installSpelling(ws: Workspace): void {
  let win: SpellWindow | null = null;
  const make = () => (win ??= new SpellWindow(ws));
  const has = () => editor.hasDoc;
  try { ws.viewport.opts.spelling = localStorage.getItem(DYNAMIC_KEY) === '1'; } catch { ws.viewport.opts.spelling = false; }
  ws.addCommands(
    { id: 'ui.spellCheck', label: 'Check Spelling…', keys: ['Mod+I'], enabled: has, run: () => make().open() },
    {
      id: 'ui.dynamicSpelling', label: 'Dynamic Spelling', enabled: has, checked: () => ws.viewport.opts.spelling,
      run: () => {
        ws.viewport.opts.spelling = !ws.viewport.opts.spelling;
        try { localStorage.setItem(DYNAMIC_KEY, ws.viewport.opts.spelling ? '1' : '0'); } catch { }
        ws.viewport.refreshOverlays();
      },
    },
    { id: 'ui.userDictionary', label: 'Dictionary…', enabled: has, run: () => make().open(true) },
  );
  ws.override('spelling.check', () => make().open());
  ws.addMenuItems({ menu: 'Edit', items: ['-', 'ui.spellCheck', 'ui.dynamicSpelling', 'ui.userDictionary'], at: 'end' });
}
