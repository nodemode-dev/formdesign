import type { ConsentChoice, ConsentHost, ConsentRequest } from './adapter';
import { el, q, setClass, setHidden, setText } from './dom';
import { fill, strings, type Say } from './strings';

const CHOICES: ConsentChoice[] = ['deny', 'once', 'always'];

export class Consent {
  private root: HTMLDialogElement | null = null;
  private readonly s: Say;
  private readonly queue: ConsentRequest[] = [];
  private busy = false;
  private opening = false;
  private ours = false;
  private timer = 0;
  onChange: (open: boolean) => void = () => undefined;
  private readonly onKey = (e: KeyboardEvent): void => {
    const at = e.target as Node;
    if (this.root && (this.root.contains(at) || at === document.body || at === document.documentElement)) e.stopPropagation();
  };

  constructor(private readonly a: ConsentHost) {
    this.s = strings(a);
    a.consent?.subscribe((request) => this.add(request), (id) => this.withdraw(id));
  }

  get waiting(): number { return this.queue.length; }

  isOpen(): boolean { return this.root?.open === true; }

  private build(): HTMLDialogElement {
    if (this.root) return this.root;
    const s = this.s;
    const c = this.a.classes;
    const root = el<HTMLDialogElement>(`<dialog class="ft-consent" aria-labelledby="ft-consent-title" aria-describedby="ft-consent-what">
    <div class="ft-consent-box">
      <h2 class="ft-consent-title" id="ft-consent-title"></h2>
      <p class="ft-consent-count ${c.note}"></p>
      <p class="ft-consent-what" id="ft-consent-what"></p>
      <p class="ft-consent-notes" hidden></p>
      <p class="ft-consent-notice ${c.note}" role="status" hidden>${s('consentChanged')}</p>
      <div class="ft-consent-buttons">
        <button type="button" class="${c.button} ft-consent-btn ft-consent-deny" data-choice="deny">${s('consentDeny')}</button>
        <button type="button" class="${c.button} ft-consent-btn ft-consent-once" data-choice="once">${s('consentOnce')}</button>
        <button type="button" class="${c.button} ${c.primaryButton} ft-consent-btn ft-consent-always" data-choice="always"></button>
      </div>
    </div>
  </dialog>`);
    for (const b of root.querySelectorAll<HTMLButtonElement>('.ft-consent-btn')) b.addEventListener('click', () => void this.pick(b.dataset.choice as ConsentChoice));
    root.addEventListener('cancel', (e) => { e.preventDefault(); this.escape(); });
    root.addEventListener('close', () => this.closed());
    document.body.append(root);
    this.root = root;
    return root;
  }

  private add(request: ConsentRequest): void {
    if (this.queue.some((r) => r.id === request.id)) return;
    this.queue.push(request);
    this.watch();
    void this.render();
  }

  withdraw(id: string): void {
    const at = this.queue.findIndex((r) => r.id === id);
    if (at < 0) return;
    this.queue.splice(at, 1);
    this.watch();
    void this.render();
  }

  private watch(): void {
    window.clearTimeout(this.timer);
    const now = Date.now();
    for (const r of this.queue.filter((x) => x.expires !== undefined && x.expires <= now)) this.queue.splice(this.queue.indexOf(r), 1);
    const next = Math.min(...this.queue.map((r) => r.expires ?? Infinity));
    if (Number.isFinite(next)) this.timer = window.setTimeout(() => { this.watch(); void this.render(); }, Math.max(50, next - now));
  }

  private async render(): Promise<void> {
    const r = this.queue[0];
    if (!r) { this.shut(); return; }
    const d = this.build();
    const s = this.s;
    setText(q(d, '.ft-consent-title'), r.agent ? fill(s('consentTitle'), { agent: r.agent }) : s('consentTitleUnnamed'));
    setText(q(d, '.ft-consent-what'), fill(s('consentWhat'), { what: r.what }));
    const notes = q(d, '.ft-consent-notes');
    const lines = r.notes ?? [];
    for (let i = 0; i < Math.max(lines.length, notes.children.length); i += 1) {
      const line = (notes.children[i] as HTMLElement | undefined) ?? notes.appendChild(el('<span class="ft-consent-note"></span>'));
      setText(line, lines[i] ?? '');
      setHidden(line, i >= lines.length);
    }
    setHidden(notes, !lines.length);
    const count = q(d, '.ft-consent-count');
    setText(count, fill(s('consentCount'), { n: 1, total: this.queue.length }));
    setClass(count, 'is-one', this.queue.length < 2);
    for (const choice of CHOICES) setHidden(q(d, `.ft-consent-${choice}`), !r.offers.includes(choice));
    setText(q(d, '.ft-consent-always'), r.always === 'all' ? s('consentAlwaysAll') : r.agent ? fill(s('consentAlways'), { agent: r.agent }) : s('consentAlwaysUnnamed'));
    if (d.open || this.opening) return;
    this.opening = true;
    let changed = false;
    try { changed = (await this.a.storedChoice?.get())?.changedOutside === true; } catch { changed = false; }
    this.opening = false;
    if (!this.queue.length || d.open) return;
    setHidden(q(d, '.ft-consent-notice'), !changed);
    window.addEventListener('keydown', this.onKey, true);
    d.showModal();
    this.onChange(true);
  }

  private shut(): void {
    const d = this.root;
    if (!d?.open) return;
    this.ours = true;
    d.close();
  }

  private closed(): void {
    const ours = this.ours;
    this.ours = false;
    if (this.root?.open) return;
    window.removeEventListener('keydown', this.onKey, true);
    this.onChange(false);
    if (ours || this.busy || !this.queue.length) return;
    this.queue.shift();
    this.watch();
    void this.render();
  }

  private async pick(choice: ConsentChoice): Promise<void> {
    const r = this.queue[0];
    if (this.busy || !r || !r.offers.includes(choice)) return;
    this.busy = true;
    try { await this.a.consent?.answer(r.id, choice); } catch { this.busy = false; } finally { this.busy = false; }
    const at = this.queue.indexOf(r);
    if (at >= 0) this.queue.splice(at, 1);
    this.watch();
    void this.render();
  }

  private escape(): void {
    const r = this.queue[0];
    if (!r || this.busy) return;
    if (r.escape === 'deny' && r.offers.includes('deny')) { void this.pick('deny'); return; }
    this.queue.shift();
    this.watch();
    void this.render();
  }
}
