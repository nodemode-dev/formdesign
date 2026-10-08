import { el, setClass, setText, setValue } from '../../ui/dom';
import { field, Pick, row } from './form';

export type WebSplit = 'auto' | 'articles' | 'pages' | 'single';

export class WebOptions {
  readonly root = el('<div class="fd-web-opts"></div>');
  private readonly title = el<HTMLInputElement>('<input class="fd-f-text" type="text" spellcheck="false" placeholder="From the document">');
  private readonly desc = el<HTMLInputElement>('<input class="fd-f-text" type="text" placeholder="From the deck or the first paragraph">');
  private readonly lang = el<HTMLInputElement>('<input class="fd-f-text" type="text" spellcheck="false" placeholder="From the text">');
  private readonly base = el<HTMLInputElement>('<input class="fd-f-text" type="url" spellcheck="false" placeholder="https://example.com/issue-12">');
  private readonly split = new Pick<WebSplit>([
    { value: 'auto', label: 'Automatic' },
    { value: 'articles', label: 'One per article' },
    { value: 'pages', label: 'One per page' },
    { value: 'single', label: 'One long page' },
  ], 'auto', 'Web pages');
  private readonly layout = new Pick<string>([
    { value: '', label: 'From the template' },
    { value: 'article', label: 'Article' },
    { value: 'landing', label: 'Landing page' },
    { value: 'card', label: 'Card' },
    { value: 'email', label: 'Email' },
    { value: 'docs', label: 'Docs' },
  ], '', 'Layout');
  private readonly edition = new Pick<string>([
    { value: '', label: 'Automatic' },
    { value: 'yes', label: 'Include' },
    { value: 'no', label: 'Leave out' },
  ], '', 'Edition');
  private readonly note = el('<p class="fd-f-note"></p>');

  constructor() {
    this.root.append(
      row(field('Site title', this.title), field('Language', this.lang)),
      field('Description', this.desc),
      row(field('Web pages', this.split.root), field('Layout', this.layout.root), field('Edition', this.edition.root)),
      field('Address', this.base),
      this.note,
    );
    this.base.addEventListener('input', () => this.syncNote());
    this.syncNote();
  }

  reset(): void {
    for (const i of [this.title, this.desc, this.lang, this.base]) setValue(i, '');
    this.split.set('auto');
    this.layout.set('');
    this.edition.set('');
    this.syncNote();
  }

  set(o: Record<string, unknown>): void {
    setValue(this.title, String(o.title ?? ''));
    setValue(this.desc, String(o.description ?? ''));
    setValue(this.lang, String(o.language ?? ''));
    setValue(this.base, String(o.baseUrl ?? ''));
    this.split.set((o.split as WebSplit) ?? 'auto');
    this.layout.set(String(o.layout ?? ''));
    this.edition.set(o.edition === true ? 'yes' : o.edition === false ? 'no' : '');
    this.syncNote();
  }

  problem(): string {
    const b = this.base.value.trim();
    return b && !/^https?:\/\/[^\s/]+\.[^\s]+/.test(b) ? 'The address must be a full web address, like https://example.com/issue-12' : '';
  }

  values(): Record<string, unknown> {
    const o: Record<string, unknown> = { split: this.split.value };
    const put = (k: string, v: string) => { if (v.trim()) o[k] = v.trim(); };
    put('title', this.title.value);
    put('description', this.desc.value);
    put('language', this.lang.value);
    put('baseUrl', this.base.value.replace(/\/+$/, ''));
    if (this.layout.value) o.layout = this.layout.value;
    if (this.edition.value) o.edition = this.edition.value === 'yes';
    return o;
  }

  private syncNote(): void {
    const bad = this.problem();
    setClass(this.note, 'is-warn', !!bad);
    setText(this.note, bad || (this.base.value.trim()
      ? 'Adds a sitemap, an RSS feed for articles and share links that point at this address.'
      : 'Where the site will live, if you know it: it adds a sitemap, a feed and share links. Leave it empty for a site opened from a folder.'));
  }
}
