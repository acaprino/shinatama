// INSPECTOR. The side panel stack of the Workspace view and the Sessions view: the one on top is `app.ui.panel`, drawn in Sigrid's
// `Inspector` by `Frame` (its title, a way back to the panel it covers, its actions at the foot). A panel opened from inside another covers
// it and gives it back. The panels are `PanelSpec` (./spec.ts): the conversation of a session and what is done with it (search in it, a
// raw entry, export, hop), the facts of a saved session, the project files, the archives, what agents left on disk, the instances, the
// index of session text and the settings of a thread.
import { useApp, useController } from '../../hooks.ts';
import { ArchivesPanel } from './archives.tsx';
import { ExportPanel } from './export.tsx';
import { FilesPanel } from './files.tsx';
import { FindPanel } from './find.tsx';
import { HopPanel } from './hop.tsx';
import { InstancesPanel } from './instances.tsx';
import { LeftoversPanel } from './leftovers.tsx';
import { RawPanel } from './raw.tsx';
import { SearchIndexPanel } from './search-index.tsx';
import { SessionPanel } from './session.tsx';
import { SummaryPanel } from './summary.tsx';
import { ThreadPanel } from './thread.tsx';

export function Inspector() {
  const app = useApp();
  const ui = useController(app.ui);
  const panel = ui.panel;
  if (!panel) return null;
  switch (panel.kind) {
    case 'session': return <SessionPanel root={panel.root} nativeId={panel.nativeId} {...(panel.at ? { at: panel.at } : {})} />;
    case 'summary': return <SummaryPanel root={panel.root} nativeId={panel.nativeId} />;
    case 'find': return <FindPanel root={panel.root} nativeId={panel.nativeId} fingerprint={panel.fingerprint} />;
    case 'raw': return <RawPanel selection={panel.selection} />;
    case 'export': return <ExportPanel selection={panel.selection} />;
    case 'hop': return <HopPanel selection={panel.selection} />;
    case 'files': return <FilesPanel />;
    case 'archives': return <ArchivesPanel />;
    case 'leftovers': return <LeftoversPanel />;
    case 'instances': return <InstancesPanel />;
    case 'search-index': return <SearchIndexPanel />;
    case 'thread': return <ThreadPanel key={panel.threadId} threadId={panel.threadId} />;
  }
}
