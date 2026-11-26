// The actions row of the Settings view: what the page says (loading, saving, what was saved, what is wrong) and Reload and Save, the primary
// action last. Save writes what waits for it: the models, the launch settings and the memory.
import { Button, Cluster, Icon, Spacer, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';

export function SettingsActions() {
  const app = useApp();
  const settings = useController(app.settings);
  const { line } = settings;
  return (
    <Cluster gap="sm" justify="end" wrap>
      <Text truncate tone={line.error ? 'danger' : undefined} aria-live="polite">{line.text || 'Agents, Models, Memory, GitHub and Environment variables apply when you press Save.'}</Text>
      <Spacer />
      <Button variant="outline" disabled={settings.busy} title="Read every saved setting again" onClick={() => { void settings.reload(); }}><Icon name="refresh" />Reload</Button>
      <Button disabled={!!settings.saveBlock} loading={settings.busy && !!settings.line.text.startsWith('Saving')} loadingText="Saving…" title={settings.saveBlock || 'Save the settings that wait for it'} onClick={() => { void settings.save(); }}>Save</Button>
    </Cluster>
  );
}
