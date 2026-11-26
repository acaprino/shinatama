// Shinatama on the shell: its identity, its screens, and nothing else. The frame, the palette, the notices, the look and the window are
// Imago's (docs/design-file.md of @acaprino/imago); the parts of every screen are Sigrid's.
import '@acaprino/sigrid/styles.css';
import { defineApp, startApp } from '@acaprino/imago';
import { Root } from './screens/shinatama.tsx';

const SHINATAMA = defineApp({
  id: 'shinatama',
  name: 'Shinatama',
  lang: 'en',
  // design-check-ignore-next-line color-value: the one colour an app gives, Shinatama's hue (the table of the design file)
  accent: '#FFA3C8',
  storage: { scheme: 'shinatama.theme', zoom: 'shinatama.zoom', sidebar: 'shinatama.sidebar' },
  // A terminal and the code view run their own keys and menu: the shell leaves them alone there (Ctrl+K, Ctrl+R, F...).
  ownKeysIn: '.xterm, .cm-editor',
});

startApp(SHINATAMA, document.getElementById('app')!, <Root />);
