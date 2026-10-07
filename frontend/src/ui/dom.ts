// stylized-nodemode · dom-stable.ts · v1.0.0
// The five primitives behind the zero-flash law, plus the boot reveal.
// Import these instead of writing innerHTML into a live container.
//
//   delayedIndicator  no spinner for the first 300 ms; once shown, hold 250 ms
//   patchList         keyed reconcile: update, insert, remove; never clear-and-refill
//   swapMedia         decode the new image off-screen, swap on load, old frame stays
//   setText           write textContent only when it changed (no layout churn)
//   reserve           set a box's dimensions before async content arrives
//   revealApp         flip body.app-ready once, after first content is in place
//
// Every helper is framework-free and works against the string-template
// rendering NodeMode uses: render a fresh container ONCE, then patch it.

export type Disposer = () => void;

/** Show an indicator only if the wait outlasts `delay`; once visible keep it
 * for at least `minVisible` so it cannot blink. Returns the dismiss function.
 * The app's own numbers: 300 / 250. */
export function delayedIndicator(show: () => void, hide: () => void, delay = 300, minVisible = 250): Disposer {
  let timer: number | undefined;
  let shownAt = 0;
  timer = window.setTimeout(() => { shownAt = performance.now(); show(); }, delay);
  return () => {
    if (timer !== undefined && shownAt === 0) { clearTimeout(timer); timer = undefined; return; }
    const remaining = Math.max(0, minVisible - (performance.now() - shownAt));
    window.setTimeout(hide, remaining);
  };
}

/** Convenience over delayedIndicator for an element that carries .ns-pending:
 * toggles .is-shown so the element keeps its reserved box while hidden. */
export function pendingClass(el: HTMLElement, delay = 300, minVisible = 250): Disposer {
  return delayedIndicator(() => el.classList.add('is-shown'), () => el.classList.remove('is-shown'), delay, minVisible);
}

export interface PatchListOptions<T> {
  /** stable identity for each item; never an array index */
  key: (item: T) => string;
  /** build a NEW node for an item that does not exist yet */
  create: (item: T) => HTMLElement;
  /** update an EXISTING node in place; touch only what changed */
  update: (node: HTMLElement, item: T) => void;
}

/** Reconcile `container`'s children against `items` by key. Existing nodes are
 * updated in place (scroll, focus, selection, in-progress input survive);
 * missing nodes are created; extra nodes are removed; order is corrected with
 * insertBefore only where it actually differs. */
export function patchList<T>(container: HTMLElement, items: T[], opts: PatchListOptions<T>): void {
  const byKey = new Map<string, HTMLElement>();
  for (const child of Array.from(container.children)) {
    const k = (child as HTMLElement).dataset.key;
    if (k !== undefined) byKey.set(k, child as HTMLElement);
  }
  let cursor: Element | null = container.firstElementChild;
  const seen = new Set<string>();
  for (const item of items) {
    const k = opts.key(item);
    seen.add(k);
    let node = byKey.get(k);
    if (node) {
      opts.update(node, item);
    } else {
      node = opts.create(item);
      node.dataset.key = k;
    }
    if (node !== cursor) container.insertBefore(node, cursor);
    else cursor = cursor.nextElementSibling;
  }
  for (const [k, node] of byKey) if (!seen.has(k)) node.remove();
}

/** Replace an image's source without a blank frame: decode off-screen, then
 * swap src in one write. The old picture stays visible until the new one can
 * paint. Resolves when the swap happened, rejects on load failure. */
export function swapMedia(img: HTMLImageElement, nextSrc: string): Promise<void> {
  if (img.src === nextSrc) return Promise.resolve();
  return new Promise((resolve, reject) => {
    const probe = new Image();
    probe.decoding = 'async';
    probe.onload = () => {
      if (typeof probe.decode === 'function') {
        probe.decode().catch(() => undefined).then(() => { img.src = nextSrc; resolve(); });
      } else { img.src = nextSrc; resolve(); }
    };
    probe.onerror = () => reject(new Error('media load failed: ' + nextSrc));
    probe.src = nextSrc;
  });
}

/** Write text only when it differs. Numeric readouts should also carry
 * .ns-tabular and a reserved width so neighbours never shift. */
export function setText(el: Element | null, text: string): void {
  if (el && el.textContent !== text) el.textContent = text;
}

/** Toggle a class only when the state differs (no attribute churn). */
export function setClass(el: Element | null, cls: string, on: boolean): void {
  if (!el) return;
  if (el.classList.contains(cls) !== on) el.classList.toggle(cls, on);
}

/** Reserve a box before content lands. Pass width/height in px or an
 * aspect ratio string like '16 / 9'. */
export function reserve(el: HTMLElement, box: { width?: number; height?: number; minHeight?: number; aspect?: string }): void {
  if (box.width !== undefined) el.style.width = box.width + 'px';
  if (box.height !== undefined) el.style.height = box.height + 'px';
  if (box.minHeight !== undefined) el.style.minHeight = box.minHeight + 'px';
  if (box.aspect) el.style.aspectRatio = box.aspect;
}

/** The app's boot reveal: #app is visibility:hidden from the critical inline
 * style; call this exactly once after the first render is in the DOM. Two
 * frames so fonts and layout settle before the single reveal. */
export function revealApp(): void {
  if (document.body.classList.contains('app-ready')) return;
  requestAnimationFrame(() => requestAnimationFrame(() => document.body.classList.add('app-ready')));
}

/** Mount a fresh subtree from a template string ONCE into an empty host.
 * Refuses to overwrite a host that already has rendered children, which is
 * the exact mistake the zero-flash law forbids; patch it instead. */
export function mountOnce(host: HTMLElement, html: string): void {
  if (host.childElementCount > 0) {
    throw new Error('mountOnce: host already rendered; patch in place instead of rebuilding');
  }
  host.innerHTML = html; // fresh
}

export function el<T extends HTMLElement = HTMLElement>(html: string): T {
  const t = document.createElement('template');
  t.innerHTML = html.trim();
  return t.content.firstElementChild as T;
}

export function q<T extends Element = HTMLElement>(root: ParentNode, sel: string): T {
  const n = root.querySelector(sel);
  if (!n) throw new Error(`missing element: ${sel}`);
  return n as T;
}

export function setHidden(node: HTMLElement, hidden: boolean): void { if (node.hidden !== hidden) node.hidden = hidden; }
export function setDisabled(node: HTMLButtonElement | HTMLInputElement | HTMLSelectElement, disabled: boolean): void { if (node.disabled !== disabled) node.disabled = disabled; }
export function setTitle(node: HTMLElement, title: string): void { if (node.title !== title) node.title = title; }
export function setStyleVar(node: HTMLElement, name: string, value: string): void { if (node.style.getPropertyValue(name) !== value) node.style.setProperty(name, value); }
export function setValue(node: HTMLInputElement | HTMLSelectElement, value: string): void { if (node.value !== value && document.activeElement !== node) node.value = value; }
export function setChecked(node: HTMLInputElement, on: boolean): void { if (node.checked !== on) node.checked = on; }
export function setHtml(node: Element, html: string, key: string): void {
  const n = node as HTMLElement;
  if (n.dataset.htmlKey === key) return;
  n.dataset.htmlKey = key;
  const t = document.createElement('template');
  t.innerHTML = html;
  n.replaceChildren(t.content);
}
export function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c] ?? c);
}
export const isMac = /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent);
