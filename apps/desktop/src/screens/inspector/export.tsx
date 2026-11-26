// Exporting one session to a file the person chooses: the format and what it keeps and loses, how private the file is, and what was
// written. Secrets are always redacted and reasoning is never exported; the file is written under a temporary name and renamed when
// complete.
import { useEffect } from 'react';
import { DescriptionList, Note } from '@acaprino/sigrid/components';
import { Button, Checkbox, Cluster, Column, Progress, RadioGroup, RadioGroupItem, Section, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { EXPORT_FORMATS, EXPORT_MODES, exportFacts, type ExportFormat, type ExportMode, type ExportSelection } from '../../session-export-model.ts';
import { Frame } from './frame.tsx';

export function ExportPanel({ selection }: { selection: ExportSelection }) {
  const app = useApp();
  const exporter = useController(app.sessions.exporter);
  useController(exporter.progress);
  useEffect(() => {
    app.sessions.exporter.open(selection);
    return () => app.sessions.exporter.close();
  }, [app, selection]);
  const { busy, saved } = exporter;
  const line = exporter.progress.line;
  return (
    <Frame title="Export session" footer={<Cluster gap="sm"><Button disabled={busy || !exporter.selected} onClick={() => { void exporter.save(); }}>Save as…</Button></Cluster>}>
      <Text variant="muted">{`${selection.harness} · session ${selection.nativeId}`}</Text>
      <Section title="Format" level={4}>
        <RadioGroup aria-label="Format" value={exporter.format} disabled={busy} onValueChange={value => exporter.setFormat(value as ExportFormat)}>
          {EXPORT_FORMATS.map(format => (
            <Column key={format.value} gap="xs">
              <RadioGroupItem value={format.value}>{format.label}</RadioGroupItem>
              <Text variant="muted">{format.detail}</Text>
            </Column>
          ))}
        </RadioGroup>
        <Text variant="muted">The agent's own export is not offered yet: no native export has been proven for this agent.</Text>
      </Section>
      <Section title="Privacy" level={4}>
        <RadioGroup aria-label="Privacy" value={exporter.mode} disabled={busy} onValueChange={value => exporter.setMode(value as ExportMode)}>
          {EXPORT_MODES.map(mode => (
            <Column key={mode.value} gap="xs">
              <RadioGroupItem value={mode.value}>{mode.label}</RadioGroupItem>
              <Text variant="muted">{mode.detail}</Text>
            </Column>
          ))}
        </RadioGroup>
      </Section>
      <Checkbox checked={exporter.withAttachments} disabled={busy || exporter.format === 'mukade'} onCheckedChange={checked => exporter.setAttachments(checked === true)} label="Copy attachment content beside the file (inline images only). Otherwise attachments are hashed references." />
      <Text variant="muted">Reasoning is never exported. The file is written to a temporary name and renamed when complete.</Text>
      {exporter.failed ? <Note tone="error" title={exporter.message} /> : <Text role="status" variant="muted">{exporter.message}</Text>}
      {busy ? <Progress aria-label="Exporting" value={null} /> : null}
      {line ? <Cluster gap="sm" justify="between"><Text role="status" variant="muted">{line}</Text><Button size="sm" variant="outline" disabled={exporter.progress.wasCancelled} onClick={() => exporter.progress.cancel()}>Cancel</Button></Cluster> : null}
      {saved ? <DescriptionList aria-label="What was written" items={exportFacts(saved)} /> : null}
    </Frame>
  );
}
