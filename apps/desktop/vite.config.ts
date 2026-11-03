import { defineConfig } from 'vite';
export default defineConfig({
  root: new URL('.', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'),
  clearScreen: false,
  server: { host: '127.0.0.1', port: 1420, strictPort: true },
  build: { target: 'es2022', rollupOptions: { input: {
    main: new URL('./index.html', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'),
    widget: new URL('./widget.html', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1'),
  } } },
});
