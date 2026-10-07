const S = (body: string): string =>
  `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;

const sheet = (mark: string): string => S(`<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4"/>${mark}`);

const GLYPHS: Record<string, string> = {
  text: sheet('<path d="M9 11h6M9 14h6M9 17h4"/>'),
  markdown: sheet('<path d="M8.5 17v-5l2 2.4 2-2.4v5"/><path d="M15.5 12v5M14 15.5l1.5 1.5 1.5-1.5"/>'),
  doc: sheet('<path d="M8.5 11.5l1.2 5.5 1.6-4 1.6 4 1.2-5.5"/>'),
  pdf: sheet('<path d="M9 17v-5h1.4a1.3 1.3 0 0 1 0 2.6H9"/><path d="M13.4 17v-5h.8a2 2 0 0 1 0 5z"/>'),
  table: sheet('<rect x="8.5" y="11" width="7" height="6.5" rx=".5"/><path d="M8.5 14.2h7M12 11v6.5"/>'),
  web: sheet('<path d="M10 12.5 8.5 14.5 10 16.5M14 12.5l1.5 2-1.5 2"/>'),
  image: S('<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><circle cx="9" cy="10" r="1.6"/><path d="M20.5 16l-5-5-8.5 8.5"/>'),
  gif: S('<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><path d="M10 10.5 14 12l-4 1.5z" fill="currentColor"/><path d="M3.5 8h17M3.5 16h17"/>'),
  file: sheet(''),
  drop: S('<path d="M12 15V4.5M8 8.5l4-4 4 4"/><path d="M4.5 14.5v3a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2v-3"/>'),
  stack: S('<rect x="7" y="3.5" width="13" height="13" rx="1.6"/><path d="M4 7.5v11a2 2 0 0 0 2 2h11"/>'),
  textSlot: S('<path d="M6 6.5h12M12 6.5V18M9.5 18h5"/>'),
  imageSlot: S('<rect x="4" y="5" width="16" height="14" rx="1.5"/><path d="M4 15l4.5-4 3.5 3 2.5-2 5.5 4.5"/><circle cx="15.5" cy="9" r="1.3"/>'),
  flowSlot: S('<path d="M5 6h7M5 9.5h7M5 13h7"/><path d="M14.5 6h4.5v13h-7v-3"/><path d="M12 13.5 13.6 16 12 18.5"/>'),
  pick: S('<rect x="3.5" y="3.5" width="7" height="9" rx="1"/><rect x="13.5" y="3.5" width="7" height="5" rx="1"/><rect x="13.5" y="11.5" width="7" height="9" rx="1"/><rect x="3.5" y="15.5" width="7" height="5" rx="1"/>'),
  arrowRight: S('<path d="M5 12h13M13 7l5 5-5 5"/>'),
  palette: S('<path d="M12 3.5a8.5 8.5 0 1 0 0 17c1.3 0 1.8-.9 1.4-1.9-.5-1.2.2-2.4 1.6-2.4h1.8c2 0 3.7-1.6 3.7-3.7C20.5 7.6 16.7 3.5 12 3.5z"/><circle cx="7.8" cy="11" r="1.1" fill="currentColor"/><circle cx="10.5" cy="7.3" r="1.1" fill="currentColor"/><circle cx="15" cy="7.8" r="1.1" fill="currentColor"/>'),
};

export function glyph(name: string, size = 14): string {
  const svg = GLYPHS[name] ?? GLYPHS.file;
  return svg.replace('<svg ', `<svg width="${size}" height="${size}" aria-hidden="true" `);
}

export function fileKind(name: string): string {
  const ext = (name.split('.').pop() ?? '').toLowerCase();
  if (ext === 'md' || ext === 'markdown') return 'markdown';
  if (ext === 'txt' || ext === 'rtf') return 'text';
  if (ext === 'docx' || ext === 'doc' || ext === 'odt') return 'doc';
  if (ext === 'pdf') return 'pdf';
  if (ext === 'csv' || ext === 'xlsx' || ext === 'xls' || ext === 'tsv') return 'table';
  if (ext === 'html' || ext === 'htm') return 'web';
  if (ext === 'gif') return 'gif';
  if (['png', 'jpg', 'jpeg', 'webp', 'tif', 'tiff', 'svg', 'bmp', 'heic', 'avif'].includes(ext)) return 'image';
  return 'file';
}
