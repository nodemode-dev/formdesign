import { editor } from '../../editor/editor';
import { Dialog } from '../../ui/dialogs';
import { delayedIndicator, el, q, setClass, setDisabled, setHidden, setText } from '../../ui/dom';
import { toast } from '../../ui/toast';
import type { Workspace } from '../../ui/workspace';
import { Check, copyText, field } from './form';
import { docIcon } from './icons';
import { capabilities, join, stem } from './paths';

interface PackageResult { dir?: string; files?: unknown[] | number; report?: unknown }

export class PackageDialog {
  private readonly d = new Dialog('Package', 560);
  private readonly dir = el<HTMLInputElement>('<input class="fd-f-text fd-f-mono" type="text" spellcheck="false" autocomplete="off">');
  private readonly idml = new Check('An IDML copy, for InDesign', true);
  private readonly pdf = new Check('A PDF for review');
  private readonly notes = el<HTMLTextAreaElement>('<textarea class="fd-f-area" rows="3" spellcheck="true" placeholder="Instructions for the printer or the next designer (optional)"></textarea>');
  private readonly result = el(`<div class="fd-ex-result" hidden><span class="fd-ex-ok">${docIcon('check', 15)}</span><div class="fd-ex-rtext"><div class="fd-ex-rhead"></div><div class="fd-ex-rpath"></div></div><button class="cv-icon-btn fd-ex-copy" type="button" title="Copy the path" aria-label="Copy the path">${docIcon('copy', 15)}</button></div>`);
  private readonly status = el('<span class="fd-ex-status" aria-live="polite"></span>');
  private readonly go: HTMLButtonElement;
  private readonly cancel: HTMLButtonElement;
  private running = false;
  private last = '';

  constructor(private readonly ws: Workspace) {
    q<HTMLElement>(this.d.scrim, '.cv-dialog').classList.add('fd-package');
    const lead = el(`<div class="fd-pk-lead">${docIcon('package', 22)}<p>Gathers the document, every placed file (relinked into a Links folder) and a report into one folder, ready to hand over.</p></div>`);
    const include = el('<div class="fd-f-checks"></div>');
    include.append(this.idml.root, this.pdf.root);
    this.d.body.append(lead, field('Folder', this.dir), field('Include', include), field('Instructions', this.notes), this.result);
    this.d.foot.appendChild(this.status);
    this.cancel = this.d.button('Cancel', 'ghost', () => this.d.close());
    this.go = this.d.button('Package', 'primary', () => void this.run());
    q(this.result, '.fd-ex-copy').addEventListener('click', () => void copyText(this.last).then((ok) => toast(ok ? 'Path copied' : 'Could not copy the path', ok ? 'info' : 'error')));
    this.d.scrim.addEventListener('keydown', (e) => { if (e.key === 'Enter' && (e.target as HTMLElement).tagName === 'INPUT') { e.preventDefault(); this.go.click(); } });
  }

  async open(): Promise<void> {
    if (!editor.hasDoc) { toast('Open a document to package it'); return; }
    setHidden(this.result, true);
    setText(this.cancel, 'Cancel');
    const name = `${stem(editor.doc?.title)} Folder`;
    this.dir.value = name;
    this.d.open(this.dir);
    const c = await capabilities();
    if (c.paths?.exports && this.dir.value === name) this.dir.value = join(c.paths.exports, name);
  }

  private async run(): Promise<void> {
    const dir = this.dir.value.trim();
    if (this.running || !dir) return;
    this.running = true;
    setDisabled(this.go, true);
    setHidden(this.result, true);
    const done = delayedIndicator(() => { setText(this.status, 'Packaging…'); setClass(this.status, 'is-busy', true); }, () => { setText(this.status, ''); setClass(this.status, 'is-busy', false); });
    const params: Record<string, unknown> = { dir, idml: this.idml.value, pdf: this.pdf.value };
    if (this.notes.value.trim()) params.instructions = this.notes.value.trim();
    const r = await this.ws.runCore('file.package', params) as PackageResult | undefined;
    done();
    this.running = false;
    setDisabled(this.go, false);
    if (!r) return;
    const where = r.dir ?? dir;
    const n = Array.isArray(r.files) ? r.files.length : typeof r.files === 'number' ? r.files : 0;
    this.last = where;
    setText(q(this.result, '.fd-ex-rhead'), n ? `Packaged ${n} ${n === 1 ? 'file' : 'files'}` : 'Packaged');
    setText(q(this.result, '.fd-ex-rpath'), where);
    setHidden(this.result, false);
    setText(this.cancel, 'Close');
    toast(`Packaged to ${where}`);
  }
}
