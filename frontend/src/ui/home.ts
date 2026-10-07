import { el, setHidden, setText } from './dom';
import { icon } from './icons';

export interface HomeAction { label: string; command: string; icon: string; key?: string }

export interface HomeDeps {
  run: (command: string) => void;
  recent: () => Array<{ name: string; path: string; modified?: number }>;
  openRecent: (path: string) => void;
}

export class HomeScreen {
  readonly root = el('<section class="cv-home fd-home" aria-label="Home" hidden></section>');
  private readonly list = el('<div class="cv-home-list"></div>');
  private readonly recentBox = el('<div class="cv-home-main" hidden><h2 class="cv-home-title">Recent</h2></div>');
  private listKey = '';
  private forced = false;
  private docs = 0;

  constructor(host: HTMLElement, private readonly d: HomeDeps, actions: HomeAction[]) {
    const mod = /Mac|iPhone|iPad/.test(navigator.platform) ? '⌘' : 'Ctrl';
    const center = el(`<div class="cv-home-center">
      <h1 class="cv-home-brand">formdes<span class="cv-home-by">by NodeMode</span></h1>
      <p class="cv-home-sub">Lay out a magazine, a site, a report or an ad. Start from a template, make one from your files, or open a document.</p>
      <div class="cv-home-actions"></div>
      <p class="cv-home-hint">${icon('upload', 14)}<span>Drop Markdown, PDF, Word, images or a folder anywhere to make a layout from them.</span></p>
    </div>`);
    const row = center.querySelector('.cv-home-actions') as HTMLElement;
    for (const a of actions) {
      const b = el<HTMLButtonElement>(`<button class="cv-home-action" type="button">${icon(a.icon, 16)}<span class="cv-home-label"></span>${a.key ? '<kbd class="cv-key"></kbd><kbd class="cv-key"></kbd>' : ''}</button>`);
      setText(b.querySelector('.cv-home-label') as HTMLElement, a.label);
      if (a.key) {
        const [k1, k2] = Array.from(b.querySelectorAll('kbd'));
        setText(k1, mod);
        setText(k2, a.key);
      }
      b.dataset.command = a.command;
      b.addEventListener('click', () => this.d.run(a.command));
      row.appendChild(b);
    }
    this.recentBox.appendChild(this.list);
    center.appendChild(this.recentBox);
    this.root.append(center);
    host.appendChild(this.root);
    document.addEventListener('keydown', (e) => { if (e.key === 'Escape' && this.forced && this.docs > 0) { this.forced = false; this.sync(this.docs); } });
  }

  get isShown(): boolean { return !this.root.hidden; }

  show(): void { this.forced = true; this.sync(this.docs); }

  sync(docs: number): void {
    if (docs > this.docs) this.forced = false;
    this.docs = docs;
    const on = this.forced || docs === 0;
    if (on) this.fill();
    setHidden(this.root, !on);
  }

  private fill(): void {
    const items = this.d.recent();
    setHidden(this.recentBox, items.length === 0);
    const key = JSON.stringify(items);
    if (key === this.listKey) return;
    this.listKey = key;
    this.list.replaceChildren();
    for (const it of items) {
      const row = el<HTMLButtonElement>(`<button class="cv-home-item" type="button">${icon('fileImage', 18)}<span class="cv-home-name"></span><span class="cv-home-date"></span></button>`);
      setText(row.querySelector('.cv-home-name') as HTMLElement, it.name);
      setText(row.querySelector('.cv-home-date') as HTMLElement, it.modified ? new Date(it.modified).toLocaleString() : '');
      row.title = it.path;
      row.addEventListener('click', () => this.d.openRecent(it.path));
      this.list.appendChild(row);
    }
  }
}
