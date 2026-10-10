import { editor } from '../editor/editor';
import { Dialog } from '../ui/dialogs';
import { el, q, setText } from '../ui/dom';
import { toast } from '../ui/toast';
import type { Workspace } from '../ui/workspace';

interface Key { name: string; level: 'character' | 'paragraph'; kind: string; choices: string[]; about: string; value: unknown }

const GROUPS: Array<[string, RegExp]> = [
  ['Basic', /^(font|fontStyle|size|leading|kerning|kern|tracking|align|leftIndent|rightIndent|firstIndent|lastIndent|spaceBefore|spaceAfter|autoLeading)$/],
  ['Advanced character', /^(hScale|vScale|baselineShift|skew|language|position|caps|noBreak|ligatures|features)$/],
  ['Colour and lines', /^(fill|fillTint|stroke|strokeWeight|underline|strike|ruleAbove|ruleBelow|shading)$/],
  ['Keeps and spans', /^(keepNext|keepTogether|keepFirst|keepLast|start|spanColumns|splitColumns|splitGutter|gridAlign)$/],
  ['Justification and hyphenation', /^(wordSpace|letterSpace|glyphScale|composer|hyphenate|hyph\w+|balance|opticalMargin|direction)$/],
  ['Drop caps, lists and nested styles', /^(drop\w+|list|bullet|numberFormat|numberStyle|listName|startAt|nested|grep|tabs)$/],
];

const HIDE = new Set(['change', 'xmlTag', 'conditions']);

function title(name: string): string {
  return name.replace(/([a-z0-9])([A-Z])/g, '$1 $2').replace(/^./, (c) => c.toUpperCase());
}

async function open(level: 'character' | 'paragraph', ws: Workspace): Promise<void> {
  const d = new Dialog(level === 'paragraph' ? 'Paragraph Options' : 'Character Options', 640);
  const head = el('<div class="fo-form"><label class="fo-field"><span>Apply to</span><select class="fto-target"></select></label></div>');
  const target = q<HTMLSelectElement>(head, '.fto-target');
  target.append(new Option(editor.wantsText ? 'The selected text' : 'The selected frames', ''));
  const styles = await editor.exec<Record<string, string[]>>('style.list', {}, true).catch(() => undefined);
  for (const s of styles?.[level] ?? []) if (!/No Paragraph Style|^\[None\]$/.test(s)) target.append(new Option(`Style: ${s.replace(/^\[(.*)\]$/, '$1')}`, s));
  const body = el('<div class="fto-body"></div>');
  d.body.append(head, body);
  let initial = new Map<string, string>();
  let readers = new Map<string, () => unknown>();
  const load = async () => {
    const params: Record<string, unknown> = { level };
    if (target.value) params.style = target.value;
    const r = await editor.exec<{ keys: Key[] }>('type.attrsCatalog', params, true).catch((e) => { toast((e as Error).message, 'error'); return undefined; });
    const keys = (r?.keys ?? []).filter((k) => !HIDE.has(k.name));
    initial = new Map();
    readers = new Map();
    body.replaceChildren();
    const sections = new Map<string, HTMLElement>();
    for (const k of keys) {
      const g = GROUPS.find(([, re]) => re.test(k.name))?.[0] ?? 'Other';
      let sec = sections.get(g);
      if (!sec) {
        sec = el('<details class="fto-sec"><summary></summary><div class="fo-form"></div></details>');
        setText(q(sec, 'summary'), g);
        if (g === 'Basic') (sec as HTMLDetailsElement).open = true;
        sections.set(g, sec);
        body.appendChild(sec);
      }
      const row = el('<label class="fo-field"><span></span></label>');
      setText(q(row, 'span'), title(k.name));
      row.title = k.about;
      const v = k.value;
      initial.set(k.name, JSON.stringify(v ?? null));
      if (k.kind === 'boolean') {
        const i = el<HTMLInputElement>('<input type="checkbox">');
        i.checked = v === true;
        row.appendChild(i);
        readers.set(k.name, () => i.checked);
      } else if (k.kind === 'choice') {
        const s = el<HTMLSelectElement>('<select></select>');
        for (const c of k.choices) s.append(new Option(title(c), c));
        s.value = typeof v === 'string' ? v : k.choices[0];
        row.appendChild(s);
        readers.set(k.name, () => s.value);
      } else if (k.kind === 'list' || k.kind === 'object' || (k.kind === 'paint' && v !== null && typeof v === 'object')) {
        const t = el<HTMLTextAreaElement>('<textarea rows="2" spellcheck="false"></textarea>');
        t.value = v == null ? '' : JSON.stringify(v);
        t.placeholder = k.about;
        row.appendChild(t);
        readers.set(k.name, () => { const s = t.value.trim(); if (!s) return null; try { return JSON.parse(s); } catch { return s; } });
      } else {
        const numeric = k.kind === 'number' || k.kind === 'integer';
        const i = el<HTMLInputElement>(`<input type="${numeric ? 'number' : 'text'}" step="${k.kind === 'integer' ? '1' : 'any'}" spellcheck="false">`);
        i.value = v == null ? '' : String(v);
        i.placeholder = k.about;
        row.appendChild(i);
        readers.set(k.name, () => {
          const s = i.value.trim();
          if (s === '') return null;
          if (numeric || (k.kind === 'leading' && s !== 'auto')) return Number(s);
          return s;
        });
      }
      q(sec, '.fo-form').appendChild(row);
    }
  };
  target.addEventListener('change', () => void load());
  await load();
  const cancel = el<HTMLButtonElement>('<button class="cv-btn" type="button">Cancel</button>');
  const ok = el<HTMLButtonElement>('<button class="cv-btn cv-btn-primary" type="button">OK</button>');
  cancel.addEventListener('click', () => d.close());
  ok.addEventListener('click', () => void (async () => {
    const changed: Record<string, unknown> = {};
    for (const [name, read] of readers) {
      const v = read();
      if (typeof v === 'number' && Number.isNaN(v)) { toast(`${title(name)} is not a number`, 'error'); return; }
      if (JSON.stringify(v ?? null) !== initial.get(name)) changed[name] = v;
    }
    d.close();
    if (!Object.keys(changed).length) return;
    const params: Record<string, unknown> = level === 'paragraph' ? { para: changed } : { char: changed };
    if (target.value) { params.style = target.value; params.kind = level; }
    await ws.runCore('type.attrs', params);
  })());
  d.foot.append(cancel, ok);
  d.onClose = () => d.scrim.remove();
  d.open(target);
}

export function installTextOptions(ws: Workspace): void {
  ws.addCommands(
    { id: 'ui.paraOptions', label: 'Paragraph Options…', enabled: () => editor.hasDoc, run: () => void open('paragraph', ws) },
    { id: 'ui.charOptions', label: 'Character Options…', enabled: () => editor.hasDoc, run: () => void open('character', ws) },
  );
  ws.addMenuItems({ menu: 'Type', items: ['ui.charOptions', 'ui.paraOptions'], at: 'start' });
}
