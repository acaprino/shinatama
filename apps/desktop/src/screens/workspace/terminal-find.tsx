// Find in terminal: the text is looked for in the lines the terminal retains (its screen and its scrollback), the match is selected and
// scrolled into sight. A dialog of Sigrid's with its search field; Enter goes to the next line, and closing it gives the keyboard back to
// the terminal.
import { useEffect, useMemo, useState } from 'react';
import { Dialog, SearchField } from '@acaprino/sigrid/components';
import { Button, Cluster, Column, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { findLines, findStatus, stepHit } from '../../terminal-find.ts';
import { handleOf } from './handles.ts';

export function TerminalFind() {
  const app = useApp();
  const terminals = useController(app.terminals);
  const id = terminals.finding;
  const [query, setQuery] = useState('');
  const [chosen, setChosen] = useState(-1);
  const handle = id ? handleOf(id) : undefined;
  const hits = useMemo(() => {
    const view = handle?.buffer;
    return view ? findLines(index => view.line(index), view.length, query) : { rows: [], overflow: false };
  }, [handle, query, id]);

  const select = (at: number) => {
    const row = hits.rows[at];
    if (!handle || row === undefined) return;
    setChosen(at);
    handle.select(0, row, handle.size.cols);
    handle.scrollToLine(row);
  };
  // A new search starts at its first line.
  useEffect(() => { setChosen(-1); if (hits.rows.length) select(0); }, [hits]);
  useEffect(() => { if (id === null) { setQuery(''); setChosen(-1); } }, [id]);

  return (
    <Dialog
      open={id !== null}
      onOpenChange={open => { if (!open) terminals.closeFind(); }}
      title="Find in terminal"
      onCloseAutoFocus={event => { event.preventDefault(); handle?.focus(); }}
      footer={<Button variant="ghost" onClick={() => terminals.closeFind()}>Close</Button>}
    >
      <Column gap="sm">
        <SearchField
          value={query}
          onValueChange={setQuery}
          aria-label="Text in this terminal"
          maxLength={128}
          autoComplete="off"
          autoFocus
          onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); select(stepHit(hits.rows.length, chosen, event.shiftKey ? -1 : 1)); } }}
        />
        <Text variant="muted" role="status">{findStatus(query, hits, chosen)}</Text>
        <Cluster>
          <Button variant="outline" disabled={!hits.rows.length} onClick={() => select(stepHit(hits.rows.length, chosen, -1))}>Previous</Button>
          <Button disabled={!hits.rows.length} onClick={() => select(stepHit(hits.rows.length, chosen, 1))}>Next</Button>
        </Cluster>
      </Column>
    </Dialog>
  );
}
