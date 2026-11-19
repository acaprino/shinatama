import { defineConfig } from 'vite';

export default defineConfig({
  root: new URL('.', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'),
  clearScreen: false,
  build: {
    target: 'es2022',
    outDir: '../../../../.prototype-evidence/tl1-dist',
    emptyOutDir: true,
  },
});
