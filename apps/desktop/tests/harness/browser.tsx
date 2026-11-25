// A page for the browser smoke only (tests/browser.smoke.mjs): the browser tab on Imago's shell, as the tab renderer of the workspace mounts it,
// with the mock's pages. `?closed=<view>` shows that view as a tab whose page is not open. Not part of the app: nothing imports it.
import '@acaprino/sigrid/styles.css';
import { useMemo } from 'react';
import { defineApp, startApp } from '@acaprino/imago';
import { Shinatama } from '../../src/app.ts';
import { AppContext, useController } from '../../src/hooks.ts';
import { BrowserTab } from '../../src/screens/browser/index.tsx';

const HARNESS = defineApp({
  id: 'shinatama', name: 'Shinatama', lang: 'en',
  // design-check-ignore-next-line color-value: the one colour an app gives, Shinatama's hue (the table of the design file)
  accent: '#FFA3C8',
  storage: { scheme: 'shinatama.theme', zoom: 'shinatama.zoom', sidebar: 'shinatama.sidebar' },
});

function Harness() {
  const app = useMemo(() => {
    const made = new Shinatama();
    (window as unknown as { __app: Shinatama }).__app = made;
    void made.browser.start();
    return made;
  }, []);
  const browser = useController(app.browser);
  const closed = new URLSearchParams(location.search).get('closed');
  const view = closed ?? [...browser.pages.keys()].at(-1);
  return (
    <AppContext value={app}>
      <div style={{ height: '100vh' }}>{view ? <BrowserTab viewId={view} /> : null}</div>
    </AppContext>
  );
}

startApp(HARNESS, document.getElementById('app')!, <Harness />);
