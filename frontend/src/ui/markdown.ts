const INLINE = /(`[^`\n]+`)|(\*\*[^*\n]+\*\*)|(__[^_\n]+__)|(\*[^*\s][^*\n]*\*)|(_[^_\s][^_\n]*_)|(\[[^\]\n]+\]\([^)\s]+\))/g;

function inline(text: string, into: HTMLElement): void {
  let last = 0;
  for (const m of text.matchAll(INLINE)) {
    const at = m.index ?? 0;
    if (at > last) into.appendChild(document.createTextNode(text.slice(last, at)));
    const t = m[0];
    let node: HTMLElement;
    if (m[1]) { node = document.createElement('code'); node.textContent = t.slice(1, -1); }
    else if (m[2] || m[3]) { node = document.createElement('strong'); inline(t.slice(2, -2), node); }
    else if (m[4] || m[5]) { node = document.createElement('em'); inline(t.slice(1, -1), node); }
    else {
      const parts = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(t);
      node = document.createElement('span');
      node.className = 'fd-md-link';
      node.textContent = parts?.[1] ?? t;
      node.title = parts?.[2] ?? '';
    }
    into.appendChild(node);
    last = at + t.length;
  }
  if (last < text.length) into.appendChild(document.createTextNode(text.slice(last)));
}

const LIST = /^\s*(?:[-*+]|\d+[.)])\s+/;
const ORDERED = /^\s*\d+[.)]\s+/;
const ROW = /^\s*\|.*\|\s*$/;
const RULE = /^\s*\|?\s*:?-{3,}:?\s*(\|\s*:?-{3,}:?\s*)*\|?\s*$/;

function cells(line: string): string[] {
  return line.trim().replace(/^\||\|$/g, '').split('|').map((c) => c.trim());
}

export function renderMarkdown(text: string): HTMLElement[] {
  const out: HTMLElement[] = [];
  const lines = text.replace(/\r/g, '').split('\n');
  let i = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (!line.trim()) { i += 1; continue; }
    if (/^\s*```/.test(line)) {
      const pre = document.createElement('pre');
      const code = document.createElement('code');
      const body: string[] = [];
      i += 1;
      while (i < lines.length && !/^\s*```/.test(lines[i])) { body.push(lines[i]); i += 1; }
      i += 1;
      code.textContent = body.join('\n');
      pre.appendChild(code);
      out.push(pre);
      continue;
    }
    const h = /^(#{1,4})\s+(.*)$/.exec(line);
    if (h) {
      const node = document.createElement('div');
      node.className = `fd-md-h fd-md-h${Math.min(3, h[1].length)}`;
      inline(h[2], node);
      out.push(node);
      i += 1;
      continue;
    }
    if (/^\s*(-{3,}|\*{3,})\s*$/.test(line)) { out.push(document.createElement('hr')); i += 1; continue; }
    if (LIST.test(line)) {
      const list = document.createElement(ORDERED.test(line) ? 'ol' : 'ul');
      while (i < lines.length && (LIST.test(lines[i]) || (/^\s{2,}\S/.test(lines[i]) && list.lastElementChild))) {
        if (LIST.test(lines[i])) {
          const li = document.createElement('li');
          inline(lines[i].replace(LIST, ''), li);
          list.appendChild(li);
        } else if (list.lastElementChild) {
          inline(` ${lines[i].trim()}`, list.lastElementChild as HTMLElement);
        }
        i += 1;
      }
      out.push(list);
      continue;
    }
    if (ROW.test(line)) {
      const table = document.createElement('table');
      let head = true;
      while (i < lines.length && ROW.test(lines[i])) {
        if (RULE.test(lines[i])) { i += 1; continue; }
        const tr = document.createElement('tr');
        for (const c of cells(lines[i])) { const td = document.createElement(head ? 'th' : 'td'); inline(c, td); tr.appendChild(td); }
        table.appendChild(tr);
        head = false;
        i += 1;
      }
      const wrap = document.createElement('div');
      wrap.className = 'fd-md-table';
      wrap.appendChild(table);
      out.push(wrap);
      continue;
    }
    const p = document.createElement('p');
    const para: string[] = [];
    while (i < lines.length && lines[i].trim() && !LIST.test(lines[i]) && !ROW.test(lines[i]) && !/^\s*```/.test(lines[i]) && !/^#{1,4}\s/.test(lines[i])) { para.push(lines[i].trim()); i += 1; }
    inline(para.join(' '), p);
    out.push(p);
  }
  return out;
}

export class MarkdownStream {
  readonly root: HTMLElement;
  private buffer = '';
  private tail: HTMLElement | null = null;

  constructor(className = 'fd-md') {
    this.root = document.createElement('div');
    this.root.className = className;
  }

  push(text: string): void {
    this.buffer += text;
    const cut = this.boundary();
    if (cut > 0) {
      const done = this.buffer.slice(0, cut);
      this.buffer = this.buffer.slice(cut);
      for (const n of renderMarkdown(done)) this.root.insertBefore(n, this.tail);
    }
    this.showTail();
  }

  end(): void {
    for (const n of renderMarkdown(this.buffer)) this.root.insertBefore(n, this.tail);
    this.buffer = '';
    this.tail?.remove();
    this.tail = null;
  }

  private boundary(): number {
    let at = this.buffer.lastIndexOf('\n\n');
    while (at > 0) {
      const fences = (this.buffer.slice(0, at).match(/^\s*```/gm) ?? []).length;
      if (fences % 2 === 0) return at + 2;
      at = this.buffer.lastIndexOf('\n\n', at - 1);
    }
    return 0;
  }

  private showTail(): void {
    if (!this.buffer) { this.tail?.remove(); this.tail = null; return; }
    if (!this.tail) {
      this.tail = document.createElement('p');
      this.tail.className = 'fd-md-live';
      this.root.appendChild(this.tail);
    }
    this.tail.textContent = this.buffer;
  }
}
