import { api } from '../../engine/client';
import { editor, friendly } from '../../editor/editor';
import { Dialog } from '../../ui/dialogs';
import { delayedIndicator, el, patchList, q, setClass, setDisabled, setHidden, setText, setTitle } from '../../ui/dom';
import { toast } from '../../ui/toast';
import { copyText } from './form';
import { docIcon } from './icons';
import { formatBytes } from './units';
import { WebOptions } from './web-options';

interface Site {
  slug: string; title: string; url: string; path: string; doc: string | null; updated: number; pages: number;
  files: number; bytes: number; split: string; warnings?: string[]; options?: Record<string, unknown>;
}

const SPLITS: Record<string, string> = { articles: 'an article per page', pages: 'a page per page', single: 'one page' };

export class PublishDialog {
  private readonly d = new Dialog('Publish Online', 760);
  private readonly web = new WebOptions();
  private readonly list = el<HTMLUListElement>('<ul class="fd-pub-list" aria-label="Published sites"></ul>');
  private readonly empty = el('<p class="fd-pub-empty">Nothing published yet. Publish the open document and its site appears here.</p>');
  private readonly count = el('<span class="fd-pub-count"></span>');
  private readonly result = el(`<div class="fd-ex-result fd-pub-result" hidden><span class="fd-ex-ok">${docIcon('check', 15)}</span><div class="fd-ex-rtext"><div class="fd-ex-rhead"></div><a class="fd-ex-rpath fd-pub-link" target="_blank" rel="noopener"></a><ul class="fd-ex-warn"></ul></div><button class="cv-icon-btn fd-ex-copy" type="button" title="Open the site" aria-label="Open the site">${docIcon('external', 15)}</button></div>`);
  private readonly status = el('<span class="fd-ex-status" aria-live="polite"></span>');
  private readonly go: HTMLButtonElement;
  private sites: Site[] = [];
  private current: Site | null = null;
  private running = false;
  private armed = '';
  private armTimer = 0;

  constructor() {
    q<HTMLElement>(this.d.scrim, '.cv-dialog').classList.add('fd-publish');
    const head = el(`<div class="fd-pub-head">${docIcon('globe', 16)}<span>Published sites</span></div>`);
    head.appendChild(this.count);
    const sites = el('<section class="fd-pub-sites"></section>');
    sites.append(head, this.list, this.empty, el('<p class="fd-f-note">FormDesign serves these sites from this computer. To put one on the web, download it as a .zip and upload the folder to any static host (GitHub Pages, Netlify, Cloudflare Pages or a bucket), or export straight into such a folder from File &gt; Export &gt; Website.</p>'));
    const opts = el('<section class="fd-pub-opts"></section>');
    opts.append(this.web.root);
    this.d.body.append(opts, this.result, sites);
    this.d.body.classList.add('fd-pub-body');
    this.d.foot.appendChild(this.status);
    this.d.button('Close', 'ghost', () => this.d.close());
    this.go = this.d.button('Publish', 'primary', () => void this.publish());
    q(this.result, '.fd-ex-copy').addEventListener('click', () => { const u = q<HTMLAnchorElement>(this.result, '.fd-pub-link').href; if (u) void this.openUrl(u); });
    this.list.addEventListener('click', (e) => {
      const b = (e.target as HTMLElement).closest<HTMLButtonElement>('button[data-act]');
      const li = b?.closest<HTMLElement>('[data-key]');
      const site = li ? this.sites.find((s) => s.slug === li.dataset.key) : undefined;
      if (b && site) void this.act(b.dataset.act ?? '', site, b);
    });
    this.d.onClose = () => this.disarm();
  }

  async open(): Promise<void> {
    if (!editor.hasDoc) { toast('Open a document to publish it'); return; }
    setHidden(this.result, true);
    setText(this.status, '');
    this.current = null;
    setDisabled(this.go, true);
    this.d.open(this.go);
    await this.refresh(true);
    setDisabled(this.go, this.running);
  }

  private async refresh(adopt = false): Promise<void> {
    let active: string | null = null;
    try {
      const r = await api<{ sites: Site[]; active: string | null }>('/website/published');
      this.sites = r.sites.sort((a, b) => b.updated - a.updated);
      active = r.active;
    } catch (err) {
      toast(friendly((err as Error).message), 'error');
      return;
    }
    this.current = this.sites.find((s) => s.slug === active) ?? null;
    if (adopt) {
      if (this.current?.options) this.web.set(this.current.options);
      else this.web.reset();
    }
    setText(this.go, this.current ? 'Update' : 'Publish');
    setTitle(this.go, this.current ? `Publish again to ${this.current.url}` : 'Publish the open document as a website');
    setText(this.count, this.sites.length ? String(this.sites.length) : '');
    setHidden(this.empty, this.sites.length > 0);
    patchList(this.list, this.sites, {
      key: (s) => s.slug,
      create: (s) => {
        const li = el(`<li class="fd-pub-row"><div class="fd-pub-txt"><div class="fd-pub-title"></div><a class="fd-pub-url" target="_blank" rel="noopener"></a><div class="fd-pub-meta"></div></div><div class="fd-pub-acts">
          <button class="cv-icon-btn" type="button" data-act="open" title="Open in the browser" aria-label="Open in the browser">${docIcon('external', 15)}</button>
          <button class="cv-icon-btn" type="button" data-act="copy" title="Copy the link" aria-label="Copy the link">${docIcon('copy', 15)}</button>
          <button class="cv-icon-btn" type="button" data-act="folder" title="Show the folder" aria-label="Show the folder">${docIcon('folder', 15)}</button>
          <button class="cv-icon-btn" type="button" data-act="zip" title="Download as .zip" aria-label="Download as .zip">${docIcon('download', 15)}</button>
          <button class="cv-icon-btn fd-pub-remove" type="button" data-act="remove" title="Unpublish" aria-label="Unpublish">${docIcon('remove', 15)}</button></div></li>`);
        this.fill(li, s);
        return li;
      },
      update: (li, s) => this.fill(li, s),
    });
  }

  private fill(li: HTMLElement, s: Site): void {
    setText(q(li, '.fd-pub-title'), s.title);
    const a = q<HTMLAnchorElement>(li, '.fd-pub-url');
    if (a.href !== s.url) a.href = s.url;
    setText(a, s.url.replace(/^https?:\/\//, ''));
    const when = new Date(s.updated * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
    setText(q(li, '.fd-pub-meta'), `${s.pages} page${s.pages === 1 ? '' : 's'}, ${SPLITS[s.split] ?? s.split} · ${formatBytes(s.bytes)} · ${when}`);
    setClass(li, 'is-current', s.slug === this.current?.slug);
  }

  private async act(act: string, s: Site, b: HTMLButtonElement): Promise<void> {
    try {
      if (act === 'open') await this.openUrl(s.url);
      else if (act === 'copy') toast((await copyText(s.url)) ? 'Link copied' : 'Could not copy the link', 'info');
      else if (act === 'folder') await api('/system/open', { json: { path: s.path } });
      else if (act === 'zip') {
        const blob = await api<Blob>(`/website/zip/${encodeURIComponent(s.slug)}`);
        const url = URL.createObjectURL(blob);
        const a = el<HTMLAnchorElement>('<a class="fd-f-copybuf"></a>');
        a.href = url;
        a.download = `${s.slug}.zip`;
        document.body.appendChild(a);
        a.click();
        a.remove();
        window.setTimeout(() => URL.revokeObjectURL(url), 20000);
      } else if (act === 'remove') {
        if (this.armed !== s.slug) {
          this.disarm();
          this.armed = s.slug;
          setClass(b, 'is-armed', true);
          setTitle(b, 'Click again to unpublish');
          this.armTimer = window.setTimeout(() => this.disarm(), 3000);
          return;
        }
        this.disarm();
        await api('/website/unpublish', { json: { slug: s.slug } });
        toast(`Unpublished ${s.title}`);
        await this.refresh();
      }
    } catch (err) {
      toast(friendly((err as Error).message), 'error');
    }
  }

  private disarm(): void {
    window.clearTimeout(this.armTimer);
    this.armed = '';
    for (const b of Array.from(this.list.querySelectorAll<HTMLButtonElement>('.fd-pub-remove.is-armed'))) { setClass(b, 'is-armed', false); setTitle(b, 'Unpublish'); }
  }

  private async openUrl(url: string): Promise<void> {
    try { await api('/system/open', { json: { url } }); } catch { window.open(url, '_blank', 'noopener'); }
  }

  private async publish(): Promise<void> {
    if (this.running || !editor.hasDoc) return;
    const bad = this.web.problem();
    if (bad) { toast(bad, 'error'); return; }
    this.running = true;
    setDisabled(this.go, true);
    setHidden(this.result, true);
    const done = delayedIndicator(() => { setText(this.status, 'Publishing…'); setClass(this.status, 'is-busy', true); }, () => { setText(this.status, ''); setClass(this.status, 'is-busy', false); });
    try {
      const r = await api<Site>('/website/publish', { json: { options: this.web.values(), slug: this.current?.slug } });
      setText(q(this.result, '.fd-ex-rhead'), `Published ${r.title} · ${r.pages} page${r.pages === 1 ? '' : 's'} · ${formatBytes(r.bytes)}`);
      const a = q<HTMLAnchorElement>(this.result, '.fd-pub-link');
      a.href = r.url;
      setText(a, r.url);
      const ul = q(this.result, '.fd-ex-warn');
      const notes = r.warnings ?? [];
      ul.replaceChildren(...notes.slice(0, 6).map((w) => { const li = document.createElement('li'); li.textContent = w; return li; }));
      setHidden(ul, notes.length === 0);
      setHidden(this.result, false);
      toast(`Published at ${r.url}`);
      await this.refresh();
    } catch (err) {
      toast(friendly((err as Error).message), 'error');
    } finally {
      done();
      this.running = false;
      setDisabled(this.go, false);
      if (this.d.scrim.isConnected && !this.d.scrim.contains(document.activeElement)) this.go.focus();
    }
  }
}
