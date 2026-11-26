// The tab of a session: Sigrid's `Terminal` wired to the terminal the controller holds, the strip of its lines in miniature, a foot that
// says what runs and how much it uses, and, for a tab whose session does not run, the card that says why and how to bring it back.
// What a terminal does (the pseudo-terminal, its output, its end) is `app.terminals`; what a thread is, is `app.threads`; this file is the
// screen that draws them and the keys and the paste that reach the program through it.
import { useEffect, useMemo, useRef, useState } from 'react';
import { useGlass } from '@acaprino/imago';
import { Empty } from '@acaprino/sigrid/components';
import { Badge, Button, Cluster, Column, Cell, Icon, Text } from '@acaprino/sigrid/primitives';
import { OverviewStrip, Terminal, textExtent, type TerminalBufferView, type TerminalHandle } from '@acaprino/sigrid/terminal';
import type { WorkspaceTabState } from '@acaprino/sigrid/assemblies';
import { useApp, useController } from '../../hooks.ts';
import { terminalLeavesKey, commandOfKey } from '../../keys.ts';
import { openCard } from '../../session-opens.ts';
import { openStep, savedCardText } from '../../session-tabs.ts';
import { readerOf } from '../../terminal-input.ts';
import { createTerminalHistory, LIVE_SCROLLBACK, type TerminalHistory } from '../../terminal-history.ts';
import { classifyTarget, findLinks, hoverText } from '../../terminal-links.ts';
import { formatTerminalResources } from '../../terminal-resources.ts';
import type { TerminalSession } from '../../controllers/terminals.ts';
import { runners } from '../commands.tsx';
import { forgetHandle, rememberHandle } from './handles.ts';
import { TerminalHistoryView } from './terminal-history.tsx';
import { acceptDrops, attachTerminalPaste } from './terminal-io.ts';

/** How often the strip and the foot follow the output: the parser can write many times a second, the eye needs a few. */
const REDRAW_MS = 150;

/** A render now and then while the buffer changes. */
function useTick(): [number, () => void] {
  const [tick, setTick] = useState(0);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  return [tick, () => { if (timer.current === undefined) timer.current = setTimeout(() => { timer.current = undefined; setTick(value => value + 1); }, REDRAW_MS); }];
}

export function TerminalTab({ id, state }: { id: string; state: WorkspaceTabState }) {
  const app = useApp();
  const terminals = useController(app.terminals);
  useController(app.threads);
  const session = terminals.get(id);
  return session ? <LiveTerminal session={session} state={state} /> : <SavedSession id={id} />;
}

function LiveTerminal({ session, state }: { session: TerminalSession; state: WorkspaceTabState }) {
  const app = useApp();
  const { id, cwd, shell } = session.info;
  const ui = useController(app.ui);
  const terminals = useController(app.terminals);
  const settings = useController(app.settings);
  useController(app.threads);
  useController(app.chat);
  const [glass] = useGlass();
  const handle = useRef<TerminalHandle>(null);
  const history = useRef<TerminalHistory | null>(null);
  const [hover, setHover] = useState<string | null>(null);
  const [historyAt, setHistoryAt] = useState<number | null>(null);
  const [revision, bump] = useTick();
  const modal = ui.dialog !== null;
  // The Workspace stays mounted behind Sessions and Settings: a terminal there takes no keyboard.
  const shown = ui.view === 'workspace';
  const open = historyAt !== null;
  /** Whether the history is over the terminal: what the paste and the drop read, which are made once. */
  const historyOpen = useRef(false);
  historyOpen.current = open;

  // The terminal is made by the part; here it is wired: its output (written, then copied into the archive once parsed), the program's
  // replies to what xterm sends, the paste and the drop of files, and the reader the hop uses to see what is printed.
  useEffect(() => {
    const part = handle.current;
    const xterm = part?.terminal;
    if (!part || !xterm) return;
    const archive = createTerminalHistory(xterm);
    history.current = archive;
    // No clipboard or image addon: escape text never reaches HTML. Bounded native attention controls are observed by Rust before
    // rendering, and the window's title and cursor reports are not the program's to change.
    for (const osc of [0, 1, 2, 7, 9, 52, 777]) xterm.parser.registerOscHandler(osc, () => true);
    xterm.parser.registerCsiHandler({ final: 't' }, () => true);
    const stopOutput = terminals.connect(id, { write: (data, done) => part.write(data, () => { archive.capture(); done(); }) });
    terminals.setReader(id, () => { const view = part.buffer; return Array.from({ length: view.length }, (_, index) => view.line(index)).join('\n'); });
    rememberHandle(id, part, archive);
    const input = {
      element: xterm.element as HTMLElement, reader: readerOf(shell),
      accepts: () => !session.ended && !session.disposed && !app.ui.modalOpen() && historyOpen.current === false,
      paste: (text: string) => part.paste(text), focus: () => { app.layout.focus(app.layout.groupOf(id)?.id ?? app.layout.focusedGroup()); part.focus(); },
    };
    const stopPaste = attachTerminalPaste(input, (message, error) => app.ui[error ? 'error' : 'info'](typeof message === 'string' ? message : message instanceof Error ? message.message : JSON.stringify(message)));
    const stopDrops = acceptDrops(input);
    const stopArchive = archive.onChange(bump);
    return () => {
      stopPaste(); stopDrops(); stopArchive(); stopOutput();
      terminals.setReader(id, null);
      forgetHandle(id);
      archive.dispose();
      history.current = null;
    };
    // The terminal lives as long as the tab does; its session is the same object all that time.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [id]);


  // The pane is shown (a tab chosen, a pane no longer hidden behind a maximized one): the terminal fits its box; the pane the person
  // works in gives it the keyboard, unless a dialog has the page. A dialog that closes gives it back.
  useEffect(() => {
    if (!state.visible) return;
    handle.current?.fit();
    if (state.focused && state.active && shown && !modal && !open) handle.current?.focus();
  }, [state.visible, state.focused, state.active, shown, modal, open]);

  const resources = terminals.metrics.get(id);
  const measured = resources ? formatTerminalResources(resources) : null;
  const harness = app.threads.harnessLabel(shell);
  const buffer: TerminalBufferView | null = handle.current?.buffer ?? null;
  const archive = history.current;
  const archived = archive?.archive.lines() ?? 0;
  const atTop = !!buffer && archived > 0 && buffer.viewportY === 0 && buffer.length > buffer.rows;
  void revision;
  // The lines that hold what the person searches (Find in terminal) are ticks across the strip while the search is open.
  const found = terminals.finding === id ? terminals.findHits : null;
  const marks = useMemo(() => found?.map(line => ({ line })), [found]);

  const ended = session.ended;
  return (
    <Column fill gap="xs">
      <Cluster fill align="stretch" wrap={false} gap="xs">
        <Cell grow>
          <Terminal
            ref={handle}
            aria-label={`Terminal of ${harness}`}
            scrollback={LIVE_SCROLLBACK}
            fontSize={settings.presentation.fontSize}
            lineHeight={settings.presentation.lineHeight / 100}
            transparent={glass}
            readOnly={ended}
            onData={data => terminals.input(id, data)}
            onResize={(cols, rows) => { history.current?.reflowed(); terminals.resize(id, cols, rows); }}
            onLinkOpen={target => terminals.openLink(id, target)}
            onLinkHover={target => setHover(target === null ? null : hoverText(classifyTarget(target, cwd)))}
            linkFinder={findLinks}
            onBufferChange={bump}
            appKeys={event => {
              if (event.type === 'keydown' && !app.ui.modalOpen()) {
                // The window's own chords run from here: a terminal is a place of `ownKeysIn`, where the shell runs no key of a command.
                const command = commandOfKey(event);
                if (command) { event.preventDefault(); runners(app)[command]?.(); }
              }
              return terminalLeavesKey(event);
            }}
          />
        </Cell>
        {buffer ? (
          <OverviewStrip
            count={buffer.length}
            extent={index => textExtent(buffer.line(index))}
            viewport={{ start: buffer.viewportY, size: buffer.rows }}
            archive={archived > 0 ? { count: archived, extent: index => { const cells = archive!.archive.width(index); return cells ? [0, cells] : null; } } : undefined}
            marks={marks}
            revision={revision}
            onSeek={line => handle.current?.scrollToLine(line)}
            onArchiveSeek={line => setHistoryAt(line)}
          />
        ) : null}
      </Cluster>
      {atTop ? (
        <Cluster>
          <Button variant="ghost" size="xs" onClick={() => setHistoryAt(Math.max(0, archived - 1))}>
            <Icon name="history" />
            {`Earlier lines · ${archived.toLocaleString()}`}
          </Button>
        </Cluster>
      ) : null}
      <Cluster gap="sm" pad="xs" wrap={false} role="group" aria-label="Terminal details">
        {ended ? <Badge tone="warning">{session.exitCode === null ? 'Exited' : `Exited · code ${session.exitCode}`}</Badge> : <Badge tone="success">Running</Badge>}
        <Text variant="muted">{harness}</Text>
        {measured ? <Text variant="muted" title={measured.detail} aria-label="Terminal resources">{measured.summary}</Text> : null}
        <Text variant="muted">{handle.current ? `${handle.current.size.cols}×${handle.current.size.rows}` : ''}</Text>
        <Text variant="muted" truncate title={cwd}>{hover ?? cwd}</Text>
        {ended ? (
          <>
            <Button size="sm" variant="outline" onClick={() => { void app.opens.newSessionLike(session, state.groupId); }}>New session</Button>
            <Button size="sm" variant="outline" onClick={() => { void terminals.stop(id); }}>Close terminal</Button>
          </>
        ) : null}
      </Cluster>
      {open && archive ? <TerminalHistoryView archive={archive} line={historyAt} onClose={() => { setHistoryAt(null); handle.current?.focus(); }} /> : null}
    </Column>
  );
}

/** The tab of a session that does not run: what it is, where its open stands, and its button. No button is disabled because another
 *  session is opening: asking queues this one. */
function SavedSession({ id }: { id: string }) {
  const app = useApp();
  const threads = useController(app.threads);
  const opens = useController(app.opens);
  const saved = threads.savedSession(id);
  if (!saved) return <Empty fill title="This terminal is not here" description="Its session is no longer in this thread." />;
  const name = threads.memberName(saved);
  const plan = openCard(saved.id, { current: opens.queue.current(), wants: opens.wants.all(), now: Date.now() });
  if (plan !== 'offer') {
    const reason = opens.wants.all().get(saved.id)?.reason ?? '';
    return (
      <Empty
        fill
        title={name}
        description={plan === 'opening' ? `Opening${opens.openingStep ? `: ${opens.openingStep}` : ''}…`
          : plan === 'queued' ? 'Waiting its turn: sessions open one at a time.'
          : `Not opened yet. ${reason} Shinatama tries again by itself.`}
        action={(
          <Cluster>
            {plan === 'retrying' ? <Button onClick={() => opens.wantOpen(saved.id)}>Try now</Button> : null}
            {plan !== 'opening' ? <Button variant="outline" onClick={() => opens.cancelWant(saved.id)}>Don't open</Button> : null}
          </Cluster>
        )}
      />
    );
  }
  const step = openStep(saved, harness => opens.canResume(harness));
  const said = savedCardText(saved, step, threads.harnessLabel(saved.harness));
  const issue = opens.openIssues.get(saved.id);
  return (
    <Empty
      fill
      title={name}
      description={issue ? `${said} Last try: ${issue}` : said}
      action={step === 'resume' ? <Button onClick={() => opens.wantOpen(saved.id)}>Resume</Button>
        : step === 'recover' ? <Button onClick={() => { void opens.recoverSaved(saved); }}>Recover session</Button>
        : saved.status === 'unbound' ? <Button onClick={() => { void threads.forgetSession(saved.id).catch(() => app.ui.error('The session could not be removed. Nothing was changed.')); }}>Remove</Button>
        : undefined}
    />
  );
}
