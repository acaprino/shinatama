// A session's tools may start many processes at once: a Claude Code session was stopped mid-turn when `npm ci` took its job past 16
// processes. This proof starts 24 processes from a shell of the real app and requires the session to stay open and keep answering.
// Run after the coordinated desktop/helper build, with TEMP outside any Git work tree.
//   SHINATAMA_EXE=target\debug\shinatama-desktop.exe node apps/desktop/tests/native/interactive-limits.mjs
import { check, finish, fixtureFolder, hasExited, openApp, paneOf, startSession, tabOf, typeInto, until } from './lib.mjs';

const fixture = await fixtureFolder('limits');
let handle;
try {
  handle = await openApp(fixture);
  const { page, ipc } = handle;
  await (await startSession(handle, { project: fixture.project, harness: 'powershell' })).done(30000);
  const [session] = await ipc('terminal_list');
  if (!session) throw new Error('The PowerShell session did not open.');
  const shows = (text) => until(async () => (await paneOf(page, session.id).innerText()).includes(text), 60000, `The terminal never showed ${text}.`);
  await tabOf(page, session.id).click();
  await shows('PS ');
  // Markers are split in the command so its echo never matches the output.
  await typeInto(page, session.id, "$p = 1..24 | % { Start-Process -PassThru -WindowStyle Hidden ping.exe -ArgumentList '-n','6','127.0.0.1' }; "
    + "'STARTED'+'='+$p.Count; $p | Wait-Process; 'ALL'+'-DONE'", 'Enter');
  await shows('STARTED=24');
  await shows('ALL-DONE');
  await typeInto(page, session.id, "'STILL'+'-OPEN'", 'Enter');
  await shows('STILL-OPEN');
  check('the session is still one of the terminals Rust holds', (await ipc('terminal_list')).some((item) => item.id === session.id));
  check('and its process has not ended', !await hasExited(page, session.id));
  check('the page said nothing wrong', handle.errors.length === 0, handle.errors.join(' | '));
} catch (error) {
  check('the probe ran to its end', false, String(error.message ?? error));
} finally {
  await handle?.stop();
  await fixture.remove();
}
finish();
