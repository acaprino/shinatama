import { defineConfig } from 'vite';

const here = new URL('.', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
const page = (name: string) => new URL(name, import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');

// Two pages: the window (index.html) and the attention widget, the small window that floats over the desktop (widget.html).
export default defineConfig({
  root: here,
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1420, strictPort: true },
  build: {
    target: 'es2022',
    outDir: 'dist',
    emptyOutDir: true,
    chunkSizeWarningLimit: 2500,
    rollupOptions: { input: { main: page('./index.html'), widget: page('./widget.html') } },
  },
});
