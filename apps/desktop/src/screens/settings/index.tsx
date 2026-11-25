// SETTINGS (placeholder, Phase A). One stacked page of settings, with its section index in the sidebar: the terminal presentation, launch,
// models, memory safety, the proxy of the models, alerts, Muro's sync, the interface zoom (a picker on `useWindow().setZoom`, 50 to 200%)
// and glass (`useGlass`, only where `useWindow().own`). Save and Reload are in the actions row. Owned by the agent that fills it in; the
// controllers are `app.settings` and `app.services`.
import { Empty } from '@acaprino/sigrid/components';

export function Settings() {
  return <Empty fill title="Settings" description="The settings are written in the next phase." />;
}
