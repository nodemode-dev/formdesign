import { el, q, setClass, setDisabled, setText } from './dom';
import { icon } from './icons';
import type { Commands } from './commands';

/** A command id, a separator, or a submenu of further items. */
export type MenuItem = string | '-' | SubMenu;
export interface SubMenu { label: string; items: MenuItem[] }
const isSub = (i: MenuItem): i is SubMenu => typeof i === 'object';

export class PopupMenu {
  readonly panel: HTMLElement;
  private rows: Array<{ id: string; node: HTMLButtonElement }> = [];
  private subs: Array<{ sub: SubMenu; node: HTMLButtonElement; menu: PopupMenu }> = [];
  private seps: HTMLElement[] = [];
  private key = '';
  private openSub: PopupMenu | null = null;
  private subTop = 0;
  onClose: (() => void) | null = null;

  constructor(private readonly commands: Commands, private readonly parent: PopupMenu | null = null, standalone = false) {
    this.panel = el('<div class="cv-menu" role="menu" hidden></div>');
    document.getElementById('overlay-root')?.appendChild(this.panel);
    this.panel.addEventListener('contextmenu', (e) => e.preventDefault());
    // A scrolled menu moves its rows, so a submenu opened beside one would no longer line up.
    this.panel.addEventListener('scroll', () => { if (this.panel.scrollTop !== this.subTop) this.closeSub(); }, { passive: true });
    // A menu opened on its own (the layers' right-click menu) closes on a press anywhere outside it; the menubar's
    // menus are closed by the menubar, which also owns their buttons.
    if (standalone) document.addEventListener('pointerdown', (e) => { if (this.isOpen && !this.contains(e.target as Node)) this.close(); }, true);
  }

  get isOpen(): boolean { return !this.panel.hidden; }

  /** Whether `t` is inside this menu or any submenu open from it. */
  contains(t: Node): boolean { return this.panel.contains(t) || !!this.openSub?.contains(t); }

  setItems(items: MenuItem[]): void {
    const key = JSON.stringify(items);
    if (key === this.key) return;
    this.key = key;
    this.closeSub();
    for (const s of this.subs) s.menu.panel.remove();
    this.panel.textContent = '';
    this.rows = [];
    this.subs = [];
    this.seps = [];
    // A tick column only where a tick can show, as the target's menus (its Filter menu has none; Window's panels do).
    this.panel.classList.toggle('has-checks', items.some((i) => typeof i === 'string' && i !== '-' && !!this.commands.get(i)?.checked));
    for (const item of items) {
      if (item === '-') { const s = el('<div class="cv-menu-sep"></div>'); this.seps.push(s); this.panel.appendChild(s); continue; }
      if (isSub(item)) {
        const node = el<HTMLButtonElement>(`<button class="cv-menu-item cv-menu-sub" role="menuitem" aria-haspopup="menu" type="button"><span class="cv-menu-check"></span><span class="cv-menu-label"></span><span class="cv-menu-keys">${icon('chevronRight', 12)}</span></button>`);
        setText(q(node, '.cv-menu-label'), item.label);
        const menu = new PopupMenu(this.commands, this);
        menu.setItems(item.items);
        node.addEventListener('pointerenter', () => this.showSub(menu, node));
        node.addEventListener('click', () => this.showSub(menu, node));
        this.subs.push({ sub: item, node, menu });
        this.panel.appendChild(node);
        continue;
      }
      const cmd = this.commands.get(item);
      if (!cmd) continue;
      const node = el<HTMLButtonElement>(`<button class="cv-menu-item" role="menuitem" type="button"><span class="cv-menu-check">${icon('check', 14)}</span><span class="cv-menu-label"></span><span class="cv-menu-keys"></span></button>`);
      setText(q(node, '.cv-menu-keys'), this.commands.keyLabel(cmd));
      node.addEventListener('pointerenter', () => this.closeSub());
      node.addEventListener('click', () => { this.root().close(); this.commands.run(item); });
      this.rows.push({ id: item, node });
      this.panel.appendChild(node);
    }
  }

  private root(): PopupMenu { let m: PopupMenu = this; while (m.parent) m = m.parent; return m; }

  private showSub(menu: PopupMenu, row: HTMLElement): void {
    if (this.openSub === menu && menu.isOpen) return;
    this.closeSub();
    this.openSub = menu;
    this.subTop = this.panel.scrollTop;
    setClass(row, 'is-open', true);
    menu.onClose = () => setClass(row, 'is-open', false);
    const r = row.getBoundingClientRect();
    menu.openBeside(r);
  }

  private closeSub(): void {
    const s = this.openSub;
    this.openSub = null;
    s?.close();
  }

  /** Refreshes labels, checks and enabled states, then shows the menu at `x, y`, kept within the window; a menu taller
   *  than the window scrolls. */
  openAt(x: number, y: number): void {
    this.refresh();
    this.panel.style.visibility = 'hidden';
    this.panel.hidden = false;
    this.panel.scrollTop = 0;
    const w = this.panel.offsetWidth, h = this.panel.offsetHeight;
    // As native menus: below and right of the pointer when it fits, otherwise above or to the left of it, so the
    // pointer sits at a corner and never over an item (clamping alone could put a submenu under it, opening it).
    const fitsRight = x + w <= window.innerWidth - 4, fitsBelow = y + h <= window.innerHeight - 4;
    const left = Math.max(4, Math.min(fitsRight ? x : x - w, window.innerWidth - w - 4));
    const top = Math.max(4, Math.min(fitsBelow ? y : y - h, window.innerHeight - h - 4));
    this.panel.style.left = `${Math.round(left)}px`;
    this.panel.style.top = `${Math.round(top)}px`;
    this.panel.style.visibility = '';
  }

  /** A submenu beside its row: to the right, or to the left when the right has no room. */
  private openBeside(row: DOMRect): void {
    this.refresh();
    this.panel.style.visibility = 'hidden';
    this.panel.hidden = false;
    this.panel.scrollTop = 0;
    const w = this.panel.offsetWidth, h = this.panel.offsetHeight, pad = 4;
    const right = row.right - 2, left = row.left - w + 2;
    const x = right + w <= window.innerWidth - pad ? right : Math.max(pad, left);
    const y = Math.max(pad, Math.min(row.top - pad, window.innerHeight - h - pad));
    this.panel.style.left = `${Math.round(x)}px`;
    this.panel.style.top = `${Math.round(y)}px`;
    this.panel.style.visibility = '';
  }

  private refresh(): void {
    for (const r of this.rows) {
      const cmd = this.commands.get(r.id);
      if (!cmd) continue;
      setText(q(r.node, '.cv-menu-label'), this.commands.label(cmd));
      setDisabled(r.node, !this.commands.enabled(cmd));
      setClass(r.node, 'is-checked', !!cmd.checked?.());
    }
  }

  close(): void {
    this.closeSub();
    if (this.panel.hidden) return;
    this.panel.hidden = true;
    this.onClose?.();
  }
}

/** A menubar menu; `items` may be a function, resolved each time the menu opens (File's Open Recent section). */
export interface MenuDef { label: string; items: MenuItem[] | (() => MenuItem[]) }

const itemsOf = (def: MenuDef): MenuItem[] => (typeof def.items === 'function' ? def.items() : def.items);

export class MenuBar {
  private readonly menus: Array<{ btn: HTMLButtonElement; menu: PopupMenu; def: MenuDef }> = [];
  private openIndex = -1;

  constructor(host: HTMLElement, commands: Commands, defs: MenuDef[]) {
    defs.forEach((def, i) => {
      const btn = el<HTMLButtonElement>('<button class="cv-menu-btn" type="button" aria-haspopup="menu"></button>');
      btn.textContent = def.label;
      host.appendChild(btn);
      const menu = new PopupMenu(commands);
      menu.setItems(itemsOf(def));
      menu.onClose = () => { if (this.openIndex === i) { this.openIndex = -1; setClass(btn, 'is-open', false); } };
      btn.addEventListener('pointerdown', (e) => { if (e.button !== 0) return; e.preventDefault(); if (this.openIndex === i) this.close(); else this.open(i); });
      btn.addEventListener('pointerenter', () => { if (this.openIndex >= 0 && this.openIndex !== i) this.open(i); });
      this.menus.push({ btn, menu, def });
    });
    document.addEventListener('pointerdown', (e) => {
      if (this.openIndex < 0) return;
      const m = this.menus[this.openIndex];
      const t = e.target as Node;
      if (!m.menu.contains(t) && !m.btn.contains(t)) this.close();
    }, true);
    window.addEventListener('blur', () => this.close());
    window.addEventListener('resize', () => this.close());
  }

  get isOpen(): boolean { return this.openIndex >= 0; }

  open(i: number): void {
    this.close();
    const m = this.menus[i];
    m.menu.setItems(itemsOf(m.def));
    const b = m.btn.getBoundingClientRect();
    this.openIndex = i;
    setClass(m.btn, 'is-open', true);
    m.menu.openAt(b.left, b.bottom + 4);
  }

  close(): void {
    if (this.openIndex < 0) return;
    const m = this.menus[this.openIndex];
    this.openIndex = -1;
    setClass(m.btn, 'is-open', false);
    m.menu.close();
  }
}
