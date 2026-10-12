import { el, q, setHidden, setText } from '../../ui/dom';
import { ctx, type Snap } from './ctx';
import { grid, labelled, row } from './fields';
import { VariationAxes } from './axes';
import { CharFields, ParaFields, textTarget } from './type';

function hint(s: Snap): string {
  if (s.mode === 'nodoc') return 'Open a document to format its text.';
  if (s.mode === 'text') return '';
  if (s.mode === 'objects' && s.kinds.has('text')) return 'Changes apply to every story in the selected text frames.';
  return 'Select text, or a text frame, to format it.';
}

export class CharacterPanel {
  readonly root = el('<div class="fp-panel fp-typep"><div class="fp-type-body"></div><p class="fp-hint"></p></div>');
  private readonly f = new CharFields();
  private readonly axes = new VariationAxes();

  constructor() {
    const f = this.f, body = q(this.root, '.fp-type-body');
    const toggles = el('<div class="fp-togs"></div>');
    toggles.append(f.allCaps().el, f.smallCaps().el, f.superscript().el, f.subscript().el, f.underline().el, f.strike().el);
    body.append(
      f.family().el,
      f.style().el,
      grid(f.size().el, f.leading().el),
      grid(f.kerning().el, f.tracking().el),
      grid(f.vScale().el, f.hScale().el),
      grid(f.baseline().el, f.skew().el),
      toggles,
      labelled('Colour', f.fill().el),
      labelled('Style', f.charStyle().el),
      labelled('Language', f.language().el),
      this.axes.el,
    );
    ctx.on((s) => this.update(s));
  }

  private update(s: Snap): void {
    this.f.update(s);
    this.axes.update(s);
    const h = hint(s);
    setText(q(this.root, '.fp-hint'), h);
    setHidden(q(this.root, '.fp-hint'), !h);
    this.root.classList.toggle('is-off', !textTarget(s));
  }
}

export class ParagraphPanel {
  readonly root = el('<div class="fp-panel fp-typep"><div class="fp-type-body"></div><p class="fp-hint"></p></div>');
  private readonly f = new ParaFields();

  constructor() {
    const f = this.f, body = q(this.root, '.fp-type-body');
    const al = f.align().el;
    al.classList.add('fp-seg-align');
    for (const id of ['leftJustified', 'towardsSpine']) f.align().button(id)?.classList.add('fp-seg-gap');
    body.append(
      al,
      grid(f.left().el, f.right().el),
      grid(f.first().el, f.last().el),
      grid(f.before().el, f.after().el),
      grid(f.dropLines().el, f.dropChars().el),
      labelled('Lists', f.list().el),
      labelled('Grid', f.grid().el),
      row(f.hyphenate().el),
      row(f.keepTogether().el, f.keepNext().el),
      labelled('Style', f.paraStyle().el),
    );
    ctx.on((s) => this.update(s));
  }

  private update(s: Snap): void {
    this.f.update(s);
    const h = hint(s);
    setText(q(this.root, '.fp-hint'), h);
    setHidden(q(this.root, '.fp-hint'), !h);
    this.root.classList.toggle('is-off', !textTarget(s));
  }
}
