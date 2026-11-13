import './links.css';

const PAGES: ReadonlyArray<{ id: string; label: string; path: string }> = [
  { id: 'magazine', label: 'Magazine', path: 'demo/index.html' },
  { id: 'event', label: 'Poster', path: 'demos/event/index.html' },
  { id: 'report', label: 'Report', path: 'demos/report/index.html' },
];

export function mountLinks(current: string): void {
  const up = current === 'magazine' ? '../' : '../../';
  const nav = document.createElement('nav');
  nav.className = 'fd-demo-links';
  nav.setAttribute('aria-label', 'FormDesign demos');
  const head = document.createElement('span');
  head.textContent = 'Demos';
  nav.append(head);
  for (const p of PAGES) {
    const a = document.createElement('a');
    a.href = `${up}${p.path}`;
    a.textContent = p.label;
    if (p.id === current) a.setAttribute('aria-current', 'page');
    nav.append(a);
  }
  const bar = document.querySelector('.fd-status');
  if (bar) bar.insertBefore(nav, bar.querySelector('.fd-st-version'));
}
