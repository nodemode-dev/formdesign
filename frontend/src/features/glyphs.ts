import '../styles/glyphs.css';
import { editor } from '../editor/editor';
import { engine } from '../engine/client';
import { el, q, setHidden, setText, setValue } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

const RECENT_KEY = 'formdesign.glyphs.recent';
const PAGE = 480;

const SETS: Array<[string, string, Array<[number, number]>]> = [
  ['all', 'Entire font', []],
  ['latin', 'Latin', [[0x20, 0x7e], [0xa0, 0x24f], [0x1e00, 0x1eff]]],
  ['punct', 'Punctuation', [[0x21, 0x2f], [0x3a, 0x40], [0x5b, 0x60], [0x7b, 0x7e], [0xa1, 0xbf], [0x2010, 0x205e]]],
  ['nums', 'Numbers and fractions', [[0x30, 0x39], [0xb2, 0xb3], [0xb9, 0xb9], [0xbc, 0xbe], [0x2070, 0x209f], [0x2150, 0x218f], [0x2460, 0x24ff]]],
  ['currency', 'Currency', [[0x24, 0x24], [0xa2, 0xa5], [0x20a0, 0x20cf]]],
  ['symbols', 'Symbols and arrows', [[0xa9, 0xa9], [0xae, 0xae], [0xb0, 0xb1], [0xb5, 0xb7], [0xd7, 0xd7], [0xf7, 0xf7], [0x2100, 0x214f], [0x2190, 0x23ff], [0x25a0, 0x25ff], [0x2700, 0x27bf]]],
  ['greek', 'Greek and Cyrillic', [[0x370, 0x3ff], [0x400, 0x4ff]]],
];

const hex = (cp: number) => `U+${cp.toString(16).toUpperCase().padStart(4, '0')}`;

export class GlyphsPanel {
  readonly root = el(`<div class="fg-panel">
    <div class="fg-font"><select class="cv-select fg-family" aria-label="Font family"></select><select class="cv-select fg-style" aria-label="Font style"></select></div>
    <div class="fg-bar"><select class="cv-select fg-set" aria-label="Show"></select><span class="fg-count"></span></div>
    <div class="fg-recent-wrap" hidden><div class="fg-label">Recently used</div><div class="fg-recent"></div></div>
    <div class="fg-grid" role="grid" aria-label="Glyphs"></div>
    <button class="cv-btn fg-more" type="button" hidden>Show more</button>
    <div class="fg-foot"><span class="fg-code"></span><span class="fg-hint">Click a glyph to insert it at the text cursor</span></div>
  </div>`);
  private readonly grid = q(this.root, '.fg-grid');
  private readonly family = q<HTMLSelectElement>(this.root, '.fg-family');
  private readonly style = q<HTMLSelectElement>(this.root, '.fg-style');
  private readonly set = q<HTMLSelectElement>(this.root, '.fg-set');
  private chars: number[] = [];
  private shown = 0;
  private faceName = '';
  private faceKey = '';
  private readonly loaded = new Map<string, string>();
  private seq = 0;
  private recent: number[] = [];
  private visible = false;
  private families: string[] = [];
  private followTimer = 0;

  constructor(private readonly ws: Workspace) {
    for (const [id, name] of SETS) this.set.appendChild(new Option(name, id));
    try { this.recent = JSON.parse(localStorage.getItem(RECENT_KEY) || '[]').filter((n: unknown) => typeof n === 'number').slice(0, 16); } catch { this.recent = []; }
    this.family.addEventListener('change', () => void this.pickFamily(this.family.value));
    this.style.addEventListener('change', () => void this.load(this.family.value, this.style.value));
    this.set.addEventListener('change', () => this.fill());
    q(this.root, '.fg-more').addEventListener('click', () => this.more());
    this.root.addEventListener('click', (e) => {
      const b = (e.target as HTMLElement).closest<HTMLElement>('.fg-cell');
      if (b?.dataset.cp) void this.insert(Number(b.dataset.cp));
    });
    this.root.addEventListener('pointerover', (e) => {
      const b = (e.target as HTMLElement).closest<HTMLElement>('.fg-cell');
      if (b?.dataset.cp) setText(q(this.root, '.fg-code'), `${hex(Number(b.dataset.cp))}  ${String.fromCodePoint(Number(b.dataset.cp))}`);
    });
    editor.on('selection', () => { if (this.visible) this.follow(); });
  }

  async onShow(): Promise<void> {
    this.visible = true;
    if (!this.families.length) {
      try { this.families = ((await engine.call<string[]>('fonts.families')).result ?? []) as string[]; } catch { this.families = []; }
      for (const f of this.families) this.family.appendChild(new Option(f, f));
    }
    if (!this.faceKey) await this.pickFamily(this.families.includes('Source Serif 4') ? 'Source Serif 4' : this.families[0] ?? '');
    this.follow();
  }

  private follow(): void {
    window.clearTimeout(this.followTimer);
    this.followTimer = window.setTimeout(async () => {
      if (!editor.hasDoc) return;
      const r = await editor.exec<{ chars?: { fontFamily?: string; fontStyle?: string } }>('type.selectionAttrs', {}, true).catch(() => undefined);
      const a = r?.chars;
      if (!a?.fontFamily || !this.families.includes(a.fontFamily)) return;
      if (`${a.fontFamily}/${a.fontStyle ?? 'Regular'}` !== this.faceKey) await this.pickFamily(a.fontFamily, a.fontStyle);
    }, 120);
  }

  private async pickFamily(family: string, style?: string): Promise<void> {
    if (!family) return;
    setValue(this.family, family);
    let styles: string[] = [];
    try { styles = ((await engine.call<string[]>('fonts.styles', { family })).result ?? []) as string[]; } catch { styles = []; }
    if (!styles.length) styles = ['Regular'];
    const keep = this.style.options.length === styles.length && styles.every((s, i) => this.style.options[i].value === s);
    if (!keep) this.style.replaceChildren(...styles.map((s) => new Option(s, s)));
    const want = style && styles.includes(style) ? style : styles.includes('Regular') ? 'Regular' : styles[0];
    setValue(this.style, want);
    await this.load(family, want);
  }

  private async load(family: string, style: string): Promise<void> {
    const key = `${family}/${style}`;
    if (key === this.faceKey) return;
    this.faceKey = key;
    let name = this.loaded.get(key);
    if (!name) {
      name = `fd-glyph-${++this.seq}`;
      try {
        const face = new FontFace(name, `url(/api/core/font?family=${encodeURIComponent(family)}&style=${encodeURIComponent(style)})`);
        document.fonts.add(await face.load());
        this.loaded.set(key, name);
      } catch {
        toast(`Could not load ${family} ${style} for the Glyphs panel`, 'error');
        return;
      }
    }
    let chars: number[] = [];
    try { chars = (((await engine.call<{ chars: number[] }>('fonts.face', { family, style })).result) as { chars: number[] })?.chars ?? []; } catch { chars = []; }
    if (key !== this.faceKey) return;
    this.faceName = name;
    this.chars = chars.filter((c) => c > 0x20 && !(c >= 0x7f && c <= 0x9f) && !(c >= 0xe000 && c <= 0xf8ff)).sort((a, b) => a - b);
    this.root.style.setProperty('--fg-font', `"${name}"`);
    this.fill();
    this.renderRecent();
  }

  private list(): number[] {
    const s = SETS.find((x) => x[0] === this.set.value) ?? SETS[0];
    if (!s[2].length) return this.chars;
    return this.chars.filter((c) => s[2].some(([a, b]) => c >= a && c <= b));
  }

  private cell(cp: number): HTMLElement {
    const b = el<HTMLButtonElement>('<button class="fg-cell" type="button" role="gridcell"></button>');
    b.dataset.cp = String(cp);
    b.textContent = String.fromCodePoint(cp);
    b.title = hex(cp);
    return b;
  }

  private fill(): void {
    const all = this.list();
    this.shown = Math.min(PAGE, all.length);
    this.grid.replaceChildren(...all.slice(0, this.shown).map((c) => this.cell(c)));
    this.grid.scrollTop = 0;
    setText(q(this.root, '.fg-count'), `${all.length} glyph${all.length === 1 ? '' : 's'}`);
    setHidden(q(this.root, '.fg-more'), this.shown >= all.length);
  }

  private more(): void {
    const all = this.list();
    const next = Math.min(all.length, this.shown + PAGE);
    this.grid.append(...all.slice(this.shown, next).map((c) => this.cell(c)));
    this.shown = next;
    setHidden(q(this.root, '.fg-more'), this.shown >= all.length);
  }

  private renderRecent(): void {
    const box = q(this.root, '.fg-recent');
    const have = new Set(this.chars);
    const items = this.recent.filter((c) => have.has(c));
    setHidden(q(this.root, '.fg-recent-wrap'), !items.length);
    const key = items.join(',') + this.faceName;
    if (box.dataset.key === key) return;
    box.dataset.key = key;
    box.replaceChildren(...items.map((c) => this.cell(c)));
  }

  private async insert(cp: number): Promise<void> {
    if (!editor.wantsText) { toast('Click into a text frame with the Type tool, then pick a glyph'); return; }
    await editor.exec('text.insert', { text: String.fromCodePoint(cp), raw: true });
    this.recent = [cp, ...this.recent.filter((c) => c !== cp)].slice(0, 16);
    try { localStorage.setItem(RECENT_KEY, JSON.stringify(this.recent)); } catch { }
    this.renderRecent();
    this.ws.viewport.refreshOverlays();
  }
}

export function installGlyphs(ws: Workspace): void {
  let panel: GlyphsPanel | null = null;
  ws.registerPanel({
    id: 'glyphs', label: 'Glyphs', group: 'typeextra', column: 'side', order: 10,
    create: () => (panel = new GlyphsPanel(ws)).root,
    onShow: () => void panel?.onShow(),
  });
  ws.addCommands({ id: 'ui.glyphs', label: 'Glyphs', keys: ['Alt+Shift+F11'], run: () => ws.showPanel('glyphs') });
  ws.addMenuItems({ menu: 'Type', items: ['ui.glyphs'], at: 'end' });
}
