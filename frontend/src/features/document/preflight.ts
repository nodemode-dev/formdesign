import { editor } from '../../editor/editor';
import type { ChangeEvent } from '../../engine/client';
import { el, patchList, q, setClass, setHidden, setText, setTitle } from '../../ui/dom';
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
};

function kindInfo(kind: string): { label: string; icon: string; order: number } {
  return KINDS[kind] ?? { label: kind.replace(/([a-z])([A-Z])/g, '$1 $2').replace(/^./, (c) => c.toUpperCase()), icon: 'issue', order: 50 };
}

export class PreflightPanel {
  readonly root = el(`<div class="fd-pf">
    <div class="fd-pf-head"><span class="fd-pf-dot"></span><span class="fd-pf-status"></span><span class="fd-pf-sub"></span><button class="cv-icon-btn fd-pf-run" type="button" title="Check again" aria-label="Check again">${docIcon('refresh', 15)}</button></div>
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

  constructor(private readonly ws: Workspace) {
    q(this.root, '.fd-pf-run').addEventListener('click', () => void this.run());
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
      const [rep, ins] = await Promise.all([editor.exec<Report>('preflight.run', {}, true), editor.inspect()]);
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
      case 'lowResolution': {
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
    setText(q(this.empty, '.fd-pf-empty-note'), c ? 'No overset text, missing fonts or links, low resolution images or RGB in print.' : 'Open or make a document and Preflight checks it as you work.');
  }
}
