import { el, patchList, q, setClass, setHidden, setHtml, setText, setTitle } from '../../ui/dom';
import { icon } from '../../ui/icons';
import type { Pack, Report } from './api';
import { friendlySlot } from './store';

interface Chip { key: string; label: string; note?: string; slot?: string; template?: string; tone?: string }
interface Line { key: string; tone: 'ok' | 'warn' | 'info' | 'muted'; say: string; chips: Chip[]; list?: string[] }

const LIMIT = 10;
const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

function parseFilled(entry: string, pack: Pack | null): Chip {
  const eq = entry.indexOf('=');
  if (eq > 0) {
    const slot = entry.slice(0, eq), ref = entry.slice(eq + 1);
    const img = pack?.images?.find((i) => i.id === ref);
    return { key: slot, label: friendlySlot(slot), note: img?.name ?? ref, slot };
  }
  const m = /^(flow:[^(]+)\((.+)\)$/.exec(entry);
  if (m) return { key: m[1], label: friendlySlot(m[1]), note: m[2], slot: m[1] };
  return { key: entry, label: friendlySlot(entry), slot: entry };
}

function slotChips(list: string[] | undefined): Chip[] {
  return (list ?? []).map((s) => ({ key: s, label: friendlySlot(s), slot: s }));
}

function joinNames(names: string[], max = 3): string {
  const shown = names.slice(0, max);
  const more = names.length - shown.length;
  return more > 0 ? `${shown.join(', ')} and ${more} more` : shown.join(', ');
}

export function reportLines(r: Report, pack: Pack | null, names: Map<string, string>): Line[] {
  const out: Line[] = [];
  const filled = (r.filled ?? []).map((f) => parseFilled(f, pack));
  if (filled.length) out.push({ key: 'filled', tone: 'ok', say: `Filled ${plural(filled.length, 'slot', 'slots')} with your content`, chips: filled });
  else out.push({ key: 'filled', tone: 'warn', say: 'Nothing from your content found a slot in this template', chips: [] });
  const kept = r.kept ?? [];
  if (kept.length) out.push({ key: 'kept', tone: 'warn', say: `${plural(kept.length, 'slot still shows', 'slots still show')} sample text`, chips: slotChips(kept) });
  const ph = r.placeholders ?? [];
  if (ph.length) out.push({ key: 'placeholders', tone: 'warn', say: `${plural(ph.length, 'picture is a placeholder', 'pictures are placeholders')}`, chips: slotChips(ph) });
  const emptied = r.emptied ?? [];
  if (emptied.length) out.push({ key: 'emptied', tone: 'muted', say: `${plural(emptied.length, 'slot was', 'slots were')} left empty: nothing in your content fits ${emptied.length === 1 ? 'it' : 'them'}`, chips: slotChips(emptied) });
  const fit = Object.entries(r.fit ?? {});
  if (fit.length) {
    const chips: Chip[] = fit.map(([slot, f]) => ({
      key: slot, label: friendlySlot(slot), slot,
      note: f.trimmed ? `trimmed to ${f.trimmed}` : f.shrunk ? `type at ${Math.round(f.shrunk * 100)}%` : f.overset ? 'still overset' : f.error ? 'could not fit' : '',
      tone: f.overset || f.error ? 'warn' : undefined,
    }));
    const trimmed = fit.filter(([, f]) => f.trimmed).length, shrunk = fit.filter(([, f]) => f.shrunk).length, over = fit.filter(([, f]) => f.overset || f.error).length;
    const parts = [trimmed ? `trimmed ${trimmed}` : '', shrunk ? `shrank the type in ${shrunk}` : '', over ? `${over} still ${over === 1 ? 'runs' : 'run'} over` : ''].filter(Boolean);
    const say = parts.join(', ');
    out.push({ key: 'fit', tone: over ? 'warn' : 'info', say: `Copy too long for its frame: ${say}`, chips });
  }
  if (r.addedPages) out.push({ key: 'pages', tone: 'info', say: `Added ${plural(r.addedPages, 'page', 'pages')} so all of the body text fits`, chips: [] });
  const ui = r.unused?.images ?? [], ua = r.unused?.articles ?? [];
  if (ui.length || ua.length) {
    const imgNames = ui.map((id) => pack?.images?.find((i) => i.id === id)?.name ?? id);
    const artNames = ua.map((id) => pack?.articles?.find((a) => a.id === id)?.title || id);
    const bits = [ui.length ? `${plural(ui.length, 'image', 'images')} (${joinNames(imgNames)})` : '', ua.length ? `${plural(ua.length, 'article', 'articles')} (${joinNames(artNames)})` : ''].filter(Boolean);
    out.push({ key: 'unused', tone: 'muted', say: `Not used: ${bits.join(' and ')}`, chips: [] });
  }
  if (r.preflight) {
    const { errors, warnings } = r.preflight;
    const say = !errors && !warnings ? 'Preflight found no problems' : `Preflight: ${[errors ? plural(errors, 'error', 'errors') : '', warnings ? plural(warnings, 'warning', 'warnings') : ''].filter(Boolean).join(', ')}`;
    out.push({ key: 'preflight', tone: errors ? 'warn' : warnings ? 'info' : 'ok', say, chips: [], list: r.preflight.top?.filter(Boolean) });
  }
  if (r.warnings?.length) out.push({ key: 'warnings', tone: 'warn', say: plural(r.warnings.length, 'thing to check', 'things to check'), chips: [], list: r.warnings });
  if (r.alternatives?.length) {
    out.push({ key: 'alts', tone: 'info', say: 'Also a good fit', chips: r.alternatives.map((a) => ({ key: a.id, label: names.get(a.id) ?? a.id, template: a.id })) });
  }
  return out;
}

const TONE_ICON: Record<Line['tone'], string> = { ok: 'check', warn: 'warning', info: 'info', muted: 'remove' };

export class ReportView {
  readonly root: HTMLElement;
  private readonly lines: HTMLElement;
  private readonly expanded = new Set<string>();
  onSlot: (slot: string) => void = () => undefined;
  onTemplate: (id: string) => void = () => undefined;

  constructor() {
    this.root = el(`<div class="mk-rp">
      <div class="mk-rp-head"><span class="mk-rp-name"></span><span class="mk-rp-meta"></span></div>
      <div class="mk-rp-lines"></div>
    </div>`);
    this.lines = q(this.root, '.mk-rp-lines');
  }

  render(r: Report, pack: Pack | null, names: Map<string, string>): void {
    setText(q(this.root, '.mk-rp-name'), names.get(r.template) ?? r.template);
    const meta = [r.pages ? plural(r.pages, 'page', 'pages') : '', r.brand ? `brand ${r.brand}` : 'template colours', typeof r.seconds === 'number' ? `made in ${r.seconds < 1 ? r.seconds.toFixed(2) : r.seconds.toFixed(1)} s` : ''].filter(Boolean);
    setText(q(this.root, '.mk-rp-meta'), meta.join(' · '));
    patchList<Line>(this.lines, reportLines(r, pack, names), {
      key: (l) => l.key,
      create: (l) => {
        const n = el(`<div class="mk-rp-line"><span class="mk-rp-ico"></span><div class="mk-rp-body"><p class="mk-rp-say"></p><div class="mk-rp-chips"></div><ul class="mk-rp-list"></ul><button class="mk-rp-more" type="button" hidden></button></div></div>`);
        q(n, '.mk-rp-more').addEventListener('click', () => {
          const k = n.dataset.key ?? '';
          if (this.expanded.has(k)) this.expanded.delete(k); else this.expanded.add(k);
          this.syncMore(n);
        });
        this.updateLine(n, l);
        return n;
      },
      update: (n, l) => this.updateLine(n, l),
    });
  }

  private updateLine(n: HTMLElement, l: Line): void {
    n.dataset.tone = l.tone;
    setHtml(q(n, '.mk-rp-ico'), icon(TONE_ICON[l.tone], 12), l.tone);
    setText(q(n, '.mk-rp-say'), l.say);
    const chips = q(n, '.mk-rp-chips');
    setHidden(chips, !l.chips.length);
    patchList<Chip>(chips, l.chips, {
      key: (c) => c.key,
      create: (c) => {
        const b = el<HTMLButtonElement>('<button class="mk-chip" type="button"><span class="mk-chip-label"></span><span class="mk-chip-note"></span></button>');
        b.addEventListener('click', () => {
          if (b.dataset.template) this.onTemplate(b.dataset.template);
          else if (b.dataset.slot) this.onSlot(b.dataset.slot);
        });
        this.updateChip(b, c);
        return b;
      },
      update: (b, c) => this.updateChip(b, c),
    });
    const list = q(n, '.mk-rp-list');
    const items = (l.list ?? []).map((t, i) => ({ t, k: `${i}:${t}` }));
    setHidden(list, !items.length);
    patchList(list, items, {
      key: (x) => x.k,
      create: (x) => { const li = el('<li></li>'); setText(li, x.t); return li; },
      update: (li, x) => setText(li, x.t),
    });
    this.syncMore(n);
  }

  private updateChip(b: HTMLElement, c: Chip): void {
    setText(q(b, '.mk-chip-label'), c.label);
    const note = q(b, '.mk-chip-note');
    setText(note, c.note ?? '');
    setHidden(note, !c.note);
    if (c.slot) b.dataset.slot = c.slot; else delete b.dataset.slot;
    if (c.template) b.dataset.template = c.template; else delete b.dataset.template;
    setClass(b, 'is-warn', c.tone === 'warn');
    setClass(b, 'is-template', !!c.template);
    setTitle(b, c.template ? `Select ${c.label}` : `Show ${c.label} in Slots`);
  }

  private syncMore(n: HTMLElement): void {
    const chips = Array.from(q(n, '.mk-rp-chips').children) as HTMLElement[];
    const open = this.expanded.has(n.dataset.key ?? '');
    chips.forEach((c, i) => setHidden(c, !open && i >= LIMIT));
    const more = q<HTMLButtonElement>(n, '.mk-rp-more');
    setHidden(more, chips.length <= LIMIT);
    setText(more, open ? 'Show fewer' : `Show ${chips.length - LIMIT} more`);
  }
}
