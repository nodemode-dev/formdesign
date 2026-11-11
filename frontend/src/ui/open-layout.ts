import { api } from '../engine/client';
import { friendly } from '../editor/editor';
import { Dialog } from './dialogs';
import { el, escapeHtml, q } from './dom';
import { toast } from './toast';

export const FOREIGN_EXTS = /\.(pdf|pub|qxd|qxp|pmd|p65|pm6|pm5|pm4|pm3|pt5|pt6|dxf|dwg|indd|indt|indb|inx)$/i;
export const FOREIGN_ACCEPT = '.pdf,.pub,.qxd,.qxp,.pmd,.p65,.pm6,.pm5,.pm4,.pm3,.pt5,.pt6,.dxf,.dwg,.indd,.indt,.indb,.inx';

interface Probe { kind: string; name: string; pages?: number | null; message?: string; steps?: string[]; help?: string }
interface Report {
  source: string; kind: string; pages: number; via?: string;
  converted: Record<string, number>; keptAsBackdrop: Record<string, number>; backdropPages: number;
  fonts: Array<{ source: string; family?: string; style?: string; exact?: boolean }>;
  substituted: Array<{ source: string; family?: string; style?: string }>; notes: string[];
}

const LABEL: Record<string, string> = { pdf: 'PDF', pub: '.pub file', qxp: '.qxd or .qxp file', pm: '.pmd or .p65 file' };

function row(label: string, value: string): string {
  return `<div class="fd-report-row"><span class="fd-report-k">${escapeHtml(label)}</span><span class="fd-report-v">${escapeHtml(value)}</span></div>`;
}

function reportDialog(r: Report): void {
  const d = new Dialog('Fidelity report', 520);
  const conv = Object.entries(r.converted);
  const kept = Object.entries(r.keptAsBackdrop);
  const parts: string[] = [`<p class="fd-note">${escapeHtml(r.source)} was rebuilt as an editable document with ${r.pages} page${r.pages === 1 ? '' : 's'}${r.via ? ` through ${escapeHtml(r.via)}` : ''}.</p>`];
  parts.push('<h4 class="fd-report-h">Converted</h4>');
  parts.push(conv.length ? conv.map(([k, v]) => row(k, String(v))).join('') : row('Nothing editable was found', ''));
  parts.push('<h4 class="fd-report-h">Kept as a backdrop</h4>');
  parts.push(kept.length ? kept.map(([k, v]) => row(k, String(v))).join('') + row('Backdrop pages', String(r.backdropPages)) : row('Nothing needed a backdrop', ''));
  if (r.substituted.length) {
    parts.push('<h4 class="fd-report-h">Fonts replaced</h4>');
    parts.push(r.substituted.map((f) => row(f.source || '(unnamed)', `${f.family ?? ''} ${f.style ?? ''}`.trim())).join(''));
  }
  if (r.notes.length) parts.push(`<h4 class="fd-report-h">Notes</h4>${r.notes.map((n) => `<p class="fd-note">${escapeHtml(n)}</p>`).join('')}`);
  d.body.append(el(`<div class="fd-report">${parts.join('')}</div>`));
  const ok = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">Done</button>');
  ok.addEventListener('click', () => d.close());
  d.foot.append(ok);
  d.onClose = () => d.scrim.remove();
  d.open(ok);
}

function inddDialog(p: Probe): void {
  const d = new Dialog('.indd files', 480);
  const steps = (p.steps ?? []).map((s) => `<li>${escapeHtml(s)}</li>`).join('');
  d.body.append(el(`<div><p class="fd-note">${escapeHtml(p.message ?? '')}</p><ol class="fd-steps">${steps}</ol>${p.help ? `<p class="fd-note"><a class="fd-link" href="${escapeHtml(p.help)}" target="_blank" rel="noopener noreferrer">How to export an IDML package</a></p>` : ''}</div>`));
  const ok = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">Close</button>');
  ok.addEventListener('click', () => d.close());
  d.foot.append(ok);
  d.onClose = () => d.scrim.remove();
  d.open(ok);
}

function chooseDialog(p: Probe): Promise<{ backdrop: string } | null> {
  const d = new Dialog(p.kind === 'pdf' ? 'Open PDF' : `Open ${LABEL[p.kind] ?? 'file'}`, 480);
  const lead = p.kind === 'pdf'
    ? `Open ${escapeHtml(p.name)} as a layout${p.pages ? ` (${p.pages} page${p.pages === 1 ? '' : 's'})` : ''}: every page becomes a page of the same size, text becomes text frames grouped into lines and paragraphs, pictures are placed at their boxes, and simple rectangles and lines become shapes.`
    : `${escapeHtml(p.name)} is converted through LibreOffice into an editable document: pages, text frames, pictures and shapes.`;
  d.body.append(el(`<div><p class="fd-note">${lead}</p><label class="fd-field"><span>Graphics that cannot be converted</span><select class="cv-search-input fd-open-backdrop"><option value="auto">Keep them as a backdrop at the back</option><option value="never">Leave them out</option></select></label></div>`));
  const sel = q<HTMLSelectElement>(d.body, '.fd-open-backdrop');
  const ok = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">Open as layout</button>');
  const cancel = el<HTMLButtonElement>('<button class="cv-btn" type="button">Cancel</button>');
  d.foot.append(cancel, ok);
  return new Promise((resolve) => {
    cancel.addEventListener('click', () => { d.close(); resolve(null); });
    ok.addEventListener('click', () => { const v = sel.value; resolve({ backdrop: v }); d.close(); });
    d.onClose = () => { d.scrim.remove(); resolve(null); };
    d.open(ok);
  });
}

export async function openForeign(file: File, refresh: () => void): Promise<void> {
  const fd = new FormData();
  fd.append('files', file, file.name);
  try {
    const up = await api<{ files: Array<{ path: string }> }>('/files', { method: 'POST', body: fd });
    const path = up.files[0]?.path ?? '';
    const probe = await api<Probe>('/documents/probe', { json: { path } });
    if (probe.kind === 'indd') { inddDialog(probe); return; }
    if (probe.kind === 'dwg') { toast(probe.message ?? '', 'error'); return; }
    const choice = probe.kind === 'dxf' ? { backdrop: 'never' } : await chooseDialog(probe);
    if (!choice) return;
    toast(`Converting ${file.name}`);
    const r = await api<{ report: Report }>('/documents/import', { json: { path, options: { backdrop: choice.backdrop } } });
    refresh();
    reportDialog(r.report);
  } catch (err) { toast(friendly((err as Error).message), 'error'); }
}
