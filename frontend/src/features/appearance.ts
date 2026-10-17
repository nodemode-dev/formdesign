import { editor } from '../editor/editor';
import { el, patchList, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import { t } from '../ui/i18n';
import { icon } from '../ui/icons';
import type { Workspace } from '../ui/workspace';

interface Entry { kind?: string; swatch?: string; weight?: number; above?: boolean; on?: boolean; style?: string }
interface Item { id: number; fill?: string; stroke?: { swatch?: string; weight?: number }; appearance?: Entry[]; effects?: Entry[]; children?: Item[] }

/** One row of the panel: an extra fill or stroke, the main stroke or fill, or a live effect. */
interface Row { key: string; label: string; detail: string; list?: 'appearance' | 'effects'; index?: number; on: boolean }

/** An effect kind's name in the interface language (literal t() calls, so the strings are extracted for translation). */
function effectName(kind: string): string {
  const names: Record<string, () => string> = {
    roundCorners: () => t('Round Corners'), zigZag: () => t('Zig Zag'), roughen: () => t('Roughen'), puckerBloat: () => t('Pucker & Bloat'),
    twist: () => t('Twist'), transform: () => t('Transform'), warp: () => t('Warp'),
  };
  return names[kind]?.() ?? kind;
}

function find(list: Item[], id: number): Item | null {
  for (const it of list) {
    if (it.id === id) return it;
    const hit = it.children ? find(it.children, id) : null;
    if (hit) return hit;
  }
  return null;
}

function paintRow(e: Entry, list: 'appearance', index: number): Row {
  const stroke = e.kind === 'stroke';
  return { key: `a${index}`, label: t(stroke ? 'Stroke' : 'Fill'), detail: `${e.swatch ?? ''}${stroke && e.weight ? `  ${e.weight} pt` : ''}`, list, index, on: e.on !== false };
}

/** Rows top to bottom in drawing order: extra paints above the main ones, the main stroke and fill, extra paints
 * below, then the live effects in the order they apply. */
function rowsOf(it: Item): Row[] {
  const extra = (it.appearance ?? []).map((e, i) => ({ e, i }));
  const above = extra.filter((x) => x.e.above).reverse().map((x) => paintRow(x.e, 'appearance', x.i));
  const below = extra.filter((x) => !x.e.above).reverse().map((x) => paintRow(x.e, 'appearance', x.i));
  const main: Row[] = [
    { key: 'stroke', label: t('Stroke'), detail: `${it.stroke?.swatch ?? ''}${it.stroke?.weight ? `  ${it.stroke.weight} pt` : ''}`, on: true },
    { key: 'fill', label: t('Fill'), detail: it.fill ?? '', on: true },
  ];
  const fx = (it.effects ?? []).map((e, i): Row => ({ key: `e${i}`, label: effectName(e.kind ?? ''), detail: e.kind === 'warp' ? String(e.style ?? '') : '', list: 'effects', index: i, on: e.on !== false }));
  return [...above, ...main, ...below, ...fx];
}

class AppearancePanel {
  readonly root = el(`<div class="ap-panel">
    <div class="ap-bar">
      <button class="cv-btn ap-add" type="button">Add Fill or Stroke</button>
      <button class="cv-btn ap-fx" type="button">Add Effect</button>
      <button class="cv-btn ap-expand" type="button">Expand Appearance</button>
    </div>
    <div class="ap-list" role="list"></div>
    <p class="cv-note ap-empty">Select an object to see its appearance.</p>
  </div>`);
  private readonly list = q(this.root, '.ap-list');
  private item: Item | null = null;
  private sig = '';
  private timer = 0;
  visible = false;

  constructor(private readonly ws: Workspace) {
    q(this.root, '.ap-add').addEventListener('click', () => ws.commands.run('object.appearance'));
    q(this.root, '.ap-fx').addEventListener('click', () => ws.commands.run('object.effect'));
    q(this.root, '.ap-expand').addEventListener('click', () => ws.commands.run('object.expandAppearance'));
    editor.on('selection', () => this.schedule());
    editor.on('change', () => this.schedule());
  }

  schedule(): void {
    if (!this.visible) return;
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => void this.refresh(), 60);
  }

  async refresh(): Promise<void> {
    const id = editor.selection?.items?.[0];
    const data = id === undefined ? null : await editor.inspect();
    const spreads = (data?.spreads ?? []) as Array<{ items: Item[] }>;
    let it: Item | null = null;
    for (const s of spreads) { it = id === undefined ? null : find(s.items, id); if (it) break; }
    const rows = it ? rowsOf(it) : [];
    const sig = JSON.stringify([it?.id ?? null, rows]);
    if (sig === this.sig) return;
    this.sig = sig;
    this.item = it;
    setHidden(q(this.root, '.ap-empty'), !!it);
    for (const b of this.root.querySelectorAll<HTMLButtonElement>('.ap-bar button')) setDisabled(b, !it);
    setDisabled(q<HTMLButtonElement>(this.root, '.ap-expand'), !it || !((it.appearance?.length ?? 0) || (it.effects?.length ?? 0)));
    patchList<Row>(this.list, rows, {
      key: (r) => r.key,
      create: (r) => {
        const n = el(`<div class="ap-row" role="listitem">
          <button class="cv-icon-btn ap-eye" type="button" title="Visibility" aria-label="Visibility"></button>
          <span class="ap-label"></span><span class="ap-detail" data-no-i18n></span>
          <button class="cv-icon-btn ap-del" type="button" title="Remove" aria-label="Remove">${icon('delete', 14)}</button></div>`);
        q(n, '.ap-eye').addEventListener('click', () => void this.toggle(n.dataset.key as string));
        q(n, '.ap-del').addEventListener('click', () => void this.remove(n.dataset.key as string));
        this.fill(n, r);
        return n;
      },
      update: (n, r) => this.fill(n, r),
    });
  }

  private rows(): Row[] { return this.item ? rowsOf(this.item) : []; }

  private fill(n: HTMLElement, r: Row): void {
    setText(q(n, '.ap-label'), r.label);
    setText(q(n, '.ap-detail'), r.detail);
    const eye = q<HTMLButtonElement>(n, '.ap-eye');
    const want = r.on ? 'eye' : 'eyeOff';
    if (eye.dataset.icon !== want) { eye.dataset.icon = want; eye.innerHTML = icon(want, 14); }
    setClass(n, 'is-off', !r.on);
    setHidden(eye, !r.list);
    setHidden(q(n, '.ap-del'), !r.list);
  }

  private async run(command: string, params: Record<string, unknown>): Promise<void> {
    if (this.item) await editor.exec(command, { ...params, ids: [this.item.id] });
  }

  private async toggle(key: string): Promise<void> {
    const r = this.rows().find((x) => x.key === key);
    if (r?.list) await this.run(r.list === 'effects' ? 'object.effect' : 'object.appearance', { index: r.index, on: !r.on });
  }

  private async remove(key: string): Promise<void> {
    const r = this.rows().find((x) => x.key === key);
    if (r?.list) await this.run(r.list === 'effects' ? 'object.effect' : 'object.appearance', { remove: r.index });
  }
}

export function installAppearance(ws: Workspace): void {
  let panel: AppearancePanel | null = null;
  const make = (): AppearancePanel => (panel ??= new AppearancePanel(ws));
  ws.registerPanel({ id: 'appearance', label: 'Appearance', group: 'props', column: 'side', order: 40, create: () => make().root, onShow: () => { const p = make(); p.visible = true; p.schedule(); } });
  // The shell lists every panel in the Window menu (ui.panel.appearance), so no command of its own is needed.
}
