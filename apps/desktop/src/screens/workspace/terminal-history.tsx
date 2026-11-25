// The lines a terminal no longer holds: its archive (terminal-history.ts) shown in a read-only terminal of Sigrid's, a window of
// `WINDOW_LINES` around the line asked for. «Earlier» and «Later» move the window; Esc or «Back to live» closes it and frees what it held.
// The live terminal keeps running and receiving output underneath.
import { useEffect, useRef, useState } from 'react';
import { Dialog } from '@acaprino/sigrid/components';
import { Button, Cluster, Column } from '@acaprino/sigrid/primitives';
import { Terminal, type TerminalHandle } from '@acaprino/sigrid/terminal';
import { WINDOW_LINES, WINDOW_STEP, windowStart, windowTitle, type TerminalHistory } from '../../terminal-history.ts';

type Anchor = 'top' | 'bottom' | 'line';

export function TerminalHistoryView({ archive, line, onClose }: { archive: TerminalHistory; line: number; onClose: () => void }) {
  const screen = useRef<TerminalHandle>(null);
  const [window_, setWindow] = useState({ start: 0, shown: 0 });
  const [busy, setBusy] = useState(true);
  const revision = useRef(0);
  const total = archive.archive.lines();
  const lost = archive.archive.dropped() + archive.archive.missed();

  async function show(at: number, anchor: Anchor) {
    const current = ++revision.current;
    const start = windowStart(at, archive.archive.lines());
    const end = Math.min(archive.archive.lines(), start + WINDOW_LINES);
    setBusy(true);
    const lines = await archive.archive.read(start, end);
    const part = screen.current;
    if (!part || current !== revision.current) return;
    part.fit();
    part.reset();
    await new Promise<void>(resolve => part.write('\x1b[?25l' + lines.join('\r\n') + '\x1b[0m', resolve));
    if (current !== revision.current) return;
    setWindow({ start, shown: lines.length });
    setBusy(false);
    if (anchor === 'top') part.scrollLines(-WINDOW_LINES);
    else if (anchor === 'bottom') part.scrollToBottom();
    else part.scrollToLine(Math.max(0, at - start - Math.floor(part.size.rows / 2)));
  }
  useEffect(() => { void show(line, 'line'); return () => { revision.current += 1; }; }, [line]);

  return (
    <Dialog
      open
      onOpenChange={open => { if (!open) onClose(); }}
      title="Earlier terminal lines"
      description={windowTitle(window_.start, window_.shown, total, lost)}
      drawer
      onCloseAutoFocus={event => event.preventDefault()}
      footer={(
        <Cluster>
          <Button variant="outline" disabled={busy || window_.start === 0} onClick={() => { void show(window_.start - WINDOW_STEP + Math.floor(WINDOW_LINES / 2), 'bottom'); }}>Earlier</Button>
          <Button variant="outline" disabled={busy || window_.start + window_.shown >= total} onClick={() => { void show(window_.start + WINDOW_STEP + Math.floor(WINDOW_LINES / 2), 'top'); }}>Later</Button>
          <Button onClick={onClose}>Back to live</Button>
        </Cluster>
      )}
    >
      <Column fill>
        <Terminal ref={screen} aria-label="Earlier terminal lines" readOnly scrollback={WINDOW_LINES} />
      </Column>
    </Dialog>
  );
}
