export function el<T extends HTMLElement = HTMLElement>(html: string): T {
  const t = document.createElement('template');
  t.innerHTML = html.trim();
  return document.adoptNode(t.content.firstElementChild as T);
}

export function q<T extends Element = HTMLElement>(root: ParentNode, sel: string): T {
  const n = root.querySelector(sel);
  if (!n) throw new Error(`missing element: ${sel}`);
  return n as T;
}

export function setText(node: Element | null, text: string): void {
  if (node && node.textContent !== text) node.textContent = text;
}

export function setClass(node: Element | null, cls: string, on: boolean): void {
  if (!node) return;
  if (node.classList.contains(cls) !== on) node.classList.toggle(cls, on);
}

export function setHidden(node: HTMLElement, hidden: boolean): void { if (node.hidden !== hidden) node.hidden = hidden; }
export function setDisabled(node: HTMLButtonElement | HTMLInputElement | HTMLSelectElement, disabled: boolean): void { if (node.disabled !== disabled) node.disabled = disabled; }

export function cssToken(name: string, fallback: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
}
