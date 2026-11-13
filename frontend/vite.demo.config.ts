import { readFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, type Plugin } from 'vite';

const DEMO_PORT = 5397;
const PAGES: Record<string, { out: string; title: string }> = {
  magazine: { out: '../demo', title: 'FormDesign demo: A magazine feature' },
  event: { out: '../demos/event', title: 'FormDesign demo: An event poster' },
  report: { out: '../demos/report', title: 'FormDesign demo: An annual review' },
};
const SHARED = '../../demo/assets/';

const demoPage = (mode: string): Plugin => ({
  name: 'formdesign-demo-page',
  enforce: 'pre',
  transform(html, id) {
    if (!id.endsWith('/frontend/index.html')) return null;
    return html
      .replace('<script type="module" src="/src/main.ts"></script>', '<script type="module" src="../demo/src/main.ts"></script>')
      .replace(/\s*<link rel="(?:icon|apple-touch-icon|preload|manifest)"[^>]*>/g, '')
      .replace(/<html([^>]*)>/, `<html$1 data-demo="building" data-demo-id="${mode}">`)
      .replace('</head>', '  <style>html[data-demo="building"] #app { visibility: hidden !important; } html[data-demo="building"]::before { content: "Starting FormDesign"; position: fixed; left: 0; right: 0; top: calc(50% - 30px); font: 13px -apple-system, "Segoe UI", system-ui, sans-serif; color: #a4a4a4; text-align: center; } html[data-demo="building"]::after { content: ""; position: fixed; left: calc(50% - 90px); top: calc(50% - 2px); width: 180px; height: 3px; border-radius: 2px; background: linear-gradient(90deg, #f97316, #fdba74) 0 0 / 60px 3px no-repeat #222127; animation: demo-start 1.4s ease-in-out infinite alternate; } @keyframes demo-start { to { background-position: 120px 0; } }</style>\n</head>')
      .replace(/<title>[^<]*<\/title>/, `<title>${PAGES[mode].title}</title>`);
  },
});

const pageLogo = (): Plugin => ({
  name: 'formdesign-demo-logo',
  enforce: 'pre',
  transform(code, id) {
    if (!id.endsWith('/frontend/src/ui/brand.ts')) return null;
    return { code: code.replace("const LOGO = '/formdesign-logo.png';", "import LOGO from '../../public/formdesign-logo.png';"), map: null };
  },
});

const COVER = '`${import.meta.env.BASE_URL}demos/${d.id}.png`';
const covers = (mode: string): Plugin => ({
  name: 'formdesign-demo-covers',
  enforce: 'pre',
  transform(code, id) {
    if (!id.endsWith('/frontend/src/ui/demos.ts')) return null;
    if (!code.includes(COVER)) this.error('demoCover changed: update formdesign-demo-covers in vite.demo.config.ts');
    return { code: code.replace(COVER, `\`\${document.documentElement.dataset.demoId === 'magazine' ? '' : '../../demo/'}assets/covers/\${d.id}.png\``), map: null };
  },
  generateBundle() {
    if (mode !== 'magazine') return;
    for (const id of Object.keys(PAGES)) {
      this.emitFile({ type: 'asset', fileName: `assets/covers/${id}.png`, source: readFileSync(fileURLToPath(new URL(`public/demos/${id}.png`, import.meta.url))) });
    }
  },
});

const sharedBundle = (mode: string, out: string): Plugin => ({
  name: 'formdesign-demo-shared-bundle',
  enforce: 'post',
  transformIndexHtml(html) {
    return mode === 'magazine' ? html : html.replace(/"\.\/assets\/demo\.(js|css)"/g, `"${SHARED}demo.$1"`);
  },
  writeBundle(_options, bundle) {
    if (mode === 'magazine') return;
    for (const name of Object.keys(bundle)) if (name !== 'index.html') rmSync(join(out, name), { force: true });
  },
});

const symbol = (cp: number): boolean => (cp >= 0x2600 && cp <= 0x27bf) || (cp >= 0x1f000 && cp <= 0x1faff);
const unit = (n: number): string => `\\u${n.toString(16).toUpperCase().padStart(4, '0')}`;
const escapeSymbols = (): Plugin => ({
  name: 'formdesign-demo-escape-symbols',
  enforce: 'post',
  generateBundle(_options, bundle) {
    for (const file of Object.values(bundle)) {
      if (file.type !== 'chunk') continue;
      let out = '';
      for (const ch of file.code) {
        const cp = ch.codePointAt(0) ?? 0;
        out += !symbol(cp) ? ch : cp > 0xffff ? unit(0xd800 + ((cp - 0x10000) >> 10)) + unit(0xdc00 + ((cp - 0x10000) & 0x3ff)) : unit(cp);
      }
      file.code = out;
    }
  },
});

const classicScript = (): Plugin => ({
  name: 'formdesign-demo-classic-script',
  enforce: 'post',
  transformIndexHtml(html) {
    const tag = /<script type="module" crossorigin src="([^"]+)"><\/script>\s*/;
    const found = html.match(tag);
    if (!found) return html;
    return html.replace(tag, '').replace('</body>', `<script src="assets/session.js"></script>\n<script defer src="${found[1]}"></script>\n</body>`).replace(/ crossorigin(?=[ >])/g, '');
  },
});

export default defineConfig(({ mode }) => {
  const id = PAGES[mode] ? mode : 'magazine';
  const out = fileURLToPath(new URL(PAGES[id].out, import.meta.url));
  return {
    root: fileURLToPath(new URL('.', import.meta.url)),
    base: './',
    publicDir: false,
    plugins: [demoPage(id), pageLogo(), covers(id), classicScript(), sharedBundle(id, out), escapeSymbols()],
    server: { host: false, port: DEMO_PORT, strictPort: true, fs: { allow: ['..'] } },
    preview: { host: false, port: DEMO_PORT + 1, strictPort: true },
    build: {
      outDir: out,
      emptyOutDir: false,
      target: 'es2022',
      sourcemap: false,
      cssCodeSplit: false,
      assetsInlineLimit: (file: string) => file.endsWith('formdesign-logo.png'),
      modulePreload: false,
      chunkSizeWarningLimit: 20000,
      rollupOptions: {
        output: {
          format: 'iife' as const,
          inlineDynamicImports: true,
          entryFileNames: 'assets/demo.js',
          assetFileNames: (info: { names?: string[] }) => (info.names?.some((n) => n.endsWith('.css')) ? 'assets/demo.css' : 'assets/[name][extname]'),
        },
      },
    },
  };
});
