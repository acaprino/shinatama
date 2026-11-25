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

// pending Imago secondary-window (branch `claude/secondary-window` of acaprino/imago, on Sigrid's `claude/clear-material`): this page is not the
// app's window. Started as the main window's page, the shell asks the plugin for what only the main window has (kept_theme, kept_glass and ready are
// refused: three warnings in the console) and the page paints an opaque ground over the transparent window. With the release, the call is
//   startApp(WIDGET, document.getElementById('app')!, <Widget />, { secondary: true });
// and `capabilities/attention.json` stays as it is (the events only). See docs/desktop-ui.md, "The attention widget".
startApp(WIDGET, document.getElementById('app')!, <Widget />);
