// One raw native entry of a session. It may hold credentials, private reasoning, signed or encrypted data: nothing is hidden or redacted
// here, so opening this panel reads nothing; only a checked warning and an explicit Load do. Closing clears it; nothing is saved.
import { useEffect } from 'react';
import { CopyButton, Note } from '@acaprino/sigrid/components';
import { Button, Checkbox, Cluster, Column, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import type { RawSelection } from '../../session-detail-model.ts';
import { Frame } from './frame.tsx';

export function RawPanel({ selection }: { selection: RawSelection }) {
  const app = useApp();
  const raw = useController(app.sessions.raw);
  useEffect(() => {
    app.sessions.raw.open(selection);
    return () => app.sessions.raw.clear();
  }, [app, selection]);
  return (
    <Frame title="RAW native entry">
      <Note tone="warning" title="RAW may contain private data">It may contain credentials, private reasoning, signed or encrypted data, and other sensitive native fields. Nothing is hidden or redacted in this view. Load only if you intend to inspect this entry.</Note>
      <Checkbox checked={raw.confirmed} disabled={raw.busy} onCheckedChange={checked => raw.confirm(checked === true)} label="I understand and want to load this one RAW entry." />
      <Text variant="muted" mono truncate>{`Session ${selection.nativeId} · Entry ${selection.entryOrdinal} · Archive ${selection.root.storeRootId}`}</Text>
      <Cluster gap="sm">
        <Button variant="outline" disabled={raw.busy || !raw.confirmed} onClick={() => { void raw.load(); }}>Load this RAW entry</Button>
        {raw.content && raw.confirmed ? <CopyButton value={raw.content} label="Copy visible RAW" showLabel variant="outline" onCopied={() => app.ui.info('Visible RAW copied. Your clipboard may be available to other applications.')} onCopyError={() => app.ui.error('Clipboard access is unavailable. Select the text and press Ctrl+C to copy it yourself.')} /> : null}
      </Cluster>
      {raw.failed ? <Note tone="error" title={raw.message} /> : <Text role="status" variant="muted">{raw.busy ? 'Reading this confirmed RAW entry…' : raw.message}</Text>}
      {raw.content ? <Column><Text as="div" pre mono tabIndex={0} aria-label="Visible RAW native record">{raw.content}</Text></Column> : null}
      <Text variant="muted">One entry, at most 64 KiB. Attachment files stay closed. Closing clears this view. Copy writes only the displayed RAW to your clipboard.</Text>
    </Frame>
  );
}
