// Settings, Alerts, Plugins and the Model proxy: what Shinatama keeps for its sessions. Alerts apply at once; Muro's Sync now and the proxy's
// commands are explicit, and a stop that would cut agents off, or a restart, asks first with what it costs (`ServicesController`).
import { inTauri } from '../../api/core.ts';
import { Note, SettingRow } from '@acaprino/sigrid/components';
import { Badge, Button, Cluster, Column, Switch, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { pluginsButtons, pluginsLine } from '../../muro-status.ts';
import {
  archiveLine, buttons, diskLine, driftLine, exitLine, faultLine, messageLine, phaseLabel, showsTraffic, stallLine, startLabel, indicatorTone, trafficLine,
} from '../../sidecar-status.ts';

/** Where the commands answer: the window, or the browser preview in development (the mock answers there). */
const desktop = inTauri || import.meta.env.DEV;

export function Alerts() {
  const app = useApp();
  const services = useController(app.services);
  const { alerts } = services;
  return (
    <Column gap="md">
      <Text variant="muted">Shinatama shows its face and a speech bubble over the desktop when a terminal raises a native alert or a process ends. Alerts work while the app is open.</Text>
      <SettingRow label="Show desktop alerts" control={<Switch aria-label="Show desktop alerts" checked={alerts.enabled} disabled={services.alertsBusy} onCheckedChange={enabled => { void services.configureAlerts({ ...alerts, enabled }, true); }} />} />
      <SettingRow label="Play a short sound" control={<Switch aria-label="Play a short sound" checked={alerts.sound} disabled={services.alertsBusy} onCheckedChange={sound => { void services.configureAlerts({ ...alerts, sound }, true); }} />} />
      <Cluster gap="sm">
        <Button variant="outline" disabled={services.alertsBusy || !services.alertsApplied || !alerts.enabled} onClick={() => { void services.previewAlert(); }}>Test alert</Button>
        <Text variant="muted" aria-live="polite">{services.alertsStatus}</Text>
      </Cluster>
      <Text variant="muted">Which signals arrive depends on the agent. Read the question and answer it in the session: the alert opens the terminal and grants nothing.</Text>
    </Column>
  );
}

const clock = (seconds: number) => {
  const at = new Date(seconds * 1000);
  const today = at.toDateString() === new Date().toDateString();
  return at.toLocaleString([], today ? { hour: '2-digit', minute: '2-digit' } : { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
};

export function Plugins() {
  const app = useApp();
  const services = useController(app.services);
  const { muro } = services;
  const why = pluginsButtons(muro, desktop);
  const problem = services.problems.get('muro');
  const line = problem ? { text: problem, tone: 'error' as const } : !desktop ? { text: 'Open the desktop app to use Muro.', tone: 'info' as const }
    : !muro ? { text: "Muro's state could not be read. Reload to try again.", tone: 'error' as const } : pluginsLine(muro, clock);
  return (
    <Column gap="md">
      <Text variant="muted">Muro installs and updates the plugins you choose for the agents Shinatama launches, in their homes inside Shinatama only: your own Muro and your agents' usual homes are left alone. Open Muro to choose them; a session already open gets a new plugin at its next start.</Text>
      <Note tone={line.tone === 'error' ? 'error' : 'info'} title={line.text} />
      <Cluster gap="sm">
        <Button variant="outline" disabled={why.sync !== null || services.muroBusy} loading={services.muroBusy} loadingText="Working…" title={why.sync ?? 'Ask Muro to check and apply the plugins now'} onClick={() => { void services.syncMuro(); }}>Sync now</Button>
        <Button variant="outline" disabled={why.open !== null || services.muroBusy} title={why.open ?? "Show Muro's window on the list of Shinatama's agents"} onClick={() => { void services.openMuro(); }}>Open Muro</Button>
      </Cluster>
    </Column>
  );
}

export function Proxy() {
  const app = useApp();
  const services = useController(app.services);
  const { proxy } = services;
  const phase = proxy?.phase ?? 'unavailable';
  const tone = { green: 'success', amber: 'warning', red: 'danger', off: undefined }[indicatorTone(proxy?.stalled ? 'failed' : phase)] as 'success' | 'warning' | 'danger' | undefined;
  const can = buttons(proxy, services.proxyBusy);
  const lines = proxy ? [
    showsTraffic(phase) ? trafficLine(proxy) : null, messageLine(proxy), stallLine(proxy), faultLine(proxy), diskLine(proxy), archiveLine(proxy), exitLine(proxy), driftLine(proxy),
  ].filter((text): text is string => !!text) : [];
  return (
    <Column gap="md">
      <Text variant="muted">Every agent reaches its model through this local service. Recovery never resends an agent request.</Text>
      <Cluster gap="sm">
        <Badge tone={tone}>{proxy?.stalled ? 'Not reporting' : phaseLabel(phase)}</Badge>
        {services.problems.has('proxy') && !proxy ? <Text variant="muted">The proxy's state could not be read.</Text> : null}
      </Cluster>
      {lines.map(text => <Text key={text} as="p" variant="muted">{text}</Text>)}
      <Cluster gap="sm">
        <Button variant="outline" disabled={!can.stop} onClick={() => { void services.stopProxy(); }}>Stop</Button>
        <Button variant="outline" disabled={!can.retry} onClick={() => { void services.retryProxy(); }}>Retry recovery</Button>
        {services.restartOffered ? <Button variant="destructive" disabled={services.proxyBusy} onClick={() => { void services.restart(); }}>Restart Shinatama</Button> : null}
        <Button variant="outline" disabled={!can.start} onClick={() => { void services.startProxy(); }}>{startLabel(proxy)}</Button>
      </Cluster>
    </Column>
  );
}
