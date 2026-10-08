import { editor } from '../../editor/editor';
import { api, engine, type ChangeEvent } from '../../engine/client';
import { confirmAction, Dialog } from '../../ui/dialogs';
import { enhanceSelect } from '../../ui/dropdown';
import { el, patchList, q, setClass, setDisabled, setHidden, setText, setTitle } from '../../ui/dom';
import { icon } from '../../ui/icons';
import { toolIcon } from '../../ui/tool-icons';
import type { Workspace } from '../../ui/workspace';
import { docIcon } from './icons';

interface Issue { severity: 'error' | 'warning' | string; kind: string; message: string; item?: number | null; page?: number | null }
interface Report { errors?: number; warnings?: number; issues?: Issue[] }
interface Row { key: string; issue: Issue; title: string; detail: string }
interface Group { kind: string; rows: Row[]; errors: number }

const KINDS: Record<string, { label: string; icon: string; order: number }> = {
  overset: { label: 'Overset text', icon: 'overset', order: 0 },
  missingFont: { label: 'Missing fonts', icon: 'missingFont', order: 1 },
  missingLink: { label: 'Missing links', icon: 'missingLink', order: 2 },
  lowResolution: { label: 'Low resolution images', icon: 'lowResolution', order: 3 },
  rgbColor: { label: 'RGB in print', icon: 'rgbColor', order: 4 },
  emptyText: { label: 'Empty text frames', icon: 'emptyText', order: 5 },
  bleed: { label: 'Missing bleed', icon: 'issue', order: 6 },
  pageCount: { label: 'Page count', icon: 'issue', order: 7 },
  spotColors: { label: 'Spot inks', icon: 'rgbColor', order: 8 },
  highResolution: { label: 'Very high resolution images', icon: 'lowResolution', order: 9 },
  smallType: { label: 'Small type', icon: 'issue', order: 10 },
  trimSafety: { label: 'Text near the trim', icon: 'issue', order: 11 },
  transparency: { label: 'Transparency', icon: 'issue', order: 12 },
};

interface Profile { name: string; note?: string; builtin?: boolean; checks: Record<string, boolean | number | null> }
const PROFILE_KEY = 'formdes.preflight.profile';
const TOGGLES: Array<[string, string]> = [['overset', 'Overset text'], ['fonts', 'Missing fonts'], ['links', 'Missing links'], ['emptyFrames', 'Empty text frames'], ['rgb', 'RGB colour'], ['transparency', 'Transparency']];
const LIMITS: Array<[string, string, string]> = [['minPpi', 'Images below', 'ppi'], ['maxPpi', 'Images above', 'ppi'], ['bleed', 'Bleed of at least', 'pt'], ['safety', 'Text kept inside the trim by', 'pt'], ['minType', 'Type smaller than', 'pt'], ['spotMax', 'Spot inks, at most', ''], ['pageMultiple', 'Page count in multiples of', '']];

function kindInfo(kind: string): { label: string; icon: string; order: number } {
  return KINDS[kind] ?? { label: kind.replace(/([a-z])([A-Z])/g, '$1 $2').replace(/^./, (c) => c.toUpperCase()), icon: 'issue', order: 50 };
}

export class PreflightPanel {
  readonly root = el(`<div class="fd-pf">
    <div class="fd-pf-head"><span class="fd-pf-dot"></span><span class="fd-pf-status"></span><span class="fd-pf-sub"></span><button class="cv-icon-btn fd-pf-run" type="button" title="Check again" aria-label="Check again">${docIcon('refresh', 15)}</button></div>
    <div class="fd-pf-prof"><span class="fd-pf-plab">Profile</span><select class="cv-select fd-pf-psel" aria-label="Preflight profile"></select><button class="cv-btn fd-pf-pdef" type="button" title="Define or change preflight profiles">Define</button></div>
    <div class="fd-pf-list"></div>
    <div class="fd-pf-empty" hidden><span class="fd-pf-empty-icon"><span class="fd-pf-i-ok">${docIcon('check', 22)}</span><span class="fd-pf-i-none">${toolIcon('preflight', 22)}</span></span><p class="fd-pf-empty-title"></p><p class="fd-pf-empty-note"></p></div>
  </div>`);
  private readonly list = q(this.root, '.fd-pf-list');
  private readonly empty = q(this.root, '.fd-pf-empty');
  private readonly collapsed = new Set<string>();
  private timer = 0;
  private running = false;
  private again = false;
  private stale = true;
  private visible = false;
  private uid: number | null = null;
  private stories = new Map<number, string>();
  private rows: Row[] = [];
  private profiles: Profile[] = [];
  private readonly psel = q<HTMLSelectElement>(this.root, '.fd-pf-psel');

  constructor(private readonly ws: Workspace) {
    q(this.root, '.fd-pf-run').addEventListener('click', () => void this.run());
    enhanceSelect(this.psel);
    this.psel.addEventListener('change', () => {
      try { localStorage.setItem(PROFILE_KEY, this.psel.value); } catch { }
      this.schedule(0);
    });
    q(this.root, '.fd-pf-pdef').addEventListener('click', () => void this.define());
    void this.loadProfiles();
    editor.on('change', (e: ChangeEvent) => { if (e?.docChanged) this.schedule(600); });
    editor.on('docs', () => { const uid = editor.doc?.uid ?? null; if (uid !== this.uid) { this.uid = uid; this.schedule(60); } });
    editor.on('selection', () => this.markSelected());
    new IntersectionObserver((entries) => {
      this.visible = entries.some((e) => e.isIntersecting);
      if (this.visible && this.stale) this.schedule(0);
    }).observe(this.root);
    this.renderStatus(null);
  }

  onShow(): void { if (this.stale) this.schedule(0); }

  private schedule(ms: number): void {
    this.stale = true;
    window.clearTimeout(this.timer);
    this.timer = window.setTimeout(() => { if (this.visible) void this.run(); }, ms);
  }

  async run(): Promise<void> {
    if (this.running) { this.again = true; return; }
    this.running = true;
    this.stale = false;
    const slow = window.setTimeout(() => setClass(this.root, 'is-checking', true), 300);
    try {
      if (!editor.hasDoc) { this.render(null); return; }
      const [rep, ins] = await Promise.all([engine.call<Report>('preflight', { profile: this.current()?.checks ?? {} }).then((r) => r.result).catch(() => undefined), editor.inspect()]);
      this.stories = new Map();
      for (const s of ins?.stories ?? []) for (const f of s.frames ?? []) this.stories.set(f, String(s.preview ?? '').replace(//g, '').replace(/\s+/g, ' ').trim());
      this.render(rep ?? { issues: [] });
    } finally {
      window.clearTimeout(slow);
      setClass(this.root, 'is-checking', false);
      this.running = false;
      if (this.again) { this.again = false; void this.run(); }
    }
  }

  private pageName(abs: number | null | undefined): string {
    if (abs === null || abs === undefined) return '';
    const p = editor.layout?.pages.find((x) => x.abs === abs);
    return p ? `p. ${p.name}` : `p. ${abs + 1}`;
  }

  private describe(i: Issue): { title: string; detail: string } {
    const m = i.message ?? '';
    const tail = m.includes(': ') ? m.slice(m.indexOf(': ') + 2) : m;
    switch (i.kind) {
      case 'overset': {
        const preview = i.item !== undefined && i.item !== null ? this.stories.get(i.item) : '';
        const n = Number(tail.match(/\d+/)?.[0] ?? NaN);
        return { title: preview || 'Text frame', detail: Number.isFinite(n) ? `+${n.toLocaleString()} chars` : tail };
      }
      case 'lowResolution':
      case 'highResolution': {
        const name = m.includes(': ') ? m.slice(0, m.indexOf(': ')) : 'Image';
        return { title: name, detail: tail.replace(/^effective\s*/, '').replace(/\s*\(.*\)$/, '') };
      }
      case 'missingLink': return { title: tail === m ? 'Missing graphic' : tail, detail: '' };
      default: return { title: tail || m, detail: '' };
    }
  }

  private render(rep: Report | null): void {
    if (!rep) {
      this.rows = [];
      patchList<Group>(this.list, [], { key: (g) => g.kind, create: () => el('<div></div>'), update: () => undefined });
      this.renderStatus(null);
      return;
    }
    const issues = rep.issues ?? [];
    const seen = new Map<string, number>();
    const groups = new Map<string, Group>();
    for (const i of issues) {
      const base = `${i.kind}|${i.item ?? ''}|${i.page ?? ''}|${i.message}`;
      const n = seen.get(base) ?? 0;
      seen.set(base, n + 1);
      const { title, detail } = this.describe(i);
      const row: Row = { key: n ? `${base}#${n}` : base, issue: i, title, detail };
      let g = groups.get(i.kind);
      if (!g) { g = { kind: i.kind, rows: [], errors: 0 }; groups.set(i.kind, g); }
      g.rows.push(row);
      if (i.severity === 'error') g.errors++;
    }
    const list = [...groups.values()].sort((a, b) => (b.errors > 0 ? 1 : 0) - (a.errors > 0 ? 1 : 0) || kindInfo(a.kind).order - kindInfo(b.kind).order);
    this.rows = list.flatMap((g) => g.rows);
    patchList<Group>(this.list, list, {
      key: (g) => g.kind,
      create: (g) => {
        const k = kindInfo(g.kind);
        const n = el(`<section class="fd-pf-group"><button class="fd-pf-ghead" type="button" aria-expanded="true"><span class="fd-pf-chev">${icon('chevronDown', 14)}</span>${docIcon(k.icon, 15)}<span class="fd-pf-gname"></span><span class="fd-pf-count"></span></button><div class="fd-pf-rows"></div></section>`);
        setText(q(n, '.fd-pf-gname'), k.label);
        q(n, '.fd-pf-ghead').addEventListener('click', () => {
          const key = n.dataset.key ?? '';
          const shut = !this.collapsed.has(key);
          if (shut) this.collapsed.add(key); else this.collapsed.delete(key);
          setClass(n, 'is-collapsed', shut);
          q(n, '.fd-pf-ghead').setAttribute('aria-expanded', String(!shut));
        });
        this.updateGroup(n, g);
        return n;
      },
      update: (n, g) => this.updateGroup(n, g),
    });
    this.renderStatus({ errors: rep.errors ?? issues.filter((i) => i.severity === 'error').length, warnings: rep.warnings ?? issues.filter((i) => i.severity !== 'error').length });
    this.markSelected();
  }

  private updateGroup(n: HTMLElement, g: Group): void {
    setText(q(n, '.fd-pf-count'), String(g.rows.length));
    setClass(n, 'is-error', g.errors > 0);
    setClass(n, 'is-collapsed', this.collapsed.has(g.kind));
    patchList<Row>(q(n, '.fd-pf-rows'), g.rows, {
      key: (r) => r.key,
      create: (r) => {
        const b = el<HTMLButtonElement>('<button class="fd-pf-row" type="button"><span class="fd-pf-sev"></span><span class="fd-pf-title"></span><span class="fd-pf-detail"></span><span class="fd-pf-page"></span></button>');
        b.addEventListener('click', () => { const row = this.rows.find((x) => x.key === b.dataset.key); if (row) void this.reveal(row.issue); });
        this.updateRow(b, r);
        return b;
      },
      update: (b, r) => this.updateRow(b, r),
    });
  }

  private updateRow(b: HTMLElement, r: Row): void {
    setText(q(b, '.fd-pf-title'), r.title);
    setText(q(b, '.fd-pf-detail'), r.detail);
    setText(q(b, '.fd-pf-page'), this.pageName(r.issue.page));
    setTitle(b, r.issue.message);
    setClass(b, 'is-error', r.issue.severity === 'error');
    const target = r.issue.item !== undefined && r.issue.item !== null;
    setClass(b, 'is-static', !target && (r.issue.page === undefined || r.issue.page === null));
    b.dataset.item = target ? String(r.issue.item) : '';
  }

  private markSelected(): void {
    const ids = new Set((editor.selection?.items ?? []).map(String));
    for (const b of Array.from(this.list.querySelectorAll<HTMLElement>('.fd-pf-row'))) setClass(b, 'is-selected', !!b.dataset.item && ids.has(b.dataset.item));
  }

  private async reveal(i: Issue): Promise<void> {
    if (i.item !== undefined && i.item !== null) await editor.exec('selection.set', { ids: [i.item] });
    if (i.page !== undefined && i.page !== null) this.ws.viewport.showPage(i.page);
  }

  private renderStatus(c: { errors: number; warnings: number } | null): void {
    const dot = q(this.root, '.fd-pf-dot');
    setClass(dot, 'is-ok', !!c && c.errors === 0 && c.warnings === 0);
    setClass(dot, 'is-warn', !!c && c.errors === 0 && c.warnings > 0);
    setClass(dot, 'is-error', !!c && c.errors > 0);
    setText(q(this.root, '.fd-pf-status'), !c ? 'No document' : c.errors ? `${c.errors} ${c.errors === 1 ? 'error' : 'errors'}` : 'No errors');
    setText(q(this.root, '.fd-pf-sub'), c && c.warnings ? `${c.warnings} ${c.warnings === 1 ? 'warning' : 'warnings'}` : '');
    const none = !c || c.errors + c.warnings === 0;
    setHidden(this.empty, !none);
    setHidden(this.list, none);
    setClass(this.empty, 'is-clean', !!c);
    setText(q(this.empty, '.fd-pf-empty-title'), c ? 'Ready for output' : 'Nothing to check');
    setText(q(this.empty, '.fd-pf-empty-note'), c ? `Nothing the ${this.current()?.name ?? '[Basic]'} profile checks for was found.` : 'Open or make a document and Preflight checks it as you work.');
  }

  private current(): Profile | undefined {
    return this.profiles.find((p) => p.name === this.psel.value) ?? this.profiles[0];
  }

  private async loadProfiles(want?: string): Promise<void> {
    try { this.profiles = (await api<{ profiles: Profile[] }>('/preflight/profiles')).profiles ?? []; } catch { this.profiles = []; }
    let saved = want ?? '';
    if (!saved) { try { saved = localStorage.getItem(PROFILE_KEY) ?? ''; } catch { saved = ''; } }
    this.psel.replaceChildren(...this.profiles.map((p) => new Option(p.name, p.name)));
    this.psel.value = this.profiles.some((p) => p.name === saved) ? saved : (this.profiles[0]?.name ?? '');
    setTitle(this.psel, this.current()?.note ?? '');
    if (want) { try { localStorage.setItem(PROFILE_KEY, want); } catch { } }
    this.schedule(0);
  }

  private async define(): Promise<void> {
    const base = this.current() ?? { name: '[Basic]', checks: {} };
    const d = new Dialog('Preflight Profile', 460);
    const body = el(`<div class="fo-form fd-pp">
      <label class="fo-field"><span>Name</span><input class="fd-pp-name" type="text" spellcheck="false"></label>
      <p class="fo-note fd-pp-note"></p>
      <div class="fd-pp-sec">Report</div>
      <div class="fd-pp-toggles"></div>
      <div class="fd-pp-sec">Limits</div>
      <div class="fd-pp-limits"></div>
    </div>`);
    q<HTMLInputElement>(body, '.fd-pp-name').value = base.builtin ? `${base.name.replace(/^\[|\]$/g, '')} (mine)` : base.name;
    setText(q(body, '.fd-pp-note'), base.builtin ? `${base.name} is built in; saving makes your own copy. ${base.note ?? ''}` : base.note ?? '');
    const def = (k: string): boolean | number | null => {
      if (k in base.checks) return base.checks[k];
      if (k === 'rgb') return true;
      if (k === 'minPpi') return 150;
      return ['overset', 'fonts', 'links', 'emptyFrames'].includes(k) ? true : k === 'transparency' ? false : null;
    };
    for (const [k, label] of TOGGLES) {
      const r = el(`<label class="cv-check fd-pp-t"><input type="checkbox" data-k="${k}"><span></span></label>`);
      setText(q(r, 'span'), label);
      q<HTMLInputElement>(r, 'input').checked = def(k) === true;
      q(body, '.fd-pp-toggles').append(r);
    }
    for (const [k, label, unit] of LIMITS) {
      const v = def(k);
      const r = el(`<div class="fd-pp-l"><label class="cv-check"><input type="checkbox" class="fd-pp-on" data-k="${k}"><span></span></label><input class="fd-pp-v" type="number" min="0" step="1" data-k="${k}"><span class="fd-pp-u"></span></div>`);
      setText(q(r, '.cv-check span'), label);
      setText(q(r, '.fd-pp-u'), unit);
      const on = q<HTMLInputElement>(r, '.fd-pp-on'), num = q<HTMLInputElement>(r, '.fd-pp-v');
      on.checked = typeof v === 'number';
      num.value = typeof v === 'number' ? String(v) : '';
      num.disabled = !on.checked;
      on.addEventListener('change', () => { num.disabled = !on.checked; if (on.checked && !num.value) num.value = k === 'spotMax' ? '2' : k === 'pageMultiple' ? '4' : '9'; });
      q(body, '.fd-pp-limits').append(r);
    }
    d.body.appendChild(body);
    const del = el<HTMLButtonElement>('<button class="cv-btn cv-btn-danger cv-btn-apart" type="button">Delete</button>');
    const cancel = el<HTMLButtonElement>('<button class="cv-btn" type="button">Cancel</button>');
    const save = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">Save</button>');
    setDisabled(del, !!base.builtin);
    d.foot.append(del, cancel, save);
    cancel.addEventListener('click', () => d.close());
    del.addEventListener('click', async () => {
      if (!(await confirmAction('Delete Profile', `Delete the preflight profile "${base.name}"?`, 'Delete', true))) return;
      try { await api(`/preflight/profiles?name=${encodeURIComponent(base.name)}`, { method: 'DELETE' }); d.close(); await this.loadProfiles(this.profiles[0]?.name); } catch (err) { setText(q(body, '.fd-pp-note'), (err as Error).message); }
    });
    save.addEventListener('click', async () => {
      const checks: Record<string, boolean | number | null> = {};
      for (const i of body.querySelectorAll<HTMLInputElement>('.fd-pp-toggles input')) checks[i.dataset.k as string] = i.checked;
      for (const i of body.querySelectorAll<HTMLInputElement>('.fd-pp-on')) {
        const n = Number(q<HTMLInputElement>(i.closest('.fd-pp-l') as HTMLElement, '.fd-pp-v').value);
        checks[i.dataset.k as string] = i.checked && Number.isFinite(n) ? n : null;
      }
      try {
        const out = await api<Profile>('/preflight/profiles', { json: { name: q<HTMLInputElement>(body, '.fd-pp-name').value, checks } });
        d.close();
        await this.loadProfiles(out.name);
      } catch (err) { setText(q(body, '.fd-pp-note'), (err as Error).message); }
    });
    d.onClose = () => d.scrim.remove();
    d.open(q(body, '.fd-pp-name'));
  }
}
