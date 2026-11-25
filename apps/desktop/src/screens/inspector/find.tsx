// A search inside one session: a literal, case-insensitive match over what the conversation said (user, assistant, plain tool results
// and compaction summaries). Reasoning, tool arguments, structured results and attachment content are not searched. A match opens the
// saved transcript at its event; nothing starts an agent.
import { useEffect } from 'react';
import { Note, SearchField } from '@acaprino/sigrid/components';
import { Button, Cluster, Column, Progress, Text } from '@acaprino/sigrid/primitives';
import { ItemList, ItemRow } from '@acaprino/sigrid/components';
import { useApp, useController } from '../../hooks.ts';
import { positionText } from '../../session-detail-model.ts';
import { sameRoot, type SessionRootSelector } from '../../session-library-model.ts';
import { Frame } from './frame.tsx';

export function FindPanel({ root, nativeId, fingerprint }: { root: SessionRootSelector; nativeId: string; fingerprint: string }) {
  const app = useApp();
  const find = useController(app.sessions.find);
  useController(find.progress);
  // Opened when the panel is drawn for another session; kept while the conversation at a match covers it (the way back finds the matches).
  useEffect(() => {
    const { selected } = app.sessions.find;
    const same = !!selected && selected.nativeId === nativeId && sameRoot(selected.root, root) && selected.cursor?.fingerprint === fingerprint;
    if (!same) app.sessions.find.open({ root, nativeId, limit: 50, cursor: { offset: 0, fingerprint } });
    return () => { if (!app.ui.panels.some(panel => panel.kind === 'find')) app.sessions.find.clear(); };
  }, [app, root.storeRootId, root.configurationRevision, nativeId, fingerprint]); // eslint-disable-line react-hooks/exhaustive-deps
  const { page, busy } = find;
  const line = find.progress.line;
  return (
    <Frame title="Find in this session">
      <Text variant="muted" mono truncate>{`Session ${nativeId} · Archive ${root.storeRootId}`}</Text>
      <form onSubmit={event => { event.preventDefault(); void find.search(); }} aria-label="Search this session">
        <Column gap="sm">
          <SearchField aria-label="Literal text" placeholder="Literal text" value={find.query} autoFocus maxLength={128} disabled={busy} onValueChange={value => find.setQuery(value)} />
          <Cluster><Button type="submit" variant="outline" disabled={!find.canSearch}>Search this session</Button></Cluster>
        </Column>
      </form>
      {find.failed ? <Note tone="error" title={find.message} /> : <Text role="status" variant="muted">{busy ? 'Searching this verified session…' : find.message}</Text>}
      {busy ? <Progress aria-label="Searching" value={null} /> : null}
      {line ? <Cluster gap="sm" justify="between"><Text role="status" variant="muted">{line}</Text><Button size="sm" variant="outline" disabled={find.progress.wasCancelled} onClick={() => find.progress.cancel()}>Cancel</Button></Cluster> : null}
      {page ? (
        <Column gap="sm">
          <ItemList aria-label="Session search matches">
            {page.hits.map(hit => (
              <ItemRow
                key={hit.matchOrdinal}
                title={hit.snippet}
                wrapTitle
                description={`Match ${hit.matchOrdinal + 1} · Entry ${hit.entryOrdinal}${hit.truncated ? ' · Excerpt limited to 512 bytes' : ''}`}
                onClick={() => find.choose(hit)}
              />
            ))}
          </ItemList>
          <Cluster gap="sm" justify="between">
            <Cluster gap="sm">
              <Button size="sm" variant="outline" disabled={busy || !find.history.length} onClick={() => { void find.previous(); }}>Previous matches</Button>
              <Button size="sm" variant="outline" disabled={busy || !page.next} onClick={() => { void find.next(); }}>Next matches</Button>
            </Cluster>
            <Text variant="muted">{page.totalHits ? positionText(find.cursor?.offset ?? 0, page.hits.length, page.totalHits, 'matches') : '0 matches'}</Text>
          </Cluster>
        </Column>
      ) : null}
      <Text variant="muted">Searches the text of the conversation, case-insensitively and literally. At most 50 matches per page. Choosing a match opens its saved transcript; it starts no agent.</Text>
    </Frame>
  );
}
