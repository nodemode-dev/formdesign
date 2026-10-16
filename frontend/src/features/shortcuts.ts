import '../styles/shortcuts.css';
import { editor } from '../editor/editor';
import { api } from '../engine/client';
import { comboOf, formatKeys, type Command } from '../ui/commands';
import { confirmAction, Dialog, promptName } from '../ui/dialogs';
import { enhanceSelect } from '../ui/dropdown';
import { el, patchList, q, setClass, setDisabled, setHidden, setText } from '../ui/dom';
import { icon } from '../ui/icons';
import { coreCombo } from '../ui/shell';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface KeySet { name: string; keys: Record<string, string[]> }
interface Row { id: string; label: string; area: string; keys: string }

const SET_KEY = 'formdes.shortcuts.set';
const CACHE_KEY = 'formdes.shortcuts.keys';
const AREAS = ['File', 'Edit', 'Layout', 'Type', 'Object', 'Table', 'Effect', 'View', 'Window', 'Help', 'Tools', 'Other'];

const stored = (k: string): string | null => { try { return localStorage.getItem(k); } catch { return null; } };
const store = (k: string, v: string): void => { try { localStorage.setItem(k, v); } catch { } };

class ShortcutsDialog {
  private readonly d = new Dialog('Keyboard Shortcuts', 660);
  private readonly root = el(`<div class="fsc">
    <div class="fsc-top"><label class="fsc-f"><span>Set</span><select class="cv-select fsc-set" aria-label="Shortcut set"></select></label>
      <button class="cv-btn frv-btn fsc-new" type="button">New Set…</button><button class="cv-btn frv-btn fsc-del" type="button">Delete Set</button><button class="cv-btn frv-btn fsc-show" type="button">Show Set…</button></div>
    <div class="fsc-top"><label class="fsc-f"><span>Product area</span><select class="cv-select fsc-area" aria-label="Product area"></select></label>
      <input class="fsc-search" type="search" placeholder="Search commands or keys" aria-label="Search commands" spellcheck="false"></div>
    <div class="fsc-list" role="listbox" aria-label="Commands"></div>
    <div class="fsc-pane">
      <div class="fsc-cur"><span class="fsc-lab">Current shortcuts</span><div class="fsc-keys"></div></div>
      <div class="fsc-assign"><span class="fsc-lab">New shortcut</span><div class="fsc-arow"><input class="fsc-capture" type="text" readonly placeholder="Click, then press the keys" aria-label="New shortcut"><button class="cv-btn cv-btn-primary frv-btn fsc-go" type="button">Assign</button></div><p class="fsc-who"></p></div>
    </div>
  </div>`);
  private readonly setSel = q<HTMLSelectElement>(this.root, '.fsc-set');
  private readonly areaSel = q<HTMLSelectElement>(this.root, '.fsc-area');
  private readonly search = q<HTMLInputElement>(this.root, '.fsc-search');
  private readonly capture = q<HTMLInputElement>(this.root, '.fsc-capture');
  private picked = '';
  private combo = '';
  private areas = new Map<string, string>();

  constructor(private readonly ws: Workspace, private readonly sets: () => KeySet[], private readonly active: () => string, private readonly choose: (name: string) => void, private readonly changed: () => void, private readonly create: (base: Record<string, string[]>) => Promise<boolean>, private readonly removeSet: (name: string) => Promise<void>) {
    this.d.body.append(this.root);
    this.areaSel.append(new Option('All', ''), ...AREAS.map((a) => new Option(a, a)));
    enhanceSelect(this.setSel);
    enhanceSelect(this.areaSel);
    this.setSel.addEventListener('change', () => { this.choose(this.setSel.value); this.render(); });
    this.areaSel.addEventListener('change', () => this.render());
    this.search.addEventListener('input', () => this.render());
    q(this.root, '.fsc-new').addEventListener('click', async () => { if (await this.create(this.ws.commands.overrides())) this.render(); });
    q(this.root, '.fsc-del').addEventListener('click', async () => { const n = this.active(); if (n !== 'Default' && await confirmAction('Delete Set', `Delete the shortcut set ${n}? The default shortcuts come back.`, 'Delete', true)) { await this.removeSet(n); this.render(); } });
    q(this.root, '.fsc-show').addEventListener('click', () => this.showSet());
    this.capture.addEventListener('keydown', (e) => this.onCapture(e));
    q(this.root, '.fsc-go').addEventListener('click', () => void this.assign());
    this.d.button('Reset Set', 'ghost', () => void this.resetSet());
    this.d.button('Close', 'primary', () => this.d.close());
  }

  open(): void {
    this.areas = this.ws.menuAreas();
    this.combo = '';
    this.capture.value = '';
    this.search.value = '';
    this.render();
    this.d.open(this.search);
  }

  private areaOf(c: Command): string { return c.id.startsWith('tool.') ? 'Tools' : this.areas.get(c.id) ?? 'Other'; }

  private rows(): Row[] {
    const term = this.search.value.trim().toLowerCase(), area = this.areaSel.value;
    return this.ws.commands.all()
      .map((c) => ({ id: c.id, label: String(this.ws.commands.label(c) ?? '').replace(/…$/, ''), area: this.areaOf(c), keys: (c.keys ?? []).map(formatKeys).join(', ') }))
      .filter((r) => r.label && !r.id.startsWith('ui.workspace.') && !r.id.startsWith('ui.group.') && (!area || r.area === area) && (!term || r.label.toLowerCase().includes(term) || r.keys.toLowerCase().includes(term)))
      .sort((a, b) => a.area === b.area ? a.label.localeCompare(b.label) : AREAS.indexOf(a.area) - AREAS.indexOf(b.area));
  }

  private render(): void {
    const names = ['Default', ...this.sets().map((s) => s.name)];
    const key = names.join('\n');
    if (this.setSel.dataset.key !== key) { this.setSel.dataset.key = key; this.setSel.replaceChildren(...names.map((n) => new Option(n === 'Default' ? '[Default]' : n, n))); }
    this.setSel.value = this.active();
    setDisabled(q<HTMLButtonElement>(this.root, '.fsc-del'), this.active() === 'Default');
    const rows = this.rows();
    if (!rows.some((r) => r.id === this.picked)) this.picked = rows[0]?.id ?? '';
    patchList<Row>(q(this.root, '.fsc-list'), rows, {
      key: (r) => r.id,
      create: (r) => {
        const n = el('<button class="fsc-row" type="button" role="option"><span class="fsc-name"></span><span class="fsc-area"></span><span class="fsc-k"></span></button>');
        n.addEventListener('click', () => { this.picked = n.dataset.key as string; this.render(); });
        this.row(n, r);
        return n;
      },
      update: (n, r) => this.row(n, r),
    });
    const cmd = this.ws.commands.get(this.picked);
    patchList<string>(q(this.root, '.fsc-keys'), cmd?.keys ?? [], {
      key: (k) => k,
      create: (k) => {
        const n = el(`<div class="fsc-key"><kbd class="fsc-cap"></kbd><button class="cv-icon-btn fsc-x" type="button" title="Remove" aria-label="Remove">${icon('close', 12)}</button></div>`);
        setText(q(n, '.fsc-cap'), formatKeys(k));
        q(n, '.fsc-x').addEventListener('click', () => void this.edit(() => { const c = this.ws.commands.get(this.picked); if (c) this.ws.commands.setKeys(c.id, (c.keys ?? []).filter((x) => x !== n.dataset.key)); }));
        return n;
      },
      update: () => undefined,
    });
    setClass(q(this.root, '.fsc-keys'), 'is-none', !(cmd?.keys ?? []).length);
    this.who();
  }

  private row(n: HTMLElement, r: Row): void {
    setText(q(n, '.fsc-name'), r.label);
    setText(q(n, '.fsc-area'), r.area);
    setText(q(n, '.fsc-k'), r.keys);
    setClass(n, 'is-on', r.id === this.picked);
    n.setAttribute('aria-selected', String(r.id === this.picked));
  }

  private onCapture(e: KeyboardEvent): void {
    if (e.key === 'Tab' && !e.ctrlKey && !e.metaKey && !e.altKey) return;
    e.preventDefault();
    e.stopPropagation();
    if (['Shift', 'Control', 'Alt', 'Meta', 'CapsLock'].includes(e.key)) return;
    if (e.key === 'Escape' && !e.shiftKey && !e.ctrlKey && !e.metaKey && !e.altKey) { this.combo = ''; this.capture.value = ''; this.who(); return; }
    this.combo = comboOf(e);
    this.capture.value = formatKeys(this.combo);
    this.who();
  }

  private who(): void {
    const p = q(this.root, '.fsc-who'), other = this.combo ? this.ws.commands.forCombo(this.combo) : undefined;
    setHidden(p, !this.combo);
    setText(p, !this.combo ? '' : other ? `Currently assigned to: ${this.ws.commands.label(other).replace(/…$/, '')}${other.id === this.picked ? ' (this command)' : ''}` : 'Currently assigned to: [unassigned]');
    setClass(p, 'is-taken', !!other && other.id !== this.picked);
    setDisabled(q<HTMLButtonElement>(this.root, '.fsc-go'), !this.combo || !this.picked || other?.id === this.picked);
  }

  private async edit(change: () => void): Promise<void> {
    if (this.active() === 'Default') {
      toast('The default set cannot be changed; name a new set for your shortcuts.');
      if (!(await this.create({}))) return;
    }
    change();
    this.changed();
    this.render();
  }

  private async assign(): Promise<void> {
    const c = this.ws.commands.get(this.picked), combo = this.combo;
    if (!c || !combo) return;
    await this.edit(() => this.ws.commands.setKeys(c.id, [...(c.keys ?? []).filter((k) => k !== combo), combo]));
    this.combo = '';
    this.capture.value = '';
    this.who();
  }

  private async resetSet(): Promise<void> {
    if (this.active() === 'Default') return;
    if (!(await confirmAction('Reset Set', `Return every shortcut in ${this.active()} to the default?`, 'Reset'))) return;
    this.ws.commands.resetKeys();
    this.changed();
    this.render();
  }

  private showSet(): void {
    const by = new Map<string, string[]>();
    for (const c of this.ws.commands.all()) {
      if (!c.keys?.length || c.id.startsWith('ui.workspace.')) continue;
      const a = this.areaOf(c);
      by.set(a, [...(by.get(a) ?? []), `${this.ws.commands.label(c).replace(/…$/, '')}\t${c.keys.map(formatKeys).join(', ')}`]);
    }
    const text = [`formdes keyboard shortcuts: ${this.active()}`, '', ...AREAS.filter((a) => by.has(a)).flatMap((a) => [a, ...(by.get(a) ?? []).sort(), ''])].join('\n');
    const a = document.createElement('a');
    a.href = URL.createObjectURL(new Blob([text], { type: 'text/plain' }));
    a.download = `${this.active().replace(/[\\/:*?"<>|]+/g, ' ').trim() || 'shortcuts'}.txt`;
    a.click();
    window.setTimeout(() => URL.revokeObjectURL(a.href), 2000);
  }
}

export function installShortcuts(ws: Workspace): void {
  let sets: KeySet[] = [];
  let active = stored(SET_KEY) || 'Default';
  let timer = 0;
  let dialog: ShortcutsDialog | null = null;

  const apply = (keys: Record<string, string[]>): void => {
    ws.commands.resetKeys();
    for (const [id, k] of Object.entries(keys)) ws.commands.setKeys(id, k);
  };

  const cached = (): Record<string, string[]> => { try { const v = JSON.parse(stored(CACHE_KEY) ?? '{}'); return v && typeof v === 'object' ? v as Record<string, string[]> : {}; } catch { return {}; } };

  const persist = (): void => {
    const keys = ws.commands.overrides();
    store(CACHE_KEY, JSON.stringify(keys));
    if (active === 'Default') return;
    const s = sets.find((x) => x.name === active);
    if (s) s.keys = keys;
    window.clearTimeout(timer);
    const name = active;
    timer = window.setTimeout(() => { void api('/prefs/shortcuts', { json: { name, keys } }).catch((err) => toast((err as Error).message, 'error')); }, 400);
  };

  const choose = (name: string): void => {
    const s = sets.find((x) => x.name === name);
    active = s ? s.name : 'Default';
    store(SET_KEY, active);
    apply(s?.keys ?? {});
    store(CACHE_KEY, JSON.stringify(s?.keys ?? {}));
  };

  const create = async (base: Record<string, string[]>): Promise<boolean> => {
    const name = (await promptName('New Set', active === 'Default' ? 'My Shortcuts' : `${active} copy`, 'Create'))?.trim();
    if (!name) return false;
    if (name.toLowerCase() === 'default') { toast('Default is built in; choose another name.', 'error'); return false; }
    try {
      const r = await api<KeySet>('/prefs/shortcuts', { json: { name, keys: base } });
      sets = [...sets.filter((s) => s.name.toLowerCase() !== r.name.toLowerCase()), r].sort((a, b) => a.name.localeCompare(b.name));
      choose(r.name);
      return true;
    } catch (err) { toast((err as Error).message, 'error'); return false; }
  };

  const removeSet = async (name: string): Promise<void> => {
    try {
      await api(`/prefs/shortcuts?name=${encodeURIComponent(name)}`, { method: 'DELETE' });
      sets = sets.filter((s) => s.name !== name);
      choose('Default');
    } catch (err) { toast((err as Error).message, 'error'); }
  };

  ws.addCommands({ id: 'ui.shortcuts', label: 'Keyboard Shortcuts…', keys: ['Alt+Shift+Mod+K'], run: () => { dialog ??= new ShortcutsDialog(ws, () => sets, () => active, choose, persist, create, removeSet); dialog.open(); } });

  let tools = false;
  editor.on('commands', () => {
    if (!tools && editor.tools.length) {
      tools = true;
      const taken = new Set(ws.commands.all().flatMap((c) => c.keys ?? []));
      for (const t of editor.tools.flat()) {
        if (ws.commands.get(`tool.${t.id}`)) continue;
        const combo = coreCombo(t.shortcut);
        const keys = combo && !taken.has(combo) ? [combo] : [];
        for (const k of keys) taken.add(k);
        ws.commands.add({ id: `tool.${t.id}`, label: t.label, keys, enabled: () => editor.hasDoc, run: () => void editor.setTool(t.id) });
      }
    }
    apply(active === 'Default' ? {} : cached());
  });

  void api<{ items: KeySet[] }>('/prefs/shortcuts').then((r) => {
    sets = r.items ?? [];
    const s = sets.find((x) => x.name === active);
    if (!s && active !== 'Default') { choose('Default'); return; }
    if (s && JSON.stringify(s.keys) !== JSON.stringify(cached())) { store(CACHE_KEY, JSON.stringify(s.keys)); if (editor.commands.length) apply(s.keys); }
  }).catch(() => { });
}
