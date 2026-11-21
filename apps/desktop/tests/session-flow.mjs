// New session as a user starts one (ADR-0039 v2, FRD-0001 R68): the control opens a menu of
// harnesses for the selected project and one choice starts the session. Shared by the smoke tests.
import { setTimeout as delay } from 'node:timers/promises';

const key = path => String(path ?? '').replace(/^\\\\\?\\/, '').replaceAll('/', '\\').replace(/\\+$/, '').toLowerCase();
async function until(read, milliseconds, message) {
  for (const deadline = Date.now() + milliseconds; ;) {
    const value = await read(); if (value) return value;
    if (Date.now() >= deadline) throw new Error(message);
    await delay(100);
  }
}

/** Presses `trigger` (default: the toolbar's New session) and starts `harness` in `project`.
 * - No project selected: the project finder opens instead; the project is chosen there, through
 *   Project roots… and its single-folder field, and the control is pressed again.
 * - `project` is the selected project: the harness is picked from the menu (route 'menu').
 * - `project` is another folder: Other folder… and its sheet are used (route 'folder').
 * Returns once the choice is made, without waiting for the session: poll `pending()` and
 * `refusal()`, or await `done()`. */
export async function startSession(page, { project, harness, trigger = page.locator('#new-terminal'), timeout = 15000 }) {
  const menu = page.locator('#session-menu'); const finder = page.locator('#project-menu'); const sheet = page.locator('#new-session-dialog');
  const opened = () => until(async () => await menu.isVisible() ? 'menu' : await finder.isVisible() ? 'finder' : null, timeout,
    'New session opened neither the harness menu nor the project finder.');
  await trigger.click();
  if (await opened() === 'finder') {
    await finder.getByRole('button', { name: 'Project roots…', exact: true }).click();
    await page.locator('#folder').fill(project);
    await page.getByRole('button', { name: 'Use folder', exact: true }).click();
    await until(async () => !await page.locator('#project-dialog').isVisible(), timeout, 'The project folder was not accepted.');
    await trigger.click();
    if (await opened() !== 'menu') throw new Error('New session still asks for a project after one was chosen.');
  }
  let route = 'menu';
  if (key(await menu.getAttribute('data-project-path')) === key(project)) await menu.locator(`[data-harness="${harness}"]`).click();
  else {
    route = 'folder';
    await menu.locator('.session-menu-other').click();
    await until(() => sheet.isVisible(), timeout, 'Other folder… did not open the folder sheet.');
    await sheet.locator('#new-session-project').fill(project);
    await sheet.locator('#new-session-harness').selectOption(harness, { force: true });
    await sheet.getByRole('button', { name: 'Open', exact: true }).click();
  }
  const starting = page.locator('#new-terminal[aria-busy="true"]');
  const inline = sheet.locator('#new-session-error');
  const flow = {
    route,
    /** True while the session is still being opened. */
    pending: async () => await starting.count() > 0 || (await sheet.isVisible() && !await inline.isVisible()),
    /** The product's refusal text, or null: the sheet's inline error, or the error notice of the menu route. */
    refusal: async () => {
      if (await sheet.isVisible() && await inline.isVisible()) return (await inline.innerText()).replace(/[\r\n]+/g, ' ').trim();
      const notice = page.locator('#notice[data-tone="error"]');
      return await notice.isVisible() ? (await notice.locator('span').first().innerText()).replace(/[\r\n]+/g, ' ').trim() : null;
    },
    /** Waits for the open to finish; throws with the product's text if it was refused. */
    done: milliseconds => until(async () => {
      const refused = await flow.refusal(); if (refused) throw new Error(`New session was refused: ${refused.slice(0, 300)}`);
      return !await flow.pending();
    }, milliseconds ?? 90000, 'New session did not finish opening before its deadline.'),
  };
  return flow;
}

/** Drags a tab with the mouse, as a person does, and drops it on `where` of `pane`'s body
 * ('left' | 'right' | 'top' | 'bottom' split the pane there, 'center' joins its tabs).
 * Waits for the drop preview first, so a refused drop fails here instead of later. */
export async function dragTab(page, tab, pane, where) {
  const from = await tab.boundingBox(); const body = await pane.locator('.wm-body').boundingBox();
  if (!from || !body) throw new Error('The tab or the target pane is not on screen.');
  const fraction = { left: [0.1, 0.5], right: [0.9, 0.5], top: [0.5, 0.1], bottom: [0.5, 0.9], center: [0.5, 0.5] }[where];
  if (!fraction) throw new Error('Unknown drop side: ' + where);
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2); await page.mouse.down();
  await page.mouse.move(body.x + body.width * fraction[0], body.y + body.height * fraction[1], { steps: 12 });
  try { await until(() => page.locator('.wm-drop-preview').isVisible(), 5000, 'Dragging the tab showed no drop target.'); }
  catch (failure) {
    // Whether the gesture started, the dragged tab survived a re-render, or input was held by a layer.
    const state = await tab.evaluate(node => ({
      connected: node.isConnected, dragging: node.classList.contains('wm-tab-dragging'),
      hostDragging: !!document.querySelector('.wm-dragging'),
      openModals: [...document.querySelectorAll('dialog[open][aria-modal="true"]')].map(dialog => dialog.id),
      openDialogs: [...document.querySelectorAll('dialog[open]')].map(dialog => dialog.id || dialog.className),
      // What the press landed on: the tab itself, a child, or something drawn over it.
      underPress: (() => {
        const box = node.getBoundingClientRect();
        const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
        return { tag: hit?.tagName, classes: hit?.className?.toString().slice(0, 80), insideTab: !!hit && node.contains(hit) };
      })(),
      tabPointerEvents: getComputedStyle(node).pointerEvents,
      splitters: [...document.querySelectorAll('.wm-splitter')].slice(0, 6).map(splitter => {
        const box = splitter.getBoundingClientRect(); return [Math.round(box.left), Math.round(box.top), Math.round(box.width), Math.round(box.height)];
      }),
    })).catch(error => ({ tabGone: String(error).slice(0, 200) }));
    await page.mouse.up();
    throw new Error(failure.message + ' ' + JSON.stringify(state));
  }
  await page.mouse.up();
}

/** Lists a project's threads in the sidebar through the Project menu's tick, unless they already are. */
export async function showProject(page, projectId) {
  const group = page.locator(`.thread-sidebar-group[data-project-id="${projectId}"]`);
  if (!await group.count()) {
    await page.locator('#project').click();
    await page.locator(`#project-menu [data-im-mark="${projectId}"]`).click();
    await page.locator('#project').click();
  }
  await until(() => group.isVisible(), 10000, 'The project did not appear in the thread sidebar.');
}
