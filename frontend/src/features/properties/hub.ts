import { el, q, setClass, setHidden } from '../../ui/dom';
import type { Workspace } from '../../ui/workspace';
import { ObjectStylesPanel } from '../object-styles';
import { ctx } from './ctx';
import { StylesPanel } from './styles';

export type StyleKind = 'paragraph' | 'character' | 'object';

const KEY = 'formdesign.styles.kind';
const LABELS: Record<StyleKind, string> = { paragraph: 'Paragraph', character: 'Character', object: 'Object' };

export class StylesHub {
  readonly root = el('<div class="fs-hub"><div class="cv-seg fs-kinds" role="tablist" aria-label="Style kind"></div><div class="fs-pages"></div></div>');
  private readonly pages = new Map<StyleKind, HTMLElement>();
  private readonly objects: ObjectStylesPanel;
  private kind: StyleKind = 'paragraph';

  constructor(ws: Workspace) {
    this.objects = new ObjectStylesPanel(ws);
    const seg = q(this.root, '.fs-kinds');
    const host = q(this.root, '.fs-pages');
    const roots: Record<StyleKind, HTMLElement> = { paragraph: new StylesPanel('paragraph').root, character: new StylesPanel('character').root, object: this.objects.root };
    for (const k of ['paragraph', 'character', 'object'] as StyleKind[]) {
      const b = el<HTMLButtonElement>(`<button type="button" role="tab" data-kind="${k}">${LABELS[k]}</button>`);
      b.addEventListener('click', () => this.show(k));
      seg.appendChild(b);
      host.appendChild(roots[k]);
      this.pages.set(k, roots[k]);
    }
    try { const saved = localStorage.getItem(KEY); if (saved === 'paragraph' || saved === 'character' || saved === 'object') this.kind = saved; } catch { }
    this.sync();
  }

  show(kind: StyleKind): void {
    this.kind = kind;
    try { localStorage.setItem(KEY, kind); } catch { }
    this.sync();
    this.onShow();
  }

  onShow(): void {
    if (this.kind === 'object') this.objects.onShow();
    else ctx.schedule();
  }

  private sync(): void {
    for (const [k, page] of this.pages) setHidden(page, k !== this.kind);
    for (const b of this.root.querySelectorAll<HTMLButtonElement>('.fs-kinds button')) {
      const on = b.dataset.kind === this.kind;
      setClass(b, 'is-on', on);
      b.setAttribute('aria-selected', String(on));
    }
  }
}
