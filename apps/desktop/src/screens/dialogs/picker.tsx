// The projects of the window: the recent folders, the folders found under the roots that are scanned, the roots themselves, and the creation
// of a new project. Choosing is a folder operation Rust can cancel (`CatalogController`); roots are saved at once, and a removed root comes
// back with Undo. F3 (the command Open a folder as a project) opens the native picker at once.
import { useEffect, useState } from 'react';
import { Dialog, FormField, ItemList, ItemRow, Note, SearchField } from '@acaprino/sigrid/components';
import { Badge, Button, Cluster, Column, Icon, Input, Section, Spacer, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { MAX_ROOTS } from '../../controllers/catalog.ts';
import type { CatalogRoot } from '../../project-catalog.ts';
import { folderName } from '../../paths.ts';

const STATE_WORDS = { ready: 'Ready', scanning: 'Scanning…', missing: 'Folder unavailable', unavailable: 'Cannot read this root', timed_out: 'Scan timed out; waiting for the drive', truncated: 'Folder limit reached' } as const;

function rootWords(scanned: CatalogRoot | undefined): string {
  if (!scanned) return 'Scanning…';
  return scanned.state === 'ready' ? `${scanned.projectCount} ${scanned.projectCount === 1 ? 'project' : 'projects'}` : STATE_WORDS[scanned.state];
}

export function ProjectPicker({ browse }: { browse?: boolean }) {
  const app = useApp();
  const catalog = useController(app.catalog);
  const [folder, setFolder] = useState('');
  const [root, setRoot] = useState('');
  useEffect(() => {
    catalog.openPicker();
    setFolder(app.projects.path);
    if (browse) void catalog.chooseProject();
    return () => catalog.closePicker();
  }, [catalog]); // eslint-disable-line react-hooks/exhaustive-deps
  const { shown, config } = catalog;
  const blocked = catalog.saving || !config || !catalog.configFresh;
  const roots = config?.roots ?? [];
  const defaultRoot = roots.find(entry => entry.id === config?.defaultRootId);
  const none = !catalog.configLoading && roots.length === 0;
  const addTyped = async () => { if (await catalog.addRoot(root)) setRoot(''); };
  const committing = catalog.phase === 'committing';
  return (
    <Dialog
      open
      onOpenChange={next => { if (!next) void catalog.cancelOrClose(); }}
      title="Projects"
      description="Choose the folder new sessions start in."
      width="xl"
      footer={(
        <>
          <Button variant="outline" disabled={catalog.saving} onClick={() => app.ui.open({ kind: 'new-project' })}>New project…</Button>
          <Button variant="outline" disabled={catalog.saving} onClick={() => { void catalog.chooseProject(); }}>Open one folder…</Button>
          <Spacer />
          <Button variant="outline" disabled={catalog.phase === 'cancelling'} onClick={() => { void catalog.cancelOrClose(); }}>{committing ? 'Close; save continues' : 'Close'}</Button>
        </>
      )}
    >
      <Column gap="lg" aria-busy={catalog.saving || catalog.loading}>
        {catalog.error ? <Note tone="error" title={catalog.error} /> : null}
        <SearchField aria-label="Filter projects" placeholder="Filter projects" value={catalog.filter} onValueChange={text => catalog.setFilter(text)} />
        <Text variant="muted" role="status">{catalog.state}</Text>
        {shown.history.length || catalog.filter ? (
          <Section title={shown.historyHeading} level={3}>
            {shown.history.length ? (
              <ItemList aria-label={shown.historyHeading}>
                {shown.history.map(project => (
                  <ItemRow key={project.path} title={project.name || folderName(project.path)} description={project.available ? project.path : `${project.path} · folder unavailable`} leading={<Icon name="folder" />}
                    disabled={!project.available || catalog.saving} onClick={() => { void catalog.select(project.path); }} />
                ))}
              </ItemList>
            ) : <Text variant="muted">No matching folders.</Text>}
          </Section>
        ) : null}
        {shown.projects.length ? (
          <Section title="Projects" level={3}>
            <ItemList aria-label="Projects under the roots">
              {shown.projects.map(project => (
                <ItemRow key={project.path} title={project.name} description={project.path} leading={<Icon name="folder" />}
                  disabled={!project.available || catalog.saving} onClick={() => { void catalog.select(project.path); }} />
              ))}
            </ItemList>
            {catalog.catalog && (catalog.catalog.total > catalog.catalog.limit || catalog.catalog.offset > 0) ? (
              <Cluster gap="sm" justify="between">
                <Button variant="outline" size="sm" disabled={!catalog.canPrevious} onClick={() => catalog.previousPage()}>Previous</Button>
                <Text variant="muted">{`${Math.min(catalog.catalog.offset + 1, catalog.catalog.total)}–${Math.min(catalog.catalog.offset + catalog.catalog.limit, catalog.catalog.total)} of ${catalog.catalog.total}`}</Text>
                <Button variant="outline" size="sm" disabled={!catalog.canNext} onClick={() => catalog.nextPage()}>Next</Button>
              </Cluster>
            ) : null}
          </Section>
        ) : null}
        <FormField label="Folder path" help="Type the path of a folder and press Enter to make it the project.">
          <Input mono autoComplete="off" spellCheck={false} maxLength={4096} value={folder} onChange={event => setFolder(event.target.value)} onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); void catalog.select(folder.trim()); } }} />
        </FormField>
        <Section title="Project roots" level={3} end={<Button variant={none ? 'default' : 'outline'} size="sm" disabled={blocked || roots.length >= MAX_ROOTS} onClick={() => { void catalog.chooseRoot(); }}>Choose a root…</Button>}>
          <Text variant="muted">{defaultRoot ? `New projects are created in ${defaultRoot.path}.` : roots.length ? 'Make one root the default to create new projects in it.' : catalog.configLoading ? 'Reading project roots…' : 'No root yet. Choose the folder that holds your projects.'}</Text>
          {roots.length ? (
            <ItemList aria-label="Project roots">
              {roots.map((entry, index) => {
                const isDefault = config?.defaultRootId === entry.id;
                return (
                  <ItemRow
                    key={entry.id} title={entry.path} description={rootWords(catalog.catalog?.roots.find(scanned => scanned.id === entry.id))} leading={<Icon name="folder" />}
                    end={(
                      <Cluster gap="xs" wrap={false}>
                        {isDefault ? <Badge tone="info">Default</Badge> : null}
                      </Cluster>
                    )}
                    control={(
                      <Cluster gap="xs" wrap={false}>
                        {isDefault ? null : <Button variant="outline" size="sm" disabled={blocked} aria-label={`Make default: ${entry.path}`} onClick={() => { void catalog.makeDefault(index); }}>Make default</Button>}
                        <Button variant="outline" size="sm" disabled={blocked} title="Stop listing this root. The folder itself is not touched." aria-label={`Remove root ${entry.path}`} onClick={() => { void catalog.removeRoot(index); }}>Remove root</Button>
                      </Cluster>
                    )}
                    controlSide="end"
                  />
                );
              })}
            </ItemList>
          ) : null}
          <Cluster gap="sm" wrap={false}>
            <Input aria-label="Root path" mono placeholder="Path of a folder that holds your projects" autoComplete="off" spellCheck={false} maxLength={4096} value={root} onChange={event => setRoot(event.target.value)} onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); void addTyped(); } }} />
            <Button variant="outline" disabled={blocked || roots.length >= MAX_ROOTS} onClick={() => { void addTyped(); }}>Add root</Button>
            <Button variant="outline" disabled={catalog.saving} onClick={() => catalog.refresh()}>Refresh</Button>
          </Cluster>
        </Section>
      </Column>
    </Dialog>
  );
}

