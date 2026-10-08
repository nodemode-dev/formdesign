import '../styles/dictionaries.css';
import { api } from '../engine/client';
import { Dialog } from '../ui/dialogs';
import { el, patchList, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Lang { language: string; code: string; installed: boolean; builtin: boolean; file?: string | null; downloadable?: boolean; removable?: boolean; source?: string | null }
interface Listing { languages: Lang[]; folder: string }

class DictionariesDialog {
  private readonly d = new Dialog('Language Dictionaries', 600);
  private readonly root = el(`<div class="fdi">
    <p class="fd-note fdi-note"></p>
    <div class="fdi-list" role="listbox" aria-label="Languages"></div>
    <p class="fdi-status" role="status"></p>
  </div>`);
  private data: Listing = { languages: [], folder: '' };
  private picked = '';
  private busy = '';
  private readonly addBtn: HTMLButtonElement;

  constructor(private readonly ws: Workspace) {
    this.d.body.append(this.root);
    this.addBtn = this.d.button('Add Hunspell Files…', 'ghost', () => void this.addFiles());
    this.d.button('Close', 'primary', () => this.d.close());
  }

  async open(): Promise<void> {
    this.d.open();
    await this.load();
  }

  private async load(): Promise<void> {
    try { this.data = await api<Listing>('/dictionaries'); } catch (err) { this.say((err as Error).message); }
    this.render();
  }

  private say(t: string): void { setText(q(this.root, '.fdi-status'), t); }

  private status(l: Lang): string {
    if (this.busy === l.code) return 'Downloading…';
    if (l.builtin) return 'Built in';
    if (l.installed && l.file && l.file !== l.code) return `Uses ${l.file}`;
    return l.installed ? 'Installed' : 'Not installed';
  }

  private render(): void {
    setText(q(this.root, '.fdi-note'), `English (USA) is checked with formdes's own word list. Other languages use Hunspell dictionaries kept in ${this.data.folder || 'the Dictionaries folder'}. Get downloads the LibreOffice project's dictionary for a language; each carries its own licence, linked beside it. Text is checked in the language set on it in the Character panel.`);
    patchList<Lang>(q(this.root, '.fdi-list'), this.data.languages, {
      key: (l) => l.code,
      create: (l) => {
        const r = el('<div class="fdi-row" role="option"><span class="fdi-name"></span><span class="fdi-code"></span><span class="fdi-st"></span><span class="fdi-acts"><a class="fdi-src" target="_blank" rel="noopener">Licence</a><button class="cv-btn frv-btn fdi-get" type="button">Get</button><button class="cv-btn frv-btn fdi-rm" type="button">Remove</button></span></div>');
        r.addEventListener('click', (e) => { if ((e.target as HTMLElement).closest('button, a')) return; this.picked = r.dataset.key as string; this.render(); });
        q(r, '.fdi-get').addEventListener('click', () => void this.get(r.dataset.key as string));
        q(r, '.fdi-rm').addEventListener('click', () => void this.remove(r.dataset.key as string));
        this.row(r, l);
        return r;
      },
      update: (r, l) => this.row(r, l),
    });
    const p = this.data.languages.find((l) => l.code === this.picked);
    setDisabled(this.addBtn, !p || p.builtin);
    this.addBtn.title = p && !p.builtin ? `Install a .aff and .dic pair for ${p.language}` : 'Pick a language first';
  }

  private row(r: HTMLElement, l: Lang): void {
    setText(q(r, '.fdi-name'), l.language);
    setText(q(r, '.fdi-code'), l.code);
    setText(q(r, '.fdi-st'), this.status(l));
    setClass(r, 'is-on', r.dataset.key === this.picked);
    setClass(r, 'is-installed', l.installed);
    const a = q<HTMLAnchorElement>(r, '.fdi-src');
    setHidden(a, !l.source);
    if (l.source && a.href !== l.source) a.href = l.source;
    setHidden(q(r, '.fdi-get'), !l.downloadable || (l.installed && l.file === l.code));
    setHidden(q(r, '.fdi-rm'), !l.removable);
    setDisabled(q<HTMLButtonElement>(r, '.fdi-get'), !!this.busy);
  }

  private async get(code: string): Promise<void> {
    this.busy = code;
    this.render();
    try {
      this.data = await api<Listing>('/dictionaries/install', { json: { code } });
      this.say(`Installed ${this.data.languages.find((l) => l.code === code)?.language ?? code}.`);
      this.ws.viewport.refreshOverlays();
    } catch (err) { this.say((err as Error).message); }
    this.busy = '';
    this.render();
  }

  private async remove(code: string): Promise<void> {
    try {
      this.data = await api<Listing>(`/dictionaries?code=${encodeURIComponent(code)}`, { method: 'DELETE' });
      this.say(`Removed the ${code} dictionary.`);
      this.ws.viewport.refreshOverlays();
    } catch (err) { this.say((err as Error).message); }
    this.render();
  }

  private async addFiles(): Promise<void> {
    const p = this.data.languages.find((l) => l.code === this.picked);
    if (!p || p.builtin) return;
    const files = await this.ws.pickFiles('.aff,.dic', true);
    const aff = files.find((f) => /\.aff$/i.test(f.name)), dic = files.find((f) => /\.dic$/i.test(f.name));
    if (!aff || !dic) { if (files.length) toast('Choose both files of the pair: the .aff and the .dic.', 'error'); return; }
    try {
      const [ap, dp] = await this.ws.upload([aff, dic]);
      this.data = await api<Listing>('/dictionaries/add', { json: { code: p.code, aff: ap, dic: dp } });
      this.say(`Installed ${aff.name} and ${dic.name} for ${p.language}.`);
      this.ws.viewport.refreshOverlays();
    } catch (err) { this.say((err as Error).message); }
    this.render();
  }
}

export function installDictionaries(ws: Workspace): void {
  let dlg: DictionariesDialog | null = null;
  ws.addCommands({ id: 'ui.dictionaries', label: 'Language Dictionaries…', run: () => void (dlg ??= new DictionariesDialog(ws)).open() });
  ws.addMenuItems({ menu: 'Edit', items: ['ui.dictionaries'], at: 'end' });
}
