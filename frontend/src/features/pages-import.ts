import { editor } from '../editor/editor';
import { Dialog } from '../ui/dialogs';
import { el, setDisabled, setHidden } from '../ui/dom';
import { t } from '../ui/i18n';
import type { Workspace } from '../ui/workspace';

const FILE = 'file';

/** Layout > Pages > Insert Pages from Document: InDesign's Move Pages to another document and Affinity's Merge
 *  Document over layout.pages.import. The lists are filled when the dialog opens, not while it is open. */
class PagesImport {
  private readonly d = new Dialog(t('Insert Pages from Document'), 460);
  private readonly source = el<HTMLSelectElement>('<select class="cv-select"></select>');
  private readonly path = el<HTMLInputElement>('<input class="cv-input" type="text" spellcheck="false" autocomplete="off">');
  private readonly pages = el<HTMLInputElement>('<input class="cv-input" type="text" spellcheck="false" autocomplete="off">');
  private readonly where = el<HTMLSelectElement>('<select class="cv-select"></select>');
  private readonly move = el<HTMLInputElement>('<input type="checkbox">');
  private readonly pathRow: HTMLElement;
  private readonly moveRow: HTMLElement;

  constructor() {
    this.path.placeholder = t('Path of a .fdes, .fdoc or .idml file');
    this.pages.placeholder = t('All, or a range such as 2-4, 7');
    this.d.field(t('From'), this.source);
    this.pathRow = this.d.field(t('File'), this.path);
    this.d.field(t('Pages'), this.pages);
    this.d.field(t('Insert'), this.where);
    this.moveRow = el(`<label class="cv-opt"></label>`);
    this.moveRow.append(this.move, el(`<span>${t('Move the pages (take them out of the source)')}</span>`));
    this.d.body.appendChild(this.moveRow);
    this.source.addEventListener('change', () => this.sync());
    this.d.button(t('Cancel'), 'ghost', () => this.d.close());
    this.d.button(t('OK'), 'primary', () => void this.apply());
  }

  private sync(): void {
    const file = this.source.value === FILE;
    setHidden(this.pathRow, !file);
    setDisabled(this.move, file);
    if (file) this.move.checked = false;
  }

  open(): void {
    const here = editor.active;
    const others = editor.docs.filter((d) => d.index !== here);
    this.source.replaceChildren(...others.map((d) => new Option(`${d.title} (${d.pages} ${d.pages === 1 ? t('page') : t('pages')})`, String(d.index))), new Option(t('A file on disk...'), FILE));
    const n = editor.doc?.pages ?? 1;
    this.where.replaceChildren(new Option(t('At the start'), '-1'), ...Array.from({ length: n }, (_, i) => new Option(t('After page {n}').replace('{n}', String(i + 1)), String(i))));
    this.where.value = String(n - 1);
    this.pages.value = '';
    this.move.checked = false;
    this.sync();
    this.d.open(others.length ? this.source : this.path);
  }

  private async apply(): Promise<void> {
    const file = this.source.value === FILE;
    const from: number | string = file ? this.path.value.trim() : Number(this.source.value);
    if (file && !from) { this.path.focus(); return; }
    const p: Record<string, unknown> = { from, after: Number(this.where.value) };
    const range = this.pages.value.trim();
    if (range && range.toLowerCase() !== 'all') p.pages = range;
    if (this.move.checked && !file) p.move = true;
    const r = await editor.exec('layout.pages.import', p);
    if (r !== undefined) this.d.close();
  }
}

export function installPagesImport(ws: Workspace): void {
  let dlg: PagesImport | null = null;
  ws.override('layout.pages.import', () => (dlg ??= new PagesImport()).open());
}
