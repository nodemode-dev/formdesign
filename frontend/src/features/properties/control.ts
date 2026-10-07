import { el, q, setHidden, setHtml, setText } from '../../ui/dom';
import { ctx, type Snap } from './ctx';
import { glyph } from './glyphs';
import { ObjFields } from './objects';
import { CharFields, ParaFields } from './type';
import { formatMeasure } from './units';

export class ControlBar {
  readonly root = el('<div class="fp-ctl" role="toolbar" aria-label="Control"><div class="fp-ctl-strip" data-strip="obj"></div><div class="fp-ctl-strip" data-strip="text"></div><div class="fp-ctl-strip fp-ctl-doc" data-strip="doc"><span class="fp-ctl-kind"></span><span class="fp-ctl-read"></span></div></div>');
  private readonly o = new ObjFields(true);
  private readonly c = new CharFields(true);
  private readonly p = new ParaFields(true, ['left', 'center', 'right', 'leftJustified', 'fullyJustified']);

  constructor() {
    const o = this.o, c = this.c, p = this.p;
    const sep = () => el('<span class="cv-opt-sep"></span>');
    const obj = q(this.root, '[data-strip="obj"]');
    const xy = el('<div class="fp-ctl-pair"></div>');
    xy.append(o.x.el, o.y.el);
    const wh = el('<div class="fp-ctl-pair"></div>');
    wh.append(o.w.el, o.h.el);
    obj.append(o.ref.el, xy, wh, o.link.el, sep(), o.rot.el, o.flipH, o.flipV, sep(), o.fill.el, o.stroke.el, o.weight.el, sep(), o.opacity.el);
    const text = q(this.root, '[data-strip="text"]');
    text.append(el(`<span class="fp-ctl-glyph" title="Character">${glyph('fontSize', 16)}</span>`), c.family().el, c.style().el, c.size().el, c.leading().el, c.tracking().el, sep(), p.align().el, sep(), p.paraStyle().el);
    ctx.on((s) => this.update(s));
  }

  private update(s: Snap): void {
    const onObj = s.mode === 'objects';
    const textish = s.mode === 'text';
    setHidden(q(this.root, '[data-strip="obj"]'), !onObj);
    setHidden(q(this.root, '[data-strip="text"]'), !textish);
    setHidden(q(this.root, '[data-strip="doc"]'), onObj || textish || s.mode === 'nodoc');
    setHidden(this.root, s.mode === 'nodoc');
    if (onObj) this.o.update(s);
    if (textish) { this.c.update(s); this.p.update(s); }
    if (s.mode === 'none') {
      const st = s.setup;
      setHtml(q(this.root, '.fp-ctl-kind'), `${glyph('doc', 14)}<span>${s.title ? 'Document' : ''}</span>`, s.title ? 'doc' : '');
      setText(q(this.root, '.fp-ctl-read'), st ? `${formatMeasure(st.width, s.units)} × ${formatMeasure(st.height, s.vunits)} · ${st.pages} ${st.pages === 1 ? 'page' : 'pages'}${st.facingPages ? ' · facing' : ''} · nothing selected` : '');
    }
  }
}
