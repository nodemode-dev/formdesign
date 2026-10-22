import { q, setClass, setHidden, setTitle } from './dom';
import { icon } from './icons';

export const wantsCustomTitleBar = (): boolean => '__TAURI_INTERNALS__' in window && !/Mac/.test(navigator.platform);

type Win = ReturnType<typeof import('@tauri-apps/api/window').getCurrentWindow>;

export class CaptionButtons {
  private readonly box = document.createElement('div');
  private win: Promise<Win> | null = null;
  private on = false;
  private maximized = false;

  constructor(private readonly header: HTMLElement, private readonly wanted: () => boolean) {
    this.box.className = 'cv-caption';
    this.box.hidden = true;
    this.box.innerHTML =
      `<button class="cv-caption-btn" type="button" data-cap="min" title="Minimize" aria-label="Minimize">${icon('capMin', 16)}</button>` +
      `<button class="cv-caption-btn" type="button" data-cap="max" title="Maximize" aria-label="Maximize">${icon('capMax', 16)}</button>` +
      `<button class="cv-caption-btn is-close" type="button" data-cap="close" title="Close" aria-label="Close">${icon('close', 16)}</button>`;
    header.append(this.box);
    if ('__TAURI_INTERNALS__' in window) for (const n of header.querySelectorAll('.cv-menubar, .cv-zoom')) n.setAttribute('data-tauri-drag-region', '');
    this.box.addEventListener('click', (e) => {
      const b = e.target instanceof Element ? e.target.closest<HTMLElement>('[data-cap]') : null;
      if (b) void this.act(b.dataset.cap ?? '');
    });
    window.addEventListener('resize', () => { if (this.on) void this.look(); });
    this.sync();
  }

  get active(): boolean { return this.on; }

  sync(): void {
    const want = wantsCustomTitleBar() && this.wanted();
    if (want === this.on) return;
    this.on = want;
    setHidden(this.box, !want);
    setClass(document.documentElement, 'has-caption-buttons', want);
    void this.use((w) => w.setDecorations(!want)).then(() => this.look());
  }

  private async use<T>(run: (w: Win) => Promise<T>): Promise<T | undefined> {
    try {
      this.win ??= import('@tauri-apps/api/window').then((m) => m.getCurrentWindow());
      return await run(await this.win);
    } catch { return undefined; }
  }

  private async act(what: string): Promise<void> {
    if (what === 'min') await this.use((w) => w.minimize());
    else if (what === 'max') { await this.use((w) => w.toggleMaximize()); await this.look(); }
    else if (what === 'close') await this.use((w) => w.close());
  }

  private async look(): Promise<void> {
    const now = !!(await this.use((w) => w.isMaximized()));
    if (now === this.maximized && this.box.dataset.shown) return;
    this.maximized = now;
    this.box.dataset.shown = '1';
    const b = q<HTMLElement>(this.box, '[data-cap="max"]');
    b.innerHTML = icon(now ? 'capRestore' : 'capMax', 16);
    setTitle(b, now ? 'Restore' : 'Maximize');
    b.setAttribute('aria-label', now ? 'Restore' : 'Maximize');
  }
}
