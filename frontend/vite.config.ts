import { defineConfig } from 'vite';

const FRONTEND_PORT = Number(process.env.FORMDESIGN_UI_PORT ?? 5196);
const BACKEND_PORT = Number(process.env.FORMDESIGN_ENGINE_PORT ?? 8196);

export default defineConfig({
  root: '.',
  publicDir: 'public',
  server: {
    port: FRONTEND_PORT,
    host: false,
    strictPort: true,
    proxy: {
      '/api': { target: `http://127.0.0.1:${BACKEND_PORT}`, changeOrigin: true, ws: true },
      '/mcp': { target: `http://127.0.0.1:${BACKEND_PORT}`, changeOrigin: true },
    },
  },
  preview: {
    port: FRONTEND_PORT + 1,
    host: false,
    strictPort: true,
    proxy: {
      '/api': { target: `http://127.0.0.1:${BACKEND_PORT}`, changeOrigin: true, ws: true },
    },
  },
  build: {
    outDir: 'dist',
    sourcemap: true,
    target: 'es2022',
    chunkSizeWarningLimit: 1200,
  },
});
