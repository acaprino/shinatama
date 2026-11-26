// The entry of the attention widget (widget.html): its own page and its own window, on the same Imago look as the main window.
import '@acaprino/sigrid/styles.css';
import { defineApp, startApp } from '@acaprino/imago';
import { Widget } from './index.tsx';

const WIDGET = defineApp({
  id: 'shinatama',
  name: 'Shinatama',
  lang: 'it',
  // design-check-ignore-next-line color-value: the one colour an app gives, Shinatama's hue (the table of the design file)
  accent: '#FFA3C8',
  storage: { scheme: 'shinatama.theme', zoom: 'shinatama.widget.zoom', sidebar: 'shinatama.widget.sidebar' },
});

// This page is not the app's window: `secondary` keeps the shell from asking the plugin for what only the main window has, and draws the
// transparent window's ground. `capabilities/attention.json` stays as it is (the events only). See docs/desktop-ui.md, "The attention widget".
startApp(WIDGET, document.getElementById('app')!, <Widget />, { secondary: true });
