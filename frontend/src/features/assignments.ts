import '../styles/review.css';
import '../styles/interactive.css';
import '../styles/motion.css';
import { editor } from '../editor/editor';
import { api, engine } from '../engine/client';
import { confirmAction } from '../ui/dialogs';
import { enhanceSelect } from '../ui/dropdown';
import { el, patchList, q, setChecked, setClass, setDisabled, setHidden, setStyleVar, setText, setTitle, setValue } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';
import { LivePanel } from './review';

type Rect4 = [number, number, number, number];
interface Lock { by: string; since: number; assignment: string | null; label: string }
interface Frame { id: number; page: number; pageName: string; spread: number; bounds: Rect4 }
interface Story { id: number; preview: string; length: number; frames: Frame[]; lock: Lock | null; assignments: string[] }
interface Assignment { id: string; name: string; assignee: string; color: string; stories: number[]; exported: number | null }
interface Overview { user: { name: string; role: 'designer' | 'editor' }; docId: string | null; assignments: Assignment[]; stories: Story[]; package: unknown }
const EMPTY: Overview = { user: { name: 'Designer', role: 'designer' }, docId: null, assignments: [], stories: [], package: null };
const LOCK_SVG = '<svg viewBox="0 0 16 16" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><rect x="3.5" y="7" width="9" height="6.5" rx="1.2"/><path d="M5.5 7V5a2.5 2.5 0 0 1 5 0v2"/></svg>';
const KEEP_TOOLS = new Set(['type', 'hand', 'zoom']);
const HIDE_MENUS = new Set(['Layout', 'Object', 'Table']);

const when = (s: number): string => {
  const d = new Date(s * 1000);
  const today = new Date().toDateString() === d.toDateString();
  return today ? d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }) : d.toLocaleDateString([], { month: 'short', day: 'numeric' }) + ' ' + d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
};
const hex = (c: string | undefined): string => (c && /^#[0-9a-f]{6}$/i.test(c) ? c : '#4b8fe5');

class WorkflowStore {
  data: Overview = EMPTY;
  private busy: Promise<void> | null = null;
  private again = false;
  private readonly subs = new Set<() => void>();

  on(fn: () => void): void { this.subs.add(fn); }

  async refresh(): Promise<void> {
    if (this.busy) { this.again = true; return this.busy; }
    this.busy = (async () => {
      try { this.data = await api<Overview>('/workflow'); } catch { this.data = { ...EMPTY, user: this.data.user }; }
      for (const f of this.subs) f();
    })().finally(() => { this.busy = null; if (this.again) { this.again = false; void this.refresh(); } });
    return this.busy;
  }

  async post<T = unknown>(path: string, json: unknown): Promise<T | undefined> {
    try { return await api<T>(path, { json }); } catch (e) { toast((e as Error).message, 'error'); return undefined; }
  }
}

export class AssignmentsPanel extends LivePanel {
  readonly root = el(`<div class="frv-panel fwf-panel">
    <div class="fwf-who">
      <label class="fin-f"><span>You are</span><input class="fin-in fwf-name" type="text" spellcheck="false" aria-label="Your name"></label>
      <label class="fin-f"><span>Role</span><select class="cv-select fwf-rolesel" aria-label="Role"><option value="designer">Designer</option><option value="editor">Editor</option></select></label>
    </div>
    <p class="frv-hint fwf-what"></p>
    <section class="fwf-sec fwf-designer">
      <p class="frv-hint">Assignments</p>
      <div class="frv-list fwf-alist" role="list" aria-label="Assignments"></div>
      <div class="fwf-form"><label class="fin-f"><span>Name</span><input class="fin-in fwf-aname" type="text" spellcheck="false" placeholder="Chapter 1"></label><input class="fwf-colour" type="color" aria-label="Colour" title="Colour"></div>
      <div class="fwf-form"><label class="fin-f"><span>Assigned to</span><input class="fin-in fwf-ato" type="text" spellcheck="false" placeholder="Editor name"></label><button class="cv-btn frv-btn fwf-new" type="button" title="Make an assignment of the ticked stories and the selected frames">Create</button></div>
    </section>
    <section class="fwf-sec">
      <p class="frv-hint fwf-shint"></p>
      <div class="frv-list fwf-slist" role="list" aria-label="Stories"></div>
    </section>
    <div class="frv-foot"><span class="frv-count"></span><button class="cv-btn frv-btn fwf-open" type="button" title="Open an assignment package sent to you">Open Package</button><button class="cv-btn frv-btn fwf-return fwf-editor" type="button" title="Save your edited stories as a package to send back">Return Package</button><button class="cv-btn frv-btn fwf-update fwf-designer" type="button" title="Bring the stories of an edited package back into the layout">Update Content</button></div>
  </div>`);
  private readonly picked = new Set<number>();
  private colour = '#4b8fe5';

  constructor(ws: Workspace, private readonly store: WorkflowStore, private readonly setRole: (name: string, role: 'designer' | 'editor') => Promise<void>) {
    super(ws);
    enhanceSelect(q<HTMLSelectElement>(this.root, '.fwf-rolesel'));
    const name = q<HTMLInputElement>(this.root, '.fwf-name');
    name.addEventListener('keydown', (e) => { e.stopPropagation(); if (e.key === 'Enter') name.blur(); });
    name.addEventListener('change', () => void this.setRole(name.value, this.store.data.user.role));
    q<HTMLSelectElement>(this.root, '.fwf-rolesel').addEventListener('change', (e) => void this.setRole(name.value, (e.target as HTMLSelectElement).value as 'designer' | 'editor'));
    for (const c of ['.fwf-aname', '.fwf-ato']) { const n = q<HTMLInputElement>(this.root, c); n.addEventListener('keydown', (e) => { e.stopPropagation(); if (e.key === 'Enter') n.blur(); }); }
    const col = q<HTMLInputElement>(this.root, '.fwf-colour');
    col.value = this.colour;
    col.addEventListener('input', () => { this.colour = col.value; });
    q(this.root, '.fwf-new').addEventListener('click', () => void this.create());
    q(this.root, '.fwf-open').addEventListener('click', () => void this.openPackage());
    q(this.root, '.fwf-return').addEventListener('click', () => void this.returnPackage());
    q(this.root, '.fwf-update').addEventListener('click', () => void this.updateContent());
    store.on(() => this.paint());
    engine.on('workflow', () => void this.store.refresh());
  }

  protected async load(): Promise<void> { await this.store.refresh(); }

  private async create(): Promise<void> {
    const frames = (editor.selection?.items ?? []).filter((i) => this.store.data.stories.some((s) => s.frames.some((f) => f.id === i)));
    if (!this.picked.size && !frames.length) { toast('Tick the stories to include, or select their frames on the page.'); return; }
    const name = q<HTMLInputElement>(this.root, '.fwf-aname').value.trim();
    const to = q<HTMLInputElement>(this.root, '.fwf-ato').value.trim();
    const r = await this.store.post('/workflow/assignment', { name, assignee: to, color: this.colour, stories: [...this.picked], frames });
    if (r) { this.picked.clear(); q<HTMLInputElement>(this.root, '.fwf-aname').value = ''; await this.store.refresh(); toast(`Assignment created${to ? ` for ${to}` : ''}.`); }
  }

  private async exportAssignment(a: Assignment): Promise<void> {
    if (!a.assignee) { toast('Give the assignment an editor first: set Assigned to and create it again, or the stories are checked out to "Editor".'); }
    try {
      const blob = await api<Blob>('/workflow/assignment/export', { json: { id: a.id } });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `${a.name.replace(/[^\w -]+/g, '_')}.fdes-assignment`;
      link.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 4000);
      toast(`Exported ${a.name}: its stories are now checked out to ${a.assignee || 'Editor'}.`);
    } catch (e) { toast((e as Error).message, 'error'); }
    await this.store.refresh();
  }

  private async form(path: string, file: File, extra: Record<string, string> = {}): Promise<any> {
    const fd = new FormData();
    fd.append('file', file);
    for (const [k, v] of Object.entries(extra)) fd.append(k, v);
    const r = await fetch(`/api${path}`, { method: 'POST', body: fd });
    const j = await r.json().catch(() => ({}));
    if (!r.ok) throw new Error(typeof j.detail === 'string' ? j.detail : 'The engine refused the package.');
    return j;
  }

  private async openPackage(): Promise<void> {
    const [file] = await this.ws.pickFiles('.fdes-assignment,.zip');
    if (!file) return;
    try {
      await this.form('/workflow/assignment/open', file);
      editor.refreshMeta();
      await editor.sync();
      await this.store.refresh();
      toast('Assignment opened: you are in the Editor role with its stories checked out to you.');
    } catch (e) { toast((e as Error).message, 'error'); }
  }

  private async returnPackage(): Promise<void> {
    try {
      const blob = await api<Blob>('/workflow/assignment/return', { json: {} });
      const url = URL.createObjectURL(blob);
      const link = document.createElement('a');
      link.href = url;
      link.download = `${(editor.doc?.title ?? 'assignment').replace(/\.[^.]+$/, '').replace(/[^\w -]+/g, '_')}-edited.fdes-assignment`;
      link.click();
      window.setTimeout(() => URL.revokeObjectURL(url), 4000);
      toast('Package ready: send it back to the designer.');
    } catch (e) { toast((e as Error).message, 'error'); }
  }

  private async updateContent(): Promise<void> {
    const [file] = await this.ws.pickFiles('.fdes-assignment,.zip');
    if (!file) return;
    try {
      let r = await this.form('/workflow/assignment/update', file);
      if (r.conflicts?.length && await confirmAction('Update Content', `${r.conflicts.length} story changed in the layout after the package was exported. Overwrite ${r.conflicts.length === 1 ? 'it' : 'them'} with the editor's text?`, 'Update Anyway')) {
        r = await this.form('/workflow/assignment/update', file, { force: 'true' });
      }
      editor.schedule();
      await this.store.refresh();
      toast(`Updated ${r.updated.length} ${r.updated.length === 1 ? 'story' : 'stories'}${r.conflicts.length ? `, ${r.conflicts.length} left as they are` : ''}. ${r.checkedIn.length} checked in.`);
    } catch (e) { toast((e as Error).message, 'error'); }
  }

  private paint(): void {
    const d = this.store.data;
    const me = d.user;
    const editorRole = me.role === 'editor';
    setValue(q<HTMLInputElement>(this.root, '.fwf-name'), me.name);
    setValue(q<HTMLSelectElement>(this.root, '.fwf-rolesel'), me.role);
    for (const n of this.root.querySelectorAll<HTMLElement>('.fwf-designer')) setHidden(n, editorRole);
    for (const n of this.root.querySelectorAll<HTMLElement>('.fwf-editor')) setHidden(n, !editorRole);
    setText(q(this.root, '.fwf-what'), !editor.hasDoc ? 'Open a document to work with assignments.' : editorRole ? 'Editor: text can be changed only in stories checked out to you. Layout tools are off.' : 'Designer: give stories to an editor as an assignment, then update the layout when the edited package comes back. Checked-out stories cannot be changed by anyone else.');
    setText(q(this.root, '.fwf-shint'), editorRole ? 'Stories' : 'Stories (tick to include in a new assignment)');
    const byId = new Map(d.assignments.map((a) => [a.id, a]));
    patchList<Assignment>(q(this.root, '.fwf-alist'), d.assignments, {
      key: (a) => a.id,
      create: (a) => {
        const n = el('<div class="frv-row fwf-row" role="listitem"><span class="fwf-dot"></span><span class="fwf-main"><span class="frv-text fwf-aname-t"></span><span class="fwf-sub"></span></span><span class="frv-acts"><button class="cv-btn frv-btn frv-mini fwf-exp" type="button" title="Export the package for the editor and check its stories out to them">Export</button><button class="cv-btn frv-btn frv-mini fwf-set" type="button" title="Replace its stories with the ticked ones">Set Stories</button><button class="cv-btn frv-btn frv-mini fwf-del" type="button" title="Delete the assignment and release its locks">Delete</button></span></div>');
        q(n, '.fwf-exp').addEventListener('click', () => { const x = byId.get(n.dataset.key ?? ''); if (x) void this.exportAssignment(x); });
        q(n, '.fwf-set').addEventListener('click', async () => {
          const x = byId.get(n.dataset.key ?? '');
          if (!x || !this.picked.size) { toast('Tick the stories it should contain first.'); return; }
          await this.store.post('/workflow/assignment', { id: x.id, name: x.name, assignee: x.assignee, color: x.color, stories: [...this.picked] });
          this.picked.clear(); await this.store.refresh();
        });
        q(n, '.fwf-del').addEventListener('click', async () => {
          const x = byId.get(n.dataset.key ?? '');
          if (x && await confirmAction('Delete Assignment', `Delete ${x.name}? Its checked-out stories are released.`, 'Delete')) { await this.store.post('/workflow/assignment/delete', { id: x.id }); await this.store.refresh(); }
        });
        this.fillA(n, a);
        return n;
      },
      update: (n, a) => this.fillA(n, a),
    });
    patchList<Story>(q(this.root, '.fwf-slist'), d.stories, {
      key: (s) => String(s.id),
      create: (s) => {
        const n = el('<div class="frv-row fwf-row fwf-srow" role="listitem"><span class="fwf-dot"></span><span class="fwf-main"><span class="frv-text fwf-sname"></span><span class="fwf-sub"></span></span><span class="frv-acts"><label class="frv-check fwf-pickl fwf-designer" title="Include in a new assignment"><input type="checkbox" class="fwf-pick-i"></label><button class="cv-btn frv-btn frv-mini fwf-act" type="button"></button></span></div>');
        const sid = () => Number(n.dataset.key);
        q(n, '.fwf-main').addEventListener('click', () => {
          const s2 = this.store.data.stories.find((x) => x.id === sid());
          const f = s2?.frames[0];
          if (!f) return;
          void editor.exec('selection.set', { ids: [f.id] }, true).then(() => { editor.schedule(); this.ws.viewport.showPage(f.page); });
        });
        q<HTMLInputElement>(n, '.fwf-pick-i').addEventListener('change', (e) => { if ((e.target as HTMLInputElement).checked) this.picked.add(sid()); else this.picked.delete(sid()); });
        q(n, '.fwf-act').addEventListener('click', () => void this.act(sid()));
        this.fillS(n, s);
        return n;
      },
      update: (n, s) => this.fillS(n, s),
    });
    setText(q(this.root, '.frv-count'), d.stories.length ? `${d.stories.filter((s) => s.lock).length} of ${d.stories.length} checked out` : '');
    setDisabled(q<HTMLButtonElement>(this.root, '.fwf-new'), !editor.hasDoc);
    setDisabled(q<HTMLButtonElement>(this.root, '.fwf-return'), !d.package);
    setDisabled(q<HTMLButtonElement>(this.root, '.fwf-update'), !editor.hasDoc || !d.assignments.length);
  }

  private fillA(n: HTMLElement, a: Assignment): void {
    setStyleVar(n, '--fwf-color', hex(a.color));
    setText(q(n, '.fwf-aname-t'), a.name);
    setText(q(n, '.fwf-sub'), `${a.assignee ? `to ${a.assignee}` : 'no editor yet'} · ${a.stories.length} ${a.stories.length === 1 ? 'story' : 'stories'}${a.exported ? ` · exported ${when(a.exported)}` : ''}`);
  }

  private fillS(n: HTMLElement, s: Story): void {
    const me = this.store.data.user;
    const lock = s.lock;
    const asg = this.store.data.assignments.find((a) => s.assignments.includes(a.id));
    setStyleVar(n, '--fwf-color', hex(lock ? this.colorOf(lock) : asg?.color));
    setText(q(n, '.fwf-sname'), s.preview.trim() || `Story ${s.id}`);
    setText(q(n, '.fwf-sub'), lock ? `${lock.by === me.name ? 'Checked out to you' : `Checked out to ${lock.by}`} since ${when(lock.since)}${asg ? ` · ${asg.name}` : ''}` : asg ? `${asg.name} · not checked out` : 'Available');
    const act = q<HTMLButtonElement>(n, '.fwf-act');
    const mine = !!lock && lock.by === me.name;
    const label = !lock ? 'Check Out' : mine ? 'Check In' : me.role === 'designer' ? 'Break Lock' : '';
    setText(act, label);
    setHidden(act, !label);
    setClass(act, 'is-danger', !!lock && !mine);
    setChecked(q<HTMLInputElement>(n, '.fwf-pick-i'), this.picked.has(s.id));
    setTitle(n, lock ? `${s.preview.slice(0, 80)}` : '');
  }

  private colorOf(lock: Lock): string | undefined { return this.store.data.assignments.find((a) => a.id === lock.assignment)?.color; }

  private async act(story: number): Promise<void> {
    const s = this.store.data.stories.find((x) => x.id === story);
    const me = this.store.data.user;
    if (!s) return;
    if (!s.lock) await this.store.post('/workflow/checkout', { stories: [story] });
    else if (s.lock.by === me.name) await this.store.post('/workflow/checkin', { stories: [story] });
    else if (await confirmAction('Break Lock', `${s.lock.by} has this story checked out. Break the lock? Their edits to it can no longer be brought back without choosing Update Anyway.`, 'Break Lock')) await this.store.post('/workflow/checkin', { stories: [story], force: true });
    await this.store.refresh();
  }
}

/** A badge on every checked-out frame: who holds it and since when. Built once per frame, moved with the view. */
class LockLayer {
  private readonly layer = el('<div class="fwf-layer" aria-hidden="true"></div>');
  private items: Array<{ frame: Frame; lock: Lock; color: string; key: string }> = [];

  constructor(private readonly ws: Workspace, private readonly store: WorkflowStore) {
    ws.viewport.stage.appendChild(this.layer);
    ws.viewport.onView = ((prev) => () => { prev?.(); this.place(); })(ws.viewport.onView);
    store.on(() => this.sync());
    editor.on('layout', () => this.place());
  }

  private sync(): void {
    const d = this.store.data;
    this.items = d.stories.flatMap((s) => (s.lock ? s.frames.map((f) => ({ frame: f, lock: s.lock as Lock, color: hex(d.assignments.find((a) => a.id === s.lock?.assignment)?.color ?? (s.lock?.by === d.user.name ? '#4b8fe5' : '#e5654b')), key: String(f.id) })) : []));
    patchList(this.layer, this.items, {
      key: (x) => x.key,
      create: (x) => { const n = el(`<div class="fwf-badge">${LOCK_SVG}<span></span></div>`); this.fill(n, x); return n; },
      update: (n, x) => this.fill(n, x),
    });
    this.place();
  }

  private fill(n: HTMLElement, x: { lock: Lock; color: string }): void {
    setStyleVar(n, '--fwf-color', x.color);
    setText(q(n, 'span'), `${x.lock.by} · since ${when(x.lock.since)}`);
  }

  private place(): void {
    const slots: any[] = editor.layout?.slots ?? [];
    for (const n of Array.from(this.layer.children) as HTMLElement[]) {
      const x = this.items.find((i) => i.key === n.dataset.key);
      const s = x && (slots.find((z) => (typeof z.spread === 'number' ? z.spread : z.spread?.index) === x.frame.spread && (z.spread?.kind ?? 'doc') === 'doc') ?? slots[x.frame.spread]);
      if (!x || !s) { setHidden(n, true); continue; }
      const [a, , , d, e, f] = s.xf as number[];
      const [sx, sy] = this.ws.viewport.toScreen(a * x.frame.bounds[0] + e, d * x.frame.bounds[1] + f);
      setHidden(n, false);
      n.style.transform = `translate(${Math.round(sx)}px, ${Math.round(sy - n.offsetHeight - 2)}px)`;
    }
  }
}

export function installAssignments(ws: Workspace): void {
  const store = new WorkflowStore();
  const chip = el('<span class="fwf-role" hidden><span class="fwf-role-t"></span></span>');
  let panel: AssignmentsPanel | null = null;
  let lock: LockLayer | null = null;

  const applyRole = (): void => {
    const me = store.data.user;
    const on = me.role === 'editor';
    document.body.classList.toggle('fd-role-editor', on);
    setHidden(chip, !on);
    setText(q(chip, '.fwf-role-t'), `Editor: ${me.name}`);
    for (const b of Array.from(document.querySelectorAll<HTMLElement>('.cv-rail-tools .cv-tool'))) {
      const group = editor.tools[Number(b.dataset.group)] ?? [];
      setClass(b, 'fwf-hide', on && !group.some((t) => KEEP_TOOLS.has(t.id)));
    }
    for (const b of Array.from(document.querySelectorAll<HTMLElement>('.cv-menubar .cv-menu-btn'))) setClass(b, 'fwf-hide', on && HIDE_MENUS.has(b.textContent ?? ''));
    if (on && editor.hasDoc && editor.tool !== 'type' && !KEEP_TOOLS.has(editor.tool)) void editor.setTool('type');
  };

  const setRole = async (name: string, role: 'designer' | 'editor'): Promise<void> => {
    await store.post('/workflow/user', { name, role });
    await store.refresh();
    applyRole();
  };

  store.on(applyRole);
  editor.on('ready', () => { lock ??= new LockLayer(ws, store); ws.setControl('role', chip, 90); void store.refresh(); });
  editor.on('tool', applyRole);
  editor.on('docs', () => void store.refresh());
  editor.on('change', (e: { docChanged?: boolean } | undefined) => { if (e?.docChanged) void store.refresh(); });
  engine.on('open', () => void store.refresh());
  engine.on('workflow', () => void store.refresh());
  window.addEventListener('fd-workspace', (ev) => {
    const name = (ev as CustomEvent<string>).detail;
    if (name === 'Editor' && store.data.user.role !== 'editor') void setRole(store.data.user.name, 'editor');
    else if (name !== 'Editor' && store.data.user.role === 'editor') void setRole(store.data.user.name, 'designer');
  });

  ws.registerPanel({ id: 'assignments', label: 'Assignments', group: 'review', column: 'side', order: 40, create: () => (panel = new AssignmentsPanel(ws, store, setRole)).root, onShow: () => panel?.onShow() });
  ws.addCommands(
    { id: 'ui.assignments', label: 'Assignments Panel', run: () => ws.showPanel('assignments') },
    { id: 'ui.roleEditor', label: 'Editor Role', checked: () => store.data.user.role === 'editor', run: () => void setRole(store.data.user.name, store.data.user.role === 'editor' ? 'designer' : 'editor') },
  );
  ws.addMenuItems({ menu: 'Window', items: ['ui.assignments', 'ui.roleEditor'], at: 'end' });
}
