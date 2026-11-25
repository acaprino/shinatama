// The main window's page over CDP. The app also runs the alert widget (widget.html) in a second
// WebView of the same browser, and a context's first page is whichever answered first.
import { setTimeout as delay } from 'node:timers/promises';

export async function mainPage(browser, timeout = 30000) {
  const context = browser.contexts()[0];
  const find = () => context.pages().find(page => page.url().includes('tauri.localhost') && !page.url().includes('widget.html'));
  for (const deadline = Date.now() + timeout; Date.now() < deadline; await delay(100)) {
    const page = find();
    if (page) return page;
  }
  throw new Error('The main window did not appear within ' + timeout + ' ms; pages: ' + context.pages().map(page => page.url()).join(', '));
}
