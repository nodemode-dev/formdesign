export type DemoId = 'magazine' | 'event' | 'report';

export interface DemoDef { id: DemoId; title: string; blurb: string; template: string; command: string }

export const DEMOS: DemoDef[] = [
  { id: 'magazine', title: 'A magazine feature', blurb: 'A three-page story in threaded columns with a drop cap, a pull quote and paragraph styles.', template: 'magazine-feature-spread', command: 'ui.demo.magazine' },
  { id: 'event', title: 'An event poster', blurb: 'A festival poster from a short brief: headline, date, place and a colour scheme.', template: 'print-poster-event-type', command: 'ui.demo.event' },
  { id: 'report', title: 'An annual review', blurb: 'A report with headings, a table, lists and pictures, laid out from one Markdown file.', template: 'report-annual-review', command: 'ui.demo.report' },
];

export const demoCover = (d: DemoDef): string => `/api/templates/${encodeURIComponent(d.template)}/thumb.png?size=640`;
