import { editor } from '../../editor/editor';
import { Dialog } from '../../ui/dialogs';
import { el, q, setClass, setHidden, setText } from '../../ui/dom';
import { toast } from '../../ui/toast';
import type { Workspace } from '../../ui/workspace';
import { Check, field, IntField, LengthField, Pick, row, Seg } from './form';
import { docIcon } from './icons';
import { readUnits, UNITS, type Unit } from './units';

type Intent = 'print' | 'web' | 'mobile';

interface Setup { width: number; height: number; pages: number; startPage: number; facingPages: boolean; intent: Intent; bleed: number[] | number }

export class DocumentSetupDialog {
  private readonly d = new Dialog('Document Setup', 560);
  private readonly intent = new Seg<Intent>([
    { value: 'print', label: 'Print', icon: docIcon('print', 15) },
    { value: 'web', label: 'Web', icon: docIcon('web', 15) },
    { value: 'mobile', label: 'Mobile', icon: docIcon('mobile', 15) },
  ], 'print');
  private readonly pages = new IntField(1, 9999);
  private readonly start = new IntField(1, 99999);
  private readonly facing = new Check('Facing pages');
  private readonly width = new LengthField('pt', 1, 15552);
  private readonly height = new LengthField('pt', 1, 15552);
  private readonly units = new Pick<Unit>(UNITS.map((u) => ({ value: u, label: u })), 'pt', 'Units');
  private readonly orient = new Seg<'portrait' | 'landscape'>([
    { value: 'portrait', icon: docIcon('portrait', 16), title: 'Portrait' },
    { value: 'landscape', icon: docIcon('landscape', 16), title: 'Landscape' },
  ], 'portrait');
  private readonly bleed = new LengthField('pt', 0, 432);
  private readonly adjust = new Check('Adjust layout: objects follow the new page size');
  private readonly warn = el('<p class="fd-f-note is-warn" hidden></p>');
  private readonly ok: HTMLButtonElement;
  private before: Setup | null = null;

  constructor(private readonly ws: Workspace) {
    q<HTMLElement>(this.d.scrim, '.cv-dialog').classList.add('fd-setup');
    const dims = el('<div class="fd-nd-dims"></div>');
    dims.append(field('Width', this.width.root), field('Height', this.height.root), field('Units', this.units.root));
    const facingBox = el('<div class="fd-f-field fd-nd-facing"></div>');
    facingBox.appendChild(this.facing.root);
    const checks = el('<div class="fd-f-checks"></div>');
    checks.appendChild(this.adjust.root);
    this.d.body.append(
      field('Intent', this.intent.root),
      row(field('Pages', this.pages.root), field('Start page', this.start.root), facingBox),
      el('<div class="fd-f-sep"></div>'),
      dims,
      row(field('Orientation', this.orient.root), field('Bleed', this.bleed.root)),
      checks,
      this.warn,
    );
    this.d.button('Cancel', 'ghost', () => this.d.close());
    this.ok = this.d.button('OK', 'primary', () => void this.apply());
    this.units.onChange = (u) => { for (const f of [this.width, this.height, this.bleed]) f.setUnit(u); };
    const sync = () => { this.orient.set(this.width.value > this.height.value ? 'landscape' : 'portrait'); this.check(); };
    this.width.onChange = sync;
    this.height.onChange = sync;
    this.pages.onChange = () => this.check();
    this.orient.onChange = (o) => {
      const w = this.width.value, h = this.height.value;
      if ((o === 'landscape') !== (w > h) && w !== h) { this.width.set(h); this.height.set(w); }
      this.check();
    };
    this.d.scrim.addEventListener('keydown', (e) => { if (e.key === 'Enter' && (e.target as HTMLElement).tagName === 'INPUT') { e.preventDefault(); window.setTimeout(() => this.ok.click(), 0); } });
  }

  async open(): Promise<void> {
    if (!editor.hasDoc) { toast('Open a document first'); return; }
    const s = await editor.exec<Setup>('layout.documentSetup', {});
    if (!s) return;
    this.before = s;
    const u = readUnits();
    this.units.set(u);
    for (const f of [this.width, this.height, this.bleed]) f.setUnit(u);
    this.intent.set(s.intent ?? 'print');
    this.pages.set(s.pages);
    this.start.set(s.startPage ?? 1);
    this.facing.set(!!s.facingPages);
    this.width.set(s.width);
    this.height.set(s.height);
    this.orient.set(s.width > s.height ? 'landscape' : 'portrait');
    this.bleed.set(Array.isArray(s.bleed) ? s.bleed[0] ?? 0 : s.bleed ?? 0);
    this.adjust.set(false);
    this.check();
    this.d.open(this.ok);
  }

  private check(): void {
    const b = this.before;
    const lose = b ? b.pages - this.pages.value : 0;
    setHidden(this.warn, lose <= 0);
    if (lose > 0 && b) setText(this.warn, lose === 1 ? `Page ${b.pages} and what is on it will be removed.` : `Pages ${b.pages - lose + 1} to ${b.pages} and what is on them will be removed.`);
    setClass(this.warn, 'is-warn', lose > 0);
  }

  private async apply(): Promise<void> {
    for (const inp of Array.from(this.d.body.querySelectorAll('input'))) inp.dispatchEvent(new Event('change'));
    const b = this.before;
    const p: Record<string, unknown> = {};
    const near = (a: number, c: number) => Math.abs(a - c) < 0.01;
    if (!b || !near(b.width, this.width.value)) p.width = this.width.value;
    if (!b || !near(b.height, this.height.value)) p.height = this.height.value;
    if (!b || b.pages !== this.pages.value) p.pages = this.pages.value;
    if (!b || (b.startPage ?? 1) !== this.start.value) p.startPage = this.start.value;
    if (!b || !!b.facingPages !== this.facing.value) p.facingPages = this.facing.value;
    if (!b || b.intent !== this.intent.value) p.intent = this.intent.value;
    const oldBleed = b ? (Array.isArray(b.bleed) ? b.bleed : [b.bleed, b.bleed, b.bleed, b.bleed]) : [];
    if (!b || oldBleed.some((x) => !near(Number(x), this.bleed.value))) p.bleed = this.bleed.value;
    if ((p.width !== undefined || p.height !== undefined) && this.adjust.value) p.adjustLayout = true;
    this.d.close();
    if (!Object.keys(p).length) return;
    await this.ws.runCore('layout.documentSetup', p);
  }
}
