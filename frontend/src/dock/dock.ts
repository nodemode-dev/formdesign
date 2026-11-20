import type { DockAdapter, DockAgentStatus, DockAppTab, DockChannel, DockKind, DockOwnAsk, DockPaneHidden, DockTerminal, PtyKind } from './adapter';
import { ChatPane, OwnChat, type DockChatView } from './chat';
import { Consent } from './consent';
import { el, q, setClass, setDisabled, setHidden, setText } from './dom';
import { dockIcon as icon } from './icons';
import { chordLabel, contain, isChord, keeps, leaves } from './keys';
import { placementFor, type Placement } from './placement';
import type { DockSide } from './state';
import type { DockTerm } from './terminal';
import { strings, type Say } from './strings';
import { Tour, type TourStep } from './tour';

export type { DockAdapter, DockKind } from './adapter';

const PROGRAMS = ['codex', 'cursor'] as const;
type Program = typeof PROGRAMS[number];

interface Tab {
  id: string;
  term: DockTerm | null;
  host: HTMLElement;
  tab: HTMLElement;
  channel: DockChannel | null;
  kind: PtyKind;
  seen: boolean;
  closed: boolean;
}

export class Dock {
  private readonly s: Say;
  readonly root: HTMLElement;
  private readonly tabsEl: HTMLElement;
  private readonly stage: HTMLElement;
  private readonly empty: HTMLElement;
  private readonly note: HTMLElement;
  private readonly noteText: HTMLElement;
  private readonly noteCmd: HTMLElement;
  private readonly noteCopy: HTMLButtonElement;
  private readonly ask: HTMLInputElement;
  private readonly restartBtn: HTMLButtonElement;
  private readonly tabs = new Map<string, Tab>();
  private readonly owned = new Map<string, { spec: DockAppTab; tab: HTMLElement; host: HTMLElement; mounted: boolean; off: (() => void) | undefined }>();
  private chat: DockChatView | null = null;
  private readonly ownAsk: DockOwnAsk | null;
  private chatTab: HTMLElement | null = null;
  private active: string | null = null;
  private started = false;
  private pending: Promise<void> | null = null;
  private emulator: Promise<typeof import('./terminal')> | null = null;
  private asked: Promise<DockAgentStatus | null> | null = null;
  private mounted = false;
  private revealing = false;
  private readonly place: Placement;
  onTab: () => void = () => undefined;
  onChange: () => void = () => undefined;
  onResize: () => void = () => undefined;
  readonly consent: Consent | null;
  private readonly tour: Tour;

  constructor(private readonly a: DockAdapter, parts: { consent?: Consent } = {}) {
    const s = strings(a);
    const c = a.classes;
    const ib = `${c.iconButton} ft-dock-ibtn`;
    this.s = s;
    this.root = el(`<section class="ft-dock" aria-label="${s('terminal')}">
    <div class="ft-dock-grip" role="separator" aria-orientation="horizontal" aria-label="${s('resize')}"></div>
    <div class="ft-dock-head">
      <div class="ft-dock-tabs" role="tablist" aria-label="${s('terminals')}"></div>
      <span class="ft-dock-spacer"></span>
      <button type="button" class="${ib} ft-dock-codex" title="${s('codex')}" aria-label="${s('codex')}" hidden>${icon('codex', 15)}</button>
      <button type="button" class="${ib} ft-dock-cursor" title="${s('cursor')}" aria-label="${s('cursor')}" hidden>${icon('cursor', 15)}</button>
      <button type="button" class="${ib} ft-dock-new" title="${s('newTerminal')}" aria-label="${s('newTerminal')}">${icon('plus', 15)}</button>
      <button type="button" class="${ib} ft-dock-chat" title="${s('chatAbout')}" aria-label="${s('chatWith')}">${icon('agentChat', 15)}</button>
      <button type="button" class="${ib} ft-dock-claude" title="${s('claudeCode')}" aria-label="${s('claudeCode')}">${icon('agent', 15)}</button>
      <button type="button" class="${ib} ft-dock-more" title="${s('moreAgents')}" aria-label="${s('moreAgents')}" aria-haspopup="true" aria-expanded="false">${icon('menu', 15)}</button>
      <button type="button" class="${ib} ft-dock-restart" title="${s('restart')}" aria-label="${s('restart')}">${icon('refresh', 15)}</button>
      <button type="button" class="${ib} ft-dock-side" title="${s('toRight')}" aria-label="${s('toRight')}"><span class="ft-dock-side-right">${icon('dockRight', 15)}</span><span class="ft-dock-side-bottom">${icon('dockBottom', 15)}</span></button>
      <button type="button" class="${ib} ft-dock-hide" title="${s('hideTitle', { chord: chordLabel(a) })}" aria-label="${s('hide')}">${icon('close', 15)}</button>
    </div>
    <div class="ft-dock-menu" role="menu" hidden>
      <div class="ft-dock-menu-row" data-kind="codex"><button type="button" class="${c.button} ft-dock-menu-go" role="menuitem">${icon('codex', 13)}<span>${s('codex')}</span></button><p class="ft-dock-menu-how ${c.note}"></p><code class="ft-dock-menu-cmd"></code></div>
      <div class="ft-dock-menu-row" data-kind="cursor"><button type="button" class="${c.button} ft-dock-menu-go" role="menuitem">${icon('cursor', 13)}<span>${s('cursor')}</span></button><p class="ft-dock-menu-how ${c.note}"></p><code class="ft-dock-menu-cmd"></code></div>
      <div class="ft-dock-menu-row ft-dock-menu-links"><button type="button" class="${c.button} ft-dock-menu-settings" role="menuitem">${s('outsideAgents')}</button><button type="button" class="${c.button} ft-dock-menu-tour" role="menuitem">${s('showAround')}</button></div>
    </div>
    <div class="ft-dock-stage"><p class="ft-dock-empty ${c.note}" hidden>${s('empty')}</p>
      <div class="ft-dock-note" role="status" hidden><span class="ft-dock-note-text"></span><code class="ft-dock-note-cmd" hidden></code><button type="button" class="${c.button} ft-dock-note-copy" hidden>${s('copy')}</button><button type="button" class="${ib} ft-dock-note-x" title="${s('dismiss')}" aria-label="${s('dismiss')}">${icon('close', 12)}</button></div></div>
    <form class="ft-dock-ask" autocomplete="off">
      <span class="ft-dock-ask-mark">${icon('agent', 15)}</span>
      <input class="${c.input} ft-dock-ask-input" type="text" spellcheck="false" placeholder="${s('askPlaceholder')}" aria-label="${s('askLabel')}">
      <button type="submit" class="${ib} ft-dock-ask-send" title="${s('send')}" aria-label="${s('send')}">${icon('send', 14)}</button>
    </form>
  </section>`);
    this.tabsEl = q(this.root, '.ft-dock-tabs');
    this.stage = q(this.root, '.ft-dock-stage');
    this.empty = q(this.root, '.ft-dock-empty');
    this.note = q(this.root, '.ft-dock-note');
    this.noteText = q(this.root, '.ft-dock-note-text');
    this.noteCmd = q(this.root, '.ft-dock-note-cmd');
    this.noteCopy = q<HTMLButtonElement>(this.root, '.ft-dock-note-copy');
    this.ask = q<HTMLInputElement>(this.root, '.ft-dock-ask-input');
    this.restartBtn = q<HTMLButtonElement>(this.root, '.ft-dock-restart');
    this.place = placementFor(a);
    q(this.root, '.ft-dock-new').addEventListener('click', () => void this.open());
    q(this.root, '.ft-dock-claude').addEventListener('click', () => void this.claude());
    q(this.root, '.ft-dock-chat').addEventListener('click', () => void this.showChat());
    q(this.root, '.ft-dock-hide').addEventListener('click', () => this.toggle(false));
    q(this.root, '.ft-dock-side').addEventListener('click', () => { this.setSide(this.side() === 'right' ? 'bottom' : 'right'); this.focus(); });
    for (const kind of PROGRAMS) {
      q(this.root, `.ft-dock-${kind}`).addEventListener('click', () => void this.program(kind));
      q(this.root, `.ft-dock-menu-row[data-kind="${kind}"] .ft-dock-menu-go`).addEventListener('click', () => { this.menu(false); void this.program(kind); });
    }
    q(this.root, '.ft-dock-more').addEventListener('click', () => { this.menu(q(this.root, '.ft-dock-menu').hidden === true); });
    q(this.root, '.ft-dock-menu-settings').addEventListener('click', () => { this.menu(false); a.openSettings?.(); });
    setHidden(q(this.root, '.ft-dock-menu-settings'), !a.openSettings);
    q(this.root, '.ft-dock-menu-tour').addEventListener('click', () => { this.menu(false); void this.guide(); });
    this.consent = parts.consent ?? (a.consent ? new Consent(a) : null);
    this.tour = new Tour(a);
    this.restartBtn.addEventListener('click', () => void this.restart());
    q(this.root, '.ft-dock-ask').addEventListener('submit', (e) => { e.preventDefault(); void this.send(); });
    q(this.root, '.ft-dock-note-x').addEventListener('click', () => this.dismiss());
    this.noteCopy.addEventListener('click', () => { void navigator.clipboard?.writeText(this.noteCmd.textContent ?? '').catch(() => undefined); });
    contain(this.root, a);
    setClass(this.root, 'has-own-chat', !!a.chat && 'pane' in a.chat);
    const ask = a.chat && 'pane' in a.chat ? a.chat.ask : undefined;
    this.ownAsk = !ask || ask === 'none' ? null : ask === 'line' ? {} : ask;
    if (this.ownAsk?.element) {
      const slot = q(this.root, '.ft-dock-ask');
      const box = el('<div class="ft-dock-ask-own"></div>');
      box.appendChild(this.ownAsk.element);
      slot.classList.add('has-element');
      slot.appendChild(box);
    }
  }

  mount(host: HTMLElement, before: Node | null = null): void {
    if (this.mounted) return;
    this.mounted = true;
    const a = this.a;
    host.insertBefore(this.root, before);
    this.place.start(this.root, q(this.root, '.ft-dock-grip'), {
      changed: () => { this.labelSide(); this.onChange(); this.fitActive(); },
      resizing: () => { this.onResize(); },
      settled: () => { this.keep(); window.dispatchEvent(new Event('resize')); this.focus(); },
    });
    this.labelSide();
    this.refresh();
    document.addEventListener('pointerdown', (e) => { if (!(e.target as Element).closest?.('.ft-dock-menu, .ft-dock-more')) this.menu(false); }, true);
    new ResizeObserver(() => this.fitActive()).observe(this.stage);
    document.addEventListener('keydown', (e) => {
      if (!isChord(a, e)) return;
      e.preventDefault();
      e.stopPropagation();
      if (!e.repeat) this.toggle();
    }, true);
    a.onTheme(() => this.retheme());
    new MutationObserver(() => this.retheme()).observe(document.documentElement, { attributes: true, attributeFilter: ['style', 'class', 'data-theme'] });
    if (this.isOpen()) void this.ensure();
  }

  isOpen(): boolean { return this.place.isOpen(); }

  keeps(event: Event): boolean { return keeps(this.a, event); }

  shown(): void {
    this.place.shown(true);
    this.tell('dock');
    void this.ensure();
    this.onTab();
  }

  hidden(): void {
    this.place.shown(false);
    this.tell('dock');
    this.tour.end(false);
    this.onTab();
  }

  private tell(why: DockPaneHidden): void {
    this.chat?.visible?.(this.isOpen() && this.active === 'chat' && this.chatTab !== null, why);
  }

  toggle(on = !this.isOpen(), seed: DockKind | 'none' = 'shell'): void {
    const p = this.a.placement;
    if (p.mode === 'hosted') { p.request(on); return; }
    this.place.setOpen(on);
    this.tell('dock');
    this.keep();
    this.onChange();
    window.dispatchEvent(new Event('resize'));
    if (on && !this.place.seen() && !this.tour.isRunning() && this.a.firstRun?.() !== false) void this.guide();
    if (!on) this.tour.end(false);
    if (on) void this.ensure(seed);
    this.onTab();
  }

  async newTerminal(): Promise<void> {
    if (!this.isOpen()) this.toggle(true);
    else await this.open();
  }

  activeKind(): DockKind | null {
    if (this.active === 'chat') return 'chat';
    const kind = this.active ? this.tabs.get(this.active)?.kind : undefined;
    return kind && kind !== 'agent' ? kind : null;
  }

  isShowing(kind: DockKind): boolean { return this.isOpen() && this.activeKind() === kind; }

  async show(kind: DockKind): Promise<void> {
    if (!this.isOpen()) this.toggle(true, 'none');
    await this.ensure('none');
    if (kind === 'chat') { await this.showChat(); return; }
    const own = [...this.tabs.values()].find((x) => x.kind === kind);
    if (own) this.select(own.id);
    else if (kind === 'claude') await this.claude();
    else if (kind === 'codex' || kind === 'cursor') await this.program(kind);
    else await this.open();
  }

  private async programs(): Promise<void> {
    const status = await this.agents();
    for (const kind of PROGRAMS) {
      const st = status?.[kind];
      const offered = this.offers(kind);
      const found = offered && !!st?.installed;
      const row = q(this.root, `.ft-dock-menu-row[data-kind="${kind}"]`);
      setHidden(row, !offered || !st);
      setHidden(q(this.root, `.ft-dock-${kind}`), !found);
      setHidden(q(row, '.ft-dock-menu-go'), !found);
      setText(q(row, '.ft-dock-menu-how'), this.s(found ? 'programReady' : 'programMissing', { program: this.s(kind) }));
      setText(q(row, '.ft-dock-menu-cmd'), found ? '' : st?.install ?? '');
      setHidden(q(row, '.ft-dock-menu-cmd'), found || !st?.install);
    }
  }

  private offers(kind: DockKind): boolean {
    return this.a.tabs().builtin.includes(kind) && (kind !== 'chat' || !!this.a.chat);
  }

  activeTab(): string | null { return this.active; }

  refresh(): void {
    setHidden(q(this.root, '.ft-dock-new'), !this.offers('shell'));
    setHidden(q(this.root, '.ft-dock-chat'), !this.offers('chat'));
    setHidden(q(this.root, '.ft-dock-claude'), !this.offers('claude'));
    setHidden(this.restartBtn, !this.a.terminal.restart);
    setClass(this.root, 'has-no-ask', !this.offers('claude') && !this.offers('chat'));
    setClass(this.root, 'has-ask-line', this.ownAsk !== null && (this.offers('claude') || this.offers('chat')));
    const want = new Map((this.a.tabs().own ?? []).map((spec) => [spec.id, spec]));
    for (const [id, o] of [...this.owned]) {
      if (want.has(id)) continue;
      o.off?.();
      o.tab.remove();
      o.host.remove();
      this.owned.delete(id);
      if (this.active !== `own:${id}`) continue;
      this.active = null;
      const next = [...this.tabs.keys()].pop() ?? (this.chatTab ? 'chat' : undefined);
      if (next) this.select(next); else { setHidden(this.empty, false); this.onTab(); }
    }
    for (const spec of want.values()) {
      if (this.owned.has(spec.id)) continue;
      const tab = el('<button type="button" class="ft-dock-tab is-app" role="tab" aria-selected="false"><span class="ft-icon ft-dock-tab-icon" aria-hidden="true"></span><span class="ft-dock-tab-title"></span></button>');
      q(tab, '.ft-dock-tab-icon').innerHTML = spec.icon;
      setText(q(tab, '.ft-dock-tab-title'), spec.label);
      const host = el('<div class="ft-dock-pane" hidden></div>');
      tab.addEventListener('click', () => this.select(`own:${spec.id}`));
      this.tabsEl.appendChild(tab);
      this.stage.appendChild(host);
      this.owned.set(spec.id, { spec, tab, host, mounted: false, off: undefined });
      setHidden(this.empty, true);
    }
    if (this.started) void this.programs();
  }

  private menu(open: boolean): void {
    const m = q(this.root, '.ft-dock-menu');
    if (m.hidden === !open) return;
    if (open) void this.programs();
    setHidden(m, !open);
    q(this.root, '.ft-dock-more').setAttribute('aria-expanded', String(open));
  }

  async program(kind: Program): Promise<boolean> {
    const size = this.size();
    try {
      const info = await this.a.terminal.open(kind, { cols: size.cols, rows: size.rows });
      if (!this.isOpen()) this.toggle(true, 'none');
      await this.attach(info, true);
      this.dismiss();
      return true;
    } catch (err) {
      const kind = this.a.errors.kind?.(err);
      if (kind === 'oldEngine' || kind === 'notInstalled') { void this.programs(); this.menu(true); }
      else this.warn(err);
      return false;
    }
  }

  async toggleKind(kind: DockKind): Promise<void> {
    if (this.isShowing(kind)) this.toggle(false);
    else await this.show(kind);
  }

  async sync(): Promise<void> {
    if (!this.started) return;
    for (const info of await this.list()) if (info.kind === 'agent') this.adopt(info);
  }

  private keep(): void { this.place.keep(this.activeKind()); }

  async guide(): Promise<void> {
    if (!this.isOpen()) {
      const first = !this.place.seen();
      this.toggle(true, 'none');
      if (first) return;
    }
    await this.showChat();
    const claude = (await this.agents())?.claude ?? null;
    const middle: TourStep = !claude?.installed ? { target: '.ft-dock-claude', title: 'tourMissingTitle', body: 'tourMissingBody', cmd: claude?.install || 'npm install -g @anthropic-ai/claude-code' }
      : claude.signedIn === false ? { target: '.ft-dock-claude', title: 'tourSignInTitle', body: 'tourSignInBody' }
        : claude.signedIn === true ? { target: '.ft-dock-claude', title: 'tourReadyTitle', body: 'tourReadyBody' }
          : { target: '.ft-dock-claude', title: 'tourInstalledTitle', body: 'tourInstalledBody' };
    const steps: TourStep[] = [
      { target: '.ft-dock', title: 'tourPanelTitle', body: 'tourPanelBody' },
      { target: this.ownAsk?.element ? '.ft-dock-ask' : this.ownAsk ? '.ft-dock-ask-input' : this.chat?.askTarget?.() ?? '.ft-dock-ask-input', title: 'tourChatTitle', body: 'tourChatBody' },
      middle,
      ...(this.consent ? [{ target: '.ft-dock-more', title: 'tourAskTitle', body: 'tourAskBody' } as TourStep] : []),
      { target: '.ft-dock-side', title: 'tourSideTitle', body: 'tourSideBody' },
    ];
    this.tour.start(steps, () => this.place.markSeen());
  }

  side(): DockSide { return this.place.side(); }

  storedSide(): DockSide { return this.place.storedSide(); }

  setSide(side: DockSide): void {
    if (side === this.place.storedSide() && side === this.side()) return;
    this.place.setSide(side);
    this.labelSide();
    this.keep();
    this.onChange();
    window.dispatchEvent(new Event('resize'));
  }

  private labelSide(): void {
    const right = this.side() === 'right';
    const label = right ? this.s('toBottom') : this.s('toRight');
    const btn = q(this.root, '.ft-dock-side');
    if (btn.title !== label) { btn.title = label; btn.setAttribute('aria-label', label); }
    q(this.root, '.ft-dock-grip').setAttribute('aria-orientation', right ? 'vertical' : 'horizontal');
    setHidden(btn, !this.place.canRight());
  }

  private retheme(): void {
    for (const x of this.tabs.values()) x.term?.retheme();
    this.fitActive();
  }

  private load(): Promise<typeof import('./terminal')> {
    this.emulator ??= import('./terminal');
    return this.emulator;
  }

  private agents(): Promise<DockAgentStatus | null> {
    if (!this.asked) {
      this.asked = this.a.agentStatus().catch(() => null);
      void this.asked.then(() => window.setTimeout(() => { this.asked = null; }, 1500));
    }
    return this.asked;
  }

  private async list(): Promise<DockTerminal[]> {
    try { return await this.a.terminal.list(); } catch { return []; }
  }

  private ensure(seed: DockKind | 'none' = 'shell'): Promise<void> {
    this.pending ??= this.boot(seed).finally(() => { this.pending = null; });
    return this.pending;
  }

  private async boot(seed: DockKind | 'none'): Promise<void> {
    if (!this.started) {
      this.started = true;
      const listed = await this.list();
      for (const info of listed) this.adopt(info);
    }
    const shown = !!this.chatTab || [...this.tabs.values()].some((x) => x.kind !== 'agent');
    if (!shown && seed === 'shell') {
      const first = this.owned.keys().next();
      if (this.offers('chat') && (this.place.savedTab() === 'chat' || !this.offers('shell'))) await this.showChat();
      else if (this.offers('shell')) await this.open();
      else if (!first.done) this.select(`own:${first.value}`);
    }
    setHidden(this.empty, !!this.chatTab || this.tabs.size > 0);
    void this.programs();
    this.fitActive();
    this.focus();
    this.awaitReveal();
    this.onTab();
  }

  private awaitReveal(): void {
    const root = this.root;
    const hiddenNow = (): boolean => root.getClientRects().length === 0 || getComputedStyle(root).visibility === 'hidden';
    const lost = document.activeElement;
    if (this.revealing || !this.isOpen() || root.contains(lost) || !hiddenNow()) return;
    this.revealing = true;
    const watch = new MutationObserver(() => {
      const moved = document.activeElement !== lost || !this.isOpen();
      if (!moved && hiddenNow()) return;
      watch.disconnect();
      this.revealing = false;
      if (!moved) this.focus();
    });
    for (let node: Node | null = root.parentNode; node instanceof Element; node = node.parentNode) watch.observe(node, { attributes: true, attributeFilter: ['class', 'style', 'hidden'] });
  }

  focus(): void {
    if (this.active === 'chat' && this.ownAsk && !this.ownAsk.element) this.ask.focus({ preventScroll: true });
    else if (this.active === 'chat' && this.chat?.focus) this.chat.focus();
    else if (this.active === 'chat') this.ask.focus({ preventScroll: true });
    else if (this.active) this.tabs.get(this.active)?.term?.focus();
  }

  async sendChat(text: string): Promise<boolean> {
    await this.show('chat');
    return this.chat ? this.chat.send(text) : false;
  }

  async showChat(): Promise<void> {
    const c = this.a.chat;
    if (!c) return;
    if (!this.chat) {
      if ('pane' in c) this.chat = new OwnChat(c);
      else {
        const chat = new ChatPane({ ...this.a, chat: c });
        chat.onFail = (err) => this.warn(err);
        chat.onHandoff = (session) => void this.claude(undefined, session);
        chat.onBash = () => void this.sync();
        this.chat = chat;
      }
      this.stage.appendChild(this.chat.root);
    }
    if (!this.chatTab) {
      const tab = el(`<button type="button" class="ft-dock-tab is-chat" role="tab" aria-selected="false">${icon('agentChat', 13)}<span class="ft-dock-tab-title">${this.s('chat')}</span><span class="ft-dock-tab-x" role="button" aria-label="${this.s('closeChat')}">${icon('close', 11)}</span></button>`);
      tab.addEventListener('click', (e) => {
        if ((e.target as HTMLElement).closest('.ft-dock-tab-x')) this.closeChat();
        else this.select('chat');
      });
      this.tabsEl.prepend(tab);
      this.chatTab = tab;
    }
    setHidden(this.empty, true);
    this.select('chat');
    await this.chat.load();
  }

  private closeChat(): void {
    this.chatTab?.remove();
    this.chatTab = null;
    this.tell('tab');
    this.chat?.release();
    if (this.chat) setHidden(this.chat.root, true);
    if (this.active === 'chat') {
      this.active = null;
      const next = [...this.tabs.keys()].pop();
      if (next) this.select(next);
      else { setHidden(this.empty, false); this.syncAsk(); setDisabled(this.restartBtn, true); this.onTab(); }
    }
  }

  private syncAsk(): void {
    const chat = this.active === 'chat';
    const text = chat ? this.s('chatPlaceholder') : this.s('askPlaceholder');
    if (this.ask.placeholder !== text) this.ask.placeholder = text;
  }

  private async send(): Promise<void> {
    const text = this.ask.value.trim();
    if (!text) return;
    const chat = this.chat;
    if (this.ownAsk && (this.ownAsk.send || (this.active === 'chat' && chat))) {
      const route = this.ownAsk.send;
      if (!route && chat?.isBusy()) return;
      this.ask.value = '';
      const went = route ? (await route(text, this.activeKind())) !== false : chat ? await chat.send(text) : false;
      if (!went && !this.ask.value) this.ask.value = text;
      return;
    }
    if (this.active === 'chat' && this.chat) {
      if (this.chat.isBusy()) return;
      this.ask.value = '';
      await this.chat.send(text);
      return;
    }
    this.ask.value = '';
    const ok = await this.claude(text);
    if (!ok && !this.ask.value) this.ask.value = text;
  }

  private say(text: string, cmd = ''): void {
    setText(this.noteText, text);
    setText(this.noteCmd, cmd);
    setHidden(this.noteCmd, cmd === '');
    setHidden(this.noteCopy, cmd === '');
    setHidden(this.note, false);
  }

  private dismiss(): void { setHidden(this.note, true); }

  private warn(err: unknown): void {
    const kind = this.a.errors.kind?.(err);
    const said = this.a.errors.message(err) ?? (err instanceof Error ? err.message : '');
    if (kind === 'unreachable') this.say(this.a.words.start ? this.s('offlineStart') : this.s('offline'));
    else if (kind === 'refused') this.say(said || this.s('refused'));
    else this.say(said || this.s('failed'));
  }

  private async fail(err: unknown, what: 'shell' | 'claude'): Promise<void> {
    const kind = this.a.errors.kind?.(err);
    if (what === 'shell' || kind === 'refused' || kind === 'unreachable') { this.warn(err); return; }
    const status = (await this.agents())?.claude;
    if (status && !status.installed) {
      this.say(this.s('notInstalled'), status.install);
      return;
    }
    if (kind === 'oldEngine') this.say(this.s('oldEngine'));
    else this.say(`${this.s('notStarted')} ${err instanceof Error ? err.message : ''}`.trim());
  }

  private ended(x: Tab, on: boolean): void {
    setClass(x.tab, 'is-ended', on);
    if (x.kind === 'agent') return;
    const tip = on ? this.s('endedTab') : x.tab.dataset.tip ?? '';
    if (x.tab.title !== tip) x.tab.title = tip;
  }

  private adopt(info: DockTerminal): void {
    if (this.tabs.has(info.id)) return;
    void this.attach(info, this.active === null || info.kind !== 'agent');
  }

  private async open(): Promise<void> {
    try {
      const info = await this.a.terminal.open('shell', { cols: 100, rows: 24 });
      await this.attach(info, true);
      this.dismiss();
    } catch (err) { await this.fail(err, 'shell'); }
  }

  async claude(prompt?: string, resume?: string): Promise<boolean> {
    const size = this.size();
    try {
      const info = await this.a.terminal.open('claude', { cols: size.cols, rows: size.rows, ...(prompt ? { prompt } : {}), ...(resume ? { resume } : {}) });
      if (!this.isOpen()) this.toggle(true, 'none');
      if (this.tabs.has(info.id)) this.select(info.id);
      else await this.attach(info, true);
      this.dismiss();
      return true;
    } catch (err) {
      await this.fail(err, 'claude');
      return false;
    }
  }

  private size(): { cols: number; rows: number } {
    const tab = this.active ? this.tabs.get(this.active) : null;
    return tab?.term ? { cols: tab.term.cols(), rows: tab.term.rows() } : { cols: 100, rows: 20 };
  }

  private async attach(info: DockTerminal, show: boolean): Promise<void> {
    if (this.tabs.has(info.id)) { if (show) this.select(info.id); return; }
    const kind: PtyKind = ['claude', 'agent', 'codex', 'cursor'].includes(info.kind) ? info.kind : 'shell';
    const agent = kind === 'agent';
    const mark = kind === 'claude' ? 'agent' : kind === 'codex' || kind === 'cursor' ? kind : 'terminal';
    const host = el('<div class="ft-term" translate="no" dir="ltr" hidden></div>');
    const tab = el(`<button type="button" class="ft-dock-tab${kind === 'shell' || agent ? '' : ` is-${kind}`}${agent ? ' is-agent' : ''}" role="tab" aria-selected="false">${icon(mark, 13)}<span class="ft-dock-tab-title"></span><span class="ft-dock-tab-x" role="button" aria-label="${this.s('closeTab')}">${icon('close', 11)}</span></button>`);
    setText(q(tab, '.ft-dock-tab-title'), info.title);
    if (agent) tab.title = this.s('agentTab');
    tab.addEventListener('click', (e) => {
      if ((e.target as HTMLElement).closest('.ft-dock-tab-x')) void this.close(info.id);
      else this.select(info.id);
    });
    this.stage.appendChild(host);
    this.tabsEl.appendChild(tab);
    if (info.tip) tab.dataset.tip = info.tip;
    const entry: Tab = { id: info.id, term: null, host, tab, channel: null, kind, seen: false, closed: false };
    this.tabs.set(info.id, entry);
    setHidden(this.empty, true);
    this.ended(entry, !info.alive);
    if (show) this.select(info.id, false);
    const { createTerm } = await this.load();
    if (entry.closed) return;
    const term = createTerm({ agent, scrollback: this.a.terminal.scrollback, leaves: (e) => leaves(this.a, e) });
    entry.term = term;
    term.open(host);
    term.onInput((data) => entry.channel?.send(data));
    if (this.active === info.id) { this.fitActive(); if (show && !agent) term.focus(); }
    await this.connect(entry);
  }

  private async connect(entry: Tab): Promise<void> {
    const silent = (): void => { if (!entry.seen && !entry.closed) this.say(this.s('noAnswer')); };
    let channel: DockChannel;
    try { channel = await this.a.terminal.channel(entry.id); } catch { silent(); return; }
    if (entry.closed) { channel.close(); return; }
    entry.channel = channel;
    channel.onData((data) => { entry.seen = true; entry.term?.write(data); });
    channel.onExit(() => { entry.seen = true; this.ended(entry, true); });
    channel.onClose?.(() => { if (entry.channel === channel) entry.channel = null; silent(); });
    this.sendSize(entry);
  }

  private sendSize(x: Tab): void {
    if (x.term) x.channel?.resize(x.term.cols(), x.term.rows());
  }

  private fitActive(): void {
    const x = this.active ? this.tabs.get(this.active) : null;
    if (!x?.term || x.host.hidden || this.stage.clientHeight < 20) return;
    if (x.term.fit()) this.sendSize(x);
  }

  private select(id: string, focus = true): void {
    this.active = id;
    for (const [tid, x] of this.tabs) {
      const on = tid === id;
      setHidden(x.host, !on);
      x.tab.classList.toggle('is-on', on);
      x.tab.setAttribute('aria-selected', String(on));
    }
    if (this.chat) setHidden(this.chat.root, id !== 'chat');
    if (this.chatTab) {
      this.chatTab.classList.toggle('is-on', id === 'chat');
      this.chatTab.setAttribute('aria-selected', String(id === 'chat'));
    }
    for (const [oid, o] of this.owned) {
      const on = `own:${oid}` === id;
      if (on && !o.mounted) { o.mounted = true; const off = o.spec.mount(o.host); o.off = typeof off === 'function' ? off : undefined; }
      setHidden(o.host, !on);
      setClass(o.tab, 'is-on', on);
      o.tab.setAttribute('aria-selected', String(on));
    }
    this.syncAsk();
    setDisabled(this.restartBtn, !this.tabs.has(id) || this.tabs.get(id)?.kind === 'agent');
    this.fitActive();
    if (focus) this.focus();
    this.keep();
    this.tell('tab');
    this.onTab();
  }

  private async restart(): Promise<void> {
    const x = this.active ? this.tabs.get(this.active) : null;
    if (!x || x.kind === 'agent' || !this.a.terminal.restart) return;
    try { await this.a.terminal.restart(x.id); } catch (err) { this.warn(err); return; }
    this.ended(x, false);
    if (!x.channel) { x.term?.reset(); x.seen = false; await this.connect(x); }
    this.sendSize(x);
    x.term?.focus();
  }

  private async close(id: string): Promise<void> {
    const x = this.tabs.get(id);
    if (!x) return;
    x.closed = true;
    x.channel?.close();
    x.term?.dispose();
    x.host.remove();
    x.tab.remove();
    this.tabs.delete(id);
    await this.a.terminal.close(id).catch(() => undefined);
    if (this.active === id) {
      this.active = null;
      const next = [...this.tabs.keys()].pop() ?? (this.chatTab ? 'chat' : undefined);
      if (next) this.select(next);
      else { setDisabled(this.restartBtn, true); this.syncAsk(); this.onTab(); }
    }
    setHidden(this.empty, this.tabs.size > 0 || !!this.chatTab);
  }
}
