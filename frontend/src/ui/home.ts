import { api } from '../engine/client';
import { brandMark } from './brand';
import { DEMOS, demoCover } from './demos';
import { el, escapeHtml, isMac, patchList, setClass, setHidden, setStyleVar, setText, swapMedia } from './dom';
import { icon } from './icons';
import { onLanguage, t } from './i18n';

interface HomeTile { label: string; note: string; title: string; category: string; icon: string }
interface RecentItem { name: string; path: string; modified?: number }
type Page = 'start' | 'recent' | 'templates';

const TILES: HomeTile[] = [
  { label: 'Flyer or poster', note: 'One page for an event or sale', title: 'Show flyer or poster templates', category: 'print', icon: 'tileFlyer' },
  { label: 'Newsletter', note: 'News for your readers, in print or email', title: 'Show newsletter templates', category: 'newsletter', icon: 'tileNewsletter' },
  { label: 'Presentation', note: 'Slides for a talk or a pitch', title: 'Show presentation templates', category: 'presentation', icon: 'tilePresentation' },
  { label: 'Social and ads', note: 'Posts, banners and ads', title: 'Show social and ads templates', category: 'ad', icon: 'tileSocial' },
  { label: 'Website', note: 'A web page or a small site', title: 'Show website templates', category: 'web', icon: 'tileWebsite' },
  { label: 'Business stationery', note: 'Cards, letterheads and envelopes', title: 'Show business stationery templates', category: 'stationery', icon: 'tileStationery' },
];
const FORMATS = ['FDES', 'IDML', 'PDF', 'MD', 'DOCX', 'HTML', 'TXT', 'CSV', 'PNG', 'JPEG'];
const NAV_KEY = 'formdesign.homeNav';
const HIDDEN_KEY = 'formdesign.homeNavHidden';

export interface HomeDeps {
  run: (command: string, params?: Record<string, unknown>) => void;
  recent: () => RecentItem[];
  openRecent: (path: string) => void;
}

export class HomeScreen {
  readonly root = el('<section class="cv-home fd-home" aria-label="Home" hidden></section>');
  private page: Page = 'start';
  private forced = false;
  private docs = 0;
  private listKey = '';
  private tplAsked = false;

  constructor(host: HTMLElement, private readonly d: HomeDeps) {
    const mod = isMac ? '⌘' : 'Ctrl';
    const key = (k: string): string => `<kbd class="cv-key">${mod}</kbd><kbd class="cv-key">${k}</kbd>`;
    const page = (v: Page, ic: string, label: string, count = true): string => `<button class="cv-home-nav-item" type="button" data-v="${v}">${icon(ic, 16)}<span>${label}</span>${count ? `<span class="cv-home-count" data-count="${v}">0</span>` : ''}</button>`;
    const run = (cmd: string, ic: string, label: string, title = ''): string => `<button class="cv-home-nav-item" type="button" data-run="${cmd}"${title ? ` title="${title}"` : ''}>${icon(ic, 16)}<span>${label}</span></button>`;
    const pages = `${page('start', 'canvasGrid', 'Start', false)}${page('recent', 'history', 'Recent')}${page('templates', 'newFile', 'Templates')}`;
    const tiles = (counts: boolean): string => TILES.map((t) => `<button class="cv-home-tile" type="button" data-category="${t.category}" title="${t.title}">${icon(t.icon, 28)}<span class="cv-home-tile-label">${t.label}</span><span class="cv-home-tile-line">${t.note}</span>${counts ? `<span class="cv-home-tile-count" data-cat="${t.category}"></span>` : ''}</button>`).join('');
    this.root.innerHTML = `<nav class="cv-home-nav" aria-label="Home">
      <div class="cv-home-brand">${brandMark(26)}<span class="cv-home-brand-text">formdesign <span translate="no">by nodemode.ai</span></span></div>
      ${pages}
      <div class="cv-home-nav-sep"></div>
      <div class="cv-home-nav-title">Create</div>
      ${run('ui.make', 'makeLayout', 'Make a layout…')}${run('ui.templates', 'newFile', 'New from template…')}${run('ui.open', 'folderOpen', 'Open…')}
      <div class="cv-home-nav-sep"></div>
      <div class="cv-home-nav-title">Agents</div>
      ${run('ui.terminal.claude', 'agent', 'Claude Code')}${run('ui.claude', 'agentChat', 'Ask Claude', 'Describe what you want and Claude lays it out')}${run('ui.terminal.toggle', 'terminal', 'Terminal')}
      <div class="cv-home-nav-foot"><button class="cv-home-nav-item" type="button" data-back hidden>${icon('chevronLeft', 16)}<span>Back to document</span></button>${run('ui.prefs', 'settings', 'Preferences')}${run('ui.shortcuts', 'info', 'Help')}</div>
    </nav>
    <div class="cv-home-resizer" role="separator" aria-orientation="vertical" aria-label="Resize the side panel"></div>
    <button class="cv-home-nav-hide" type="button" title="Hide the side panel" aria-label="Hide the side panel">${icon('chevronLeft', 14)}</button>
    <button class="cv-home-nav-show" type="button" title="Show the side panel" aria-label="Show the side panel">${icon('chevronRight', 14)}</button>
    <div class="cv-home-main">
      <div class="cv-home-narrow">${pages}</div>
      <div class="cv-home-page" data-page="start">
        <div class="cv-home-head"><div><h1 class="cv-home-title"></h1><p class="cv-home-sub">Make a flyer, a newsletter, a report or a website. Pick a starting point below, describe what you want to Claude, or open a file.</p></div></div>
        <div class="cv-home-hero">
          <span class="cv-home-hero-icon">${icon('upload', 26)}</span>
          <div class="cv-home-hero-text"><div class="cv-home-hero-title">Start from your material</div>
            <div class="cv-home-hero-sub">Drop Markdown, PDF, Word, images or a folder anywhere to make a layout from them. Layout files open with every page, style and story still editable.</div>
            <div class="cv-home-formats">${FORMATS.map((f) => `<span class="cv-home-format">${f}</span>`).join('')}</div></div>
          <div class="cv-home-hero-actions">
            <button class="cv-home-action is-primary" type="button" data-run="ui.make">${icon('makeLayout', 16)}<span class="cv-home-label">Make a layout…</span>${key('M')}</button>
            <button class="cv-home-action" type="button" data-run="ui.open">${icon('folderOpen', 16)}<span class="cv-home-label">Open…</span>${key('O')}</button>
            <button class="cv-home-action is-ghost" type="button" data-run="ui.templates">${icon('newFile', 16)}<span class="cv-home-label">New from template…</span></button>
          </div>
        </div>
        <section class="cv-home-section"><div class="cv-home-section-head"><h2 class="cv-home-section-title">Demos</h2><span class="cv-home-note">Each opens as a new document. Nothing of yours is changed.</span></div><div class="cv-home-demos">${DEMOS.map((m) => `<button class="cv-home-demo" type="button" data-run="${m.command}" data-demo="${m.id}" aria-busy="false"><span class="cv-home-demo-pic"><img data-src="${demoCover(m)}" alt="" width="320" height="180" decoding="async" draggable="false"></span><span class="cv-home-demo-title">${m.title}</span><span>${m.description}</span></button>`).join('')}</div></section>
        <section class="cv-home-section"><div class="cv-home-section-head"><h2 class="cv-home-section-title">Recent</h2><button class="cv-btn cv-btn-ghost" type="button" data-v="recent">See all</button></div><div class="cv-home-files" data-files="start"></div><p class="cv-home-empty" data-empty="start">Documents you open appear here.</p></section>
        <section class="cv-home-section"><div class="cv-home-section-head"><h2 class="cv-home-section-title">Start with</h2><button class="cv-btn cv-btn-ghost" type="button" data-v="templates">See all</button></div><div class="cv-home-tiles">${tiles(false)}</div></section>
      </div>
      <div class="cv-home-page" data-page="recent" hidden><div class="cv-home-head"><div><h1 class="cv-home-title">Recent</h1><p class="cv-home-sub">Documents you opened or saved, newest first.</p></div></div><div class="cv-home-files" data-files="recent"></div><p class="cv-home-empty" data-empty="recent">Documents you open appear here.</p></div>
      <div class="cv-home-page" data-page="templates" hidden><div class="cv-home-head"><div><h1 class="cv-home-title">Templates</h1><p class="cv-home-sub">Original templates for print, screen and the web. Each opens as a new document with sample text and pictures.</p></div><div class="cv-home-head-actions"><button class="cv-btn" type="button" data-run="ui.templates">${icon('newFile', 14)}<span>Browse all templates…</span></button></div></div><div class="cv-home-tiles">${tiles(true)}</div></div>
    </div>`;
    this.root.addEventListener('click', (e) => {
      const t = e.target as HTMLElement;
      if (t.closest('[data-back]')) { this.forced = false; this.sync(this.docs); return; }
      const v = (t.closest('[data-v]') as HTMLElement | null)?.dataset.v as Page | undefined;
      if (v) { this.show(v); return; }
      const cmd = (t.closest('[data-run]') as HTMLElement | null)?.dataset.run;
      if (cmd) { this.d.run(cmd); return; }
      const cat = (t.closest('[data-category]') as HTMLElement | null)?.dataset.category;
      if (cat) { this.d.run('ui.templates', { category: cat }); return; }
      const path = (t.closest('[data-path]') as HTMLElement | null)?.dataset.path;
      if (path) this.d.openRecent(path);
    });
    host.appendChild(this.root);
    this.sidePanel();
    this.followDock();
    document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && this.forced && this.docs > 0 && !this.root.hidden) { this.forced = false; this.sync(this.docs); } });
    this.paint();
    onLanguage(() => { this.paint(); this.tplAsked = false; if (this.isShown) this.templates(); });
  }

  get isShown(): boolean { return !this.root.hidden; }

  show(page?: Page): void {
    if (page && page !== this.page) { this.page = page; this.paint(); }
    this.forced = true;
    this.sync(this.docs);
  }

  sync(docs: number): void {
    if (docs > this.docs) this.forced = false;
    this.docs = docs;
    const on = this.forced || docs === 0;
    if (on && this.root.hidden) this.paint();
    if (on) { this.fill(); this.templates(); }
    setHidden(this.root, !on);
    setHidden(this.root.querySelector('[data-back]') as HTMLElement, docs === 0);
  }

  private paint(): void {
    for (const n of Array.from(this.root.querySelectorAll<HTMLElement>('.cv-home-page'))) setHidden(n, n.dataset.page !== this.page);
    for (const n of Array.from(this.root.querySelectorAll<HTMLElement>('.cv-home-nav-item[data-v]'))) setClass(n, 'is-active', n.dataset.v === this.page);
    const h = new Date().getHours();
    setText(this.root.querySelector('[data-page="start"] .cv-home-title'), h < 12 ? t('Good morning') : h < 18 ? t('Good afternoon') : t('Good evening'));
  }

  private fill(): void {
    const items = this.d.recent();
    const key = JSON.stringify(items);
    if (key === this.listKey) return;
    this.listKey = key;
    for (const n of Array.from(this.root.querySelectorAll('[data-count="recent"]'))) setText(n, String(items.length));
    for (const [slot, list] of [['start', items.slice(0, 8)], ['recent', items]] as const) {
      setHidden(this.root.querySelector(`[data-empty="${slot}"]`) as HTMLElement, list.length > 0);
      patchList(this.root.querySelector(`[data-files="${slot}"]`) as HTMLElement, list, {
        key: (it) => it.path,
        create: (it) => this.card(it),
        update: (n, it) => { setText(n.querySelector('.cv-home-file-when'), when(it.modified)); this.thumb(n, it); },
      });
    }
  }

  private card(it: RecentItem): HTMLElement {
    const n = el(`<button class="cv-home-file" type="button" title="${escapeHtml(it.path)}"><span class="cv-home-file-thumb">${icon('fileImage', 22)}<img alt="" decoding="async" draggable="false" hidden></span><span class="cv-home-file-name"></span><span class="cv-home-file-when"></span></button>`);
    n.dataset.path = it.path;
    setText(n.querySelector('.cv-home-file-name'), it.name);
    setText(n.querySelector('.cv-home-file-when'), when(it.modified));
    this.thumb(n, it);
    return n;
  }

  private thumb(n: HTMLElement, it: RecentItem): void {
    if (!/\.fdes$/i.test(it.path)) return;
    const img = n.querySelector('img') as HTMLImageElement;
    const src = `/api/home/thumb?path=${encodeURIComponent(it.path)}&v=${it.modified ?? 0}`;
    if (img.dataset.src === src) return;
    img.dataset.src = src;
    void swapMedia(img, src).then(() => { setHidden(img, false); setClass(n, 'has-thumb', true); }, () => undefined);
  }

  private templates(): void {
    if (this.tplAsked) return;
    this.tplAsked = true;
    for (const img of Array.from(this.root.querySelectorAll<HTMLImageElement>('.cv-home-demo-pic img[data-src]'))) if (!img.getAttribute('src')) void swapMedia(img, img.dataset.src ?? '').catch(() => undefined);
    void api<{ categories: Record<string, number>; total: number }>('/templates').then((r) => {
      for (const n of Array.from(this.root.querySelectorAll('[data-count="templates"]'))) setText(n, String(r.total));
      for (const n of Array.from(this.root.querySelectorAll<HTMLElement>('[data-cat]'))) setText(n, r.categories[n.dataset.cat ?? ''] ? t('{n} templates').replace('{n}', String(r.categories[n.dataset.cat ?? ''])) : '');
    }, () => { this.tplAsked = false; });
  }

  private sidePanel(): void {
    const root = this.root;
    const bar = root.querySelector('.cv-home-resizer') as HTMLElement;
    const save = (): void => {
      try {
        localStorage.setItem(NAV_KEY, root.style.getPropertyValue('--fd-home-nav-w'));
        localStorage.setItem(HIDDEN_KEY, root.classList.contains('is-nav-hidden') ? '1' : '0');
      } catch { return; }
    };
    const hide = (on: boolean): void => { setClass(root, 'is-nav-hidden', on); save(); };
    try {
      const w = localStorage.getItem(NAV_KEY);
      if (w && /^\d+px$/.test(w.trim())) root.style.setProperty('--fd-home-nav-w', w.trim());
      if (localStorage.getItem(HIDDEN_KEY) === '1') root.classList.add('is-nav-hidden');
    } catch { void 0; }
    root.querySelector('.cv-home-nav-hide')?.addEventListener('click', () => hide(true));
    root.querySelector('.cv-home-nav-show')?.addEventListener('click', () => hide(false));
    bar.addEventListener('pointerdown', (e) => {
      e.preventDefault();
      const x0 = e.clientX;
      const start = root.querySelector('.cv-home-nav')?.getBoundingClientRect().width || 232;
      setClass(bar, 'is-dragging', true);
      const mv = (ev: PointerEvent): void => {
        const raw = start + ev.clientX - x0;
        const collapse = raw < 120;
        setClass(root, 'is-nav-hidden', collapse);
        if (!collapse) setStyleVar(root, '--fd-home-nav-w', `${Math.round(Math.max(180, Math.min(420, raw)))}px`);
      };
      const up = (): void => {
        setClass(bar, 'is-dragging', false);
        window.removeEventListener('pointermove', mv);
        window.removeEventListener('pointerup', up);
        save();
      };
      window.addEventListener('pointermove', mv);
      window.addEventListener('pointerup', up);
    });
  }

  private followDock(): void {
    const dock = document.querySelector<HTMLElement>('.fd-dock');
    if (!dock || typeof ResizeObserver === 'undefined') return;
    new ResizeObserver(() => setStyleVar(this.root, '--fd-home-dock', `${dock.offsetHeight}px`)).observe(dock);
  }
}

function when(ms?: number): string {
  return ms ? new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' }) : '';
}
