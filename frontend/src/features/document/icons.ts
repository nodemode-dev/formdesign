const S = (body: string): string =>
  `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round">${body}</svg>`;

const PAGE = '<path d="M6.5 3h7.5l4 4v14h-11.5z"/><path d="M14 3v4h4"/>';

export const DOC_ICONS: Record<string, string> = {
  print: S('<path d="M7 9V3.5h10V9"/><rect x="3.5" y="9" width="17" height="8" rx="1.6"/><path d="M7 14h10v6.5H7z"/><path d="M17 11.8h.5"/>'),
  web: S('<rect x="3" y="4.5" width="18" height="15" rx="2"/><path d="M3 8.5h18"/><path d="M6 6.5h.5M8.2 6.5h.5"/>'),
  mobile: S('<rect x="7" y="2.8" width="10" height="18.4" rx="2.2"/><path d="M11 18h2"/>'),
  templates: S('<rect x="3.5" y="3.5" width="7.5" height="7.5" rx="1.2"/><rect x="13" y="3.5" width="7.5" height="4.5" rx="1.2"/><rect x="13" y="10" width="7.5" height="10.5" rx="1.2"/><rect x="3.5" y="13" width="7.5" height="7.5" rx="1.2"/>'),
  chain: S('<path d="M10 14a3.6 3.6 0 0 0 5.1 0l3-3a3.6 3.6 0 0 0-5.1-5.1l-1 1"/><path d="M14 10a3.6 3.6 0 0 0-5.1 0l-3 3a3.6 3.6 0 0 0 5.1 5.1l1-1"/>'),
  chainOff: S('<path d="M13.3 6.6l.7-.7a3.6 3.6 0 0 1 5.1 5.1l-2 2"/><path d="M10.7 17.4l-.7.7a3.6 3.6 0 0 1-5.1-5.1l2-2"/><path d="M4 4l16 16"/>'),
  portrait: S('<rect x="7" y="3.5" width="10" height="17" rx="1.4"/>'),
  landscape: S('<rect x="3.5" y="7" width="17" height="10" rx="1.4"/>'),
  download: S('<path d="M12 3.5v11.5M7.5 10.5 12 15l4.5-4.5"/><path d="M4.5 16.5v3h15v-3"/>'),
  folder: S('<path d="M3.5 7A1.5 1.5 0 0 1 5 5.5h4.2l2 2H19a1.5 1.5 0 0 1 1.5 1.5v8.5A1.5 1.5 0 0 1 19 19H5a1.5 1.5 0 0 1-1.5-1.5z"/>'),
  copy: S('<rect x="8.5" y="8.5" width="11.5" height="11.5" rx="2"/><path d="M15.5 8.5V6A1.5 1.5 0 0 0 14 4.5H6A1.5 1.5 0 0 0 4.5 6v8A1.5 1.5 0 0 0 6 15.5h2.5"/>'),
  check: S('<path d="M5 12.5l4.5 4.5L19 7.5"/>'),
  refresh: S('<path d="M19.5 12a7.5 7.5 0 1 1-2.2-5.3"/><path d="M19.5 4.5v4h-4"/>'),
  pdf: S(`${PAGE}<path d="M9 17v-4.5h1.4a1.3 1.3 0 0 1 0 2.6H9M13 17v-4.5h.8a2.25 2.25 0 0 1 0 4.5z"/>`),
  idml: S(`${PAGE}<path d="M10 12v5.5"/><path d="M12.6 12v5.5h.9a2.75 2.75 0 0 0 0-5.5z"/>`),
  epub: S('<path d="M5 5a1.5 1.5 0 0 1 1.5-1.5h11v13.5h-11A1.5 1.5 0 0 0 5 18.5z"/><path d="M5 18.5A1.5 1.5 0 0 0 6.5 20h11v-3"/><path d="M9 7.5h5"/>'),
  epubFixed: S('<path d="M5 5a1.5 1.5 0 0 1 1.5-1.5h11v13.5h-11A1.5 1.5 0 0 0 5 18.5z"/><path d="M5 18.5A1.5 1.5 0 0 0 6.5 20h11v-3"/><rect x="8.5" y="6.5" width="6" height="7" rx=".6"/>'),
  html: S('<path d="M8.5 7.5 4 12l4.5 4.5M15.5 7.5 20 12l-4.5 4.5M13.4 5.5l-2.8 13"/>'),
  image: S('<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><circle cx="9" cy="9.6" r="1.6"/><path d="M4 17.5l5-4.5 3.6 3 2.9-2.4 4.5 3.9"/>'),
  text: S(`${PAGE}<path d="M9.5 12h5M9.5 15h5M9.5 18h3"/>`),
  xml: S(`${PAGE}<path d="M10.6 12.2 8.6 14.6l2 2.4M13.4 12.2l2 2.4-2 2.4"/>`),
  package: S('<path d="M4 7.5 12 3.5l8 4v9l-8 4-8-4z"/><path d="M4 7.5l8 4 8-4M12 11.5v9"/><path d="M8 5.5l8 4"/>'),
  overset: S('<rect x="3.5" y="3.5" width="13" height="13" rx="1"/><path d="M7 7.5h6M7 10.5h6M7 13.5h3"/><rect x="14" y="14" width="6.5" height="6.5" rx="1"/><path d="M17.25 15.6v3.3M15.6 17.25h3.3"/>'),
  missingFont: S('<path d="M5 19 10.6 5h1.2l2.6 6.4"/><path d="M7.4 13.8h4.2"/><path d="M15 15l5 5M20 15l-5 5"/>'),
  missingLink: S('<path d="M9.5 14.5 7.7 16.3a3.2 3.2 0 0 1-4.5-4.5L5 10"/><path d="M14.5 9.5l1.8-1.8a3.2 3.2 0 0 1 4.5 4.5L19 14"/><path d="M9 4v2.5M4 9h2.5M15 20v-2.5M20 15h-2.5"/>'),
  lowResolution: S('<rect x="3.5" y="3.5" width="17" height="17" rx="1.5"/><path d="M3.5 9.2h17M3.5 14.8h17M9.2 3.5v17M14.8 3.5v17"/>'),
  emptyText: S('<rect x="3.5" y="3.5" width="17" height="17" rx="1" stroke-dasharray="2.4 2.2"/><path d="M8.5 8.5h7M12 8.5v7.5"/>'),
  rgbColor: S('<circle cx="12" cy="8.5" r="4.5"/><circle cx="8.5" cy="14.5" r="4.5"/><circle cx="15.5" cy="14.5" r="4.5"/>'),
  issue: S('<path d="M12 4 21 19.5H3z"/><path d="M12 10v4.2M12 16.8v.2"/>'),
  info: S('<circle cx="12" cy="12" r="8.5"/><path d="M12 11v5.5M12 7.8v.2"/>'),
  terminal: S('<rect x="3" y="4.5" width="18" height="15" rx="2"/><path d="M7 9.5l3 2.5-3 2.5M12.5 15H17"/>'),
  palette: S('<path d="M12 3.5a8.5 8.5 0 0 0 0 17c1.3 0 1.9-.9 1.5-2-.5-1.3.3-2.6 1.7-2.6h2.1a3.2 3.2 0 0 0 3.2-3.4C20.2 7.4 16.6 3.5 12 3.5z"/><circle cx="7.8" cy="11" r="1.1"/><circle cx="10.5" cy="7.4" r="1.1"/><circle cx="15" cy="7.6" r="1.1"/>'),
  ruler: S('<rect x="2.5" y="8" width="19" height="8" rx="1.3"/><path d="M6 8v3M9.5 8v4.5M13 8v3M16.5 8v4.5"/>'),
  globe: S('<circle cx="12" cy="12" r="8.5"/><path d="M3.5 12h17"/><path d="M12 3.5c2.4 2.3 3.6 5.1 3.6 8.5s-1.2 6.2-3.6 8.5c-2.4-2.3-3.6-5.1-3.6-8.5S9.6 5.8 12 3.5z"/>'),
  external: S('<path d="M13.5 4.5h6v6"/><path d="M19.5 4.5 11 13"/><path d="M17.5 13.5v5a1.5 1.5 0 0 1-1.5 1.5H6a1.5 1.5 0 0 1-1.5-1.5V8A1.5 1.5 0 0 1 6 6.5h5"/>'),
  remove: S('<path d="M4.5 7h15"/><path d="M9.5 7V4.5h5V7"/><path d="M6.5 7l.9 12a1.5 1.5 0 0 0 1.5 1.4h6.2a1.5 1.5 0 0 0 1.5-1.4l.9-12"/><path d="M10 11v5.5M14 11v5.5"/>'),
};

export function docIcon(name: string, size = 16): string {
  const svg = DOC_ICONS[name] ?? DOC_ICONS.info;
  return `<span class="icon" style="width:${size}px;height:${size}px" aria-hidden="true">${svg}</span>`;
}
