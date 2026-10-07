const S = (body: string, fill = false): string =>
  `<svg viewBox="0 0 16 16" fill="${fill ? 'currentColor' : 'none'}" stroke="${fill ? 'none' : 'currentColor'}" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;

const GLYPHS: Record<string, string> = {
  eye: S('<path d="M1.6 8S4 3.6 8 3.6 14.4 8 14.4 8 12 12.4 8 12.4 1.6 8 1.6 8z"/><circle cx="8" cy="8" r="2.1"/>'),
  eyeOff: S('<path d="M1.6 8S4 3.6 8 3.6 14.4 8 14.4 8 12 12.4 8 12.4 1.6 8 1.6 8z" opacity=".55"/><path d="M2.6 13.4 13.4 2.6"/>'),
  lock: S('<rect x="3.4" y="7" width="9.2" height="6.6" rx="1.4"/><path d="M5.4 7V5.2a2.6 2.6 0 0 1 5.2 0V7"/>'),
  unlock: S('<rect x="3.4" y="7" width="9.2" height="6.6" rx="1.4"/><path d="M5.4 7V5.2a2.6 2.6 0 0 1 5-1"/>'),
  pageAdd: S('<path d="M9.4 14.4H3.6V1.6h5.6l3.2 3.2v3.4"/><path d="M9 1.8v3.2h3.2"/><path d="M12.2 10.2v4.6M9.9 12.5h4.6"/>'),
  parentAdd: S('<path d="M2.2 11.4V2.2h6"/><path d="M4.8 4.8h6.6v4.4M4.8 4.8v9.4h4.6"/><path d="M12.4 10.4v4.4M10.2 12.6h4.4"/>'),
  trash: S('<path d="M2.8 4.4h10.4"/><path d="M6.2 4.4V2.8h3.6v1.6"/><path d="M4.2 4.4l.7 9.2h6.2l.7-9.2"/><path d="M6.8 7v4.4M9.2 7v4.4"/>'),
  spreadDup: S('<rect x="1.4" y="4.4" width="5.2" height="8.4" rx=".8"/><rect x="6.6" y="4.4" width="5.2" height="8.4" rx=".8"/><path d="M13.6 1.4v3.6M11.8 3.2h3.6"/>'),
  pen: S('<path d="M10.6 2.4 13.6 5.4 6.4 12.6 2.6 13.4 3.4 9.6z"/><path d="M9.2 3.8 12.2 6.8"/>'),
  chevron: S('<path d="M6 3.8 10.2 8 6 12.2"/>'),
  chevronDown: S('<path d="M4 6.2 8 10.2 12 6.2"/>'),
  text: S('<rect x="2" y="2" width="12" height="12" rx="1.8"/><path d="M5.2 5.4h5.6M8 5.4v5.6"/>'),
  image: S('<rect x="2" y="2.6" width="12" height="10.8" rx="1.6"/><circle cx="5.8" cy="6.2" r="1.2"/><path d="M2.4 12.2 6.2 8.6l2.4 2.2 1.8-1.6 3.2 2.8"/>'),
  rect: S('<rect x="2.4" y="3.4" width="11.2" height="9.2" rx=".6"/>'),
  ellipse: S('<ellipse cx="8" cy="8" rx="5.8" ry="4.8"/>'),
  line: S('<path d="M3 13 13 3"/>'),
  polygon: S('<path d="M8 2.2 13.8 6.4 11.6 13.2H4.4L2.2 6.4z"/>'),
  group: S('<rect x="1.8" y="1.8" width="8" height="7" rx="1.2"/><rect x="6.2" y="7.2" width="8" height="7" rx="1.2"/>'),
  frame: S('<rect x="2.4" y="2.4" width="11.2" height="11.2" rx=".6"/><path d="M2.6 2.6 13.4 13.4M13.4 2.6 2.6 13.4"/>'),
  table: S('<rect x="2" y="2.6" width="12" height="10.8" rx="1.4"/><path d="M2 6.2h12M2 9.8h12M6.4 2.6v10.8"/>'),
  object: S('<rect x="3" y="3" width="10" height="10" rx="2.4"/>'),
  modified: S('<path d="M8 1.9 14.6 13.6H1.4z"/><path d="M8 6.4v3.2M8 11.6v.1"/>'),
  missing: S('<circle cx="8" cy="8" r="6.2"/><path d="M6.3 6.2a1.8 1.8 0 1 1 2.6 1.7c-.6.3-.9.7-.9 1.3v.4M8 11.6v.1"/>'),
  embedded: S('<rect x="2.2" y="2.2" width="11.6" height="11.6" rx="1.8"/><rect x="5.4" y="5.4" width="5.2" height="5.2" rx=".6" fill="currentColor" stroke="none"/>'),
  ok: S('<path d="M3.4 8.4 6.6 11.4 12.6 4.8"/>'),
  goTo: S('<path d="M8.6 2.4h5v5"/><path d="M13.4 2.6 7.4 8.6"/><path d="M11.6 9.6v3a1 1 0 0 1-1 1H3.4a1 1 0 0 1-1-1V5.4a1 1 0 0 1 1-1h3"/>'),
  relink: S('<path d="M6.6 9.4a2.8 2.8 0 0 0 4 0l2.2-2.2a2.8 2.8 0 0 0-4-4l-.8.8"/><path d="M9.4 6.6a2.8 2.8 0 0 0-4 0L3.2 8.8a2.8 2.8 0 0 0 4 4l.8-.8"/>'),
  update: S('<path d="M13.2 8a5.2 5.2 0 1 1-1.6-3.8"/><path d="M13.2 2.4v3.4H9.8"/>'),
  pageSize: S('<rect x="2.4" y="1.6" width="8.4" height="11" rx=".8"/><path d="M9 14.4h4.6V9.8"/><path d="M13.4 14.2 10 10.8"/>'),
  layerAdd: S('<path d="M8 1.8 14.2 5 8 8.2 1.8 5z"/><path d="M1.8 8.2 6 10.4"/><path d="M11.8 9.6v4.8M9.4 12h4.8"/>'),
  layers: S('<path d="M8 1.8 14.2 5 8 8.2 1.8 5z"/><path d="M1.8 8.2 8 11.4l6.2-3.2"/><path d="M1.8 11.2 8 14.4l6.2-3.2"/>'),
};

export function glyph(name: string, size = 16): string {
  return `<span class="icon" style="width:${size}px;height:${size}px" aria-hidden="true">${GLYPHS[name] ?? GLYPHS.object}</span>`;
}

export function kindGlyph(kind: string, hasChildren: boolean): string {
  const k = kind.replace(/[<>]/g, '').toLowerCase();
  if (hasChildren || k.includes('group')) return 'group';
  if (k.includes('text')) return 'text';
  if (k.includes('image') || k.includes('graphic') || k.includes('pdf')) return 'image';
  if (k.includes('ellipse') || k.includes('oval')) return 'ellipse';
  if (k.includes('line') || k.includes('path')) return 'line';
  if (k.includes('polygon') || k.includes('star')) return 'polygon';
  if (k.includes('table')) return 'table';
  if (k.includes('frame')) return 'frame';
  if (k.includes('rect')) return 'rect';
  return 'object';
}
