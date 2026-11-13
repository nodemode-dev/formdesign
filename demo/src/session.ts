import { mountLinks } from './links';
import { tileWork } from './shim/api';

const wait = (ms: number): Promise<void> => new Promise((done) => window.setTimeout(done, ms));
const frame = (): Promise<void> => new Promise((done) => requestAnimationFrame(() => done()));

async function card(id: string): Promise<HTMLElement> {
  for (let i = 0; i < 400; i++) {
    const b = document.querySelector<HTMLElement>(`.cv-home-demo[data-demo="${id}"]`);
    if (b && document.body.classList.contains('app-ready') && document.querySelector('.fd-st-engine')?.textContent) return b;
    await wait(50);
  }
  throw new Error('The demo window did not start.');
}

async function painted(): Promise<void> {
  for (let i = 0; i < 200; i++) {
    const idle = tileWork.done > 0 && tileWork.busy === 0 && performance.now() - tileWork.at > 300;
    if (idle && !document.querySelector('.cv-home-demo[aria-busy="true"]') && document.querySelector('.cv-tab-strip [role="tab"]')) break;
    await wait(50);
  }
}

export async function openSession(id: string): Promise<void> {
  try {
    const open = await card(id);
    mountLinks(id);
    open.click();
    await painted();
  } finally {
    await frame();
    await frame();
    document.documentElement.removeAttribute('data-demo');
  }
}
