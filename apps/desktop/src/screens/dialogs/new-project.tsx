// New project: a folder made inside one of the configured roots, with Git initialised if asked. It is a folder operation Rust can cancel:
// Cancel (or Esc) while it runs asks Rust to stop it, and once it entered its final save it says so and lets it finish. The project becomes the
// one new sessions start in only after the backend returns it committed.
import { useEffect } from 'react';
import { Dialog, FormField, Note } from '@acaprino/sigrid/components';
import { Button, Checkbox, Column, Input, Select, Spacer, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';

export function NewProject() {
  const app = useApp();
  const catalog = useController(app.catalog);
  useEffect(() => { void catalog.openCreation(); return () => catalog.closeCreation(); }, [catalog]);
  const { creation } = catalog;
  const committing = catalog.phase === 'committing';
  const cancelling = catalog.phase === 'cancelling';
  const status = cancelling ? 'Cancelling project creation…' : committing ? 'Creation has entered its final save. Closing this dialog keeps it running.'
    : catalog.saving ? 'Preparing the project folder…' : catalog.creationLoading ? 'Reading project roots…' : '';
  return (
    <Dialog
      open
      onOpenChange={next => { if (!next) void catalog.cancelOrClose(); }}
      title="New project"
      description="Create a project folder inside one of your configured roots."
      width="lg"
      footer={(
        <>
          <Button variant="outline" disabled={catalog.saving || catalog.creationLoading} onClick={() => { void catalog.loadOptions(); }}>Reload roots</Button>
          <Spacer />
          <Button variant="outline" disabled={cancelling} onClick={() => { void catalog.cancelOrClose(); }}>{committing ? 'Close; creation continues' : 'Cancel'}</Button>
          <Button type="submit" form="new-project-form" disabled={catalog.saving || catalog.creationLoading || !creation || !catalog.rootId}>Create project</Button>
        </>
      )}
    >
      <form id="new-project-form" noValidate aria-busy={catalog.saving || catalog.creationLoading} onSubmit={event => { event.preventDefault(); void catalog.create(); }}>
        <Column gap="md">
          <FormField
            label="Project root"
            help={creation ? (creation.roots.length ? 'The configured default is selected. Choose a different root explicitly if needed.' : 'Configure a project root in the project picker before creating a project.') : undefined}
            required
          >
            <Select
              aria-label="Project root"
              placeholder="Choose a project root"
              options={(creation?.roots ?? []).map(root => ({ value: root.id, label: root.path }))}
              value={catalog.rootId}
              disabled={catalog.saving || catalog.creationLoading}
              onValueChange={id => catalog.setRoot(id)}
            />
          </FormField>
          <FormField label="Project folder name" required {...(catalog.nameError ? { error: catalog.nameError } : {})} help={catalog.destination ? `Destination: ${catalog.destination}` : undefined}>
            <Input autoComplete="off" spellCheck={false} maxLength={255} placeholder="my-project" value={catalog.name} disabled={catalog.saving} onChange={event => catalog.setName(event.target.value)} />
          </FormField>
          <Checkbox label="Initialize Git repository · default branch main" checked={catalog.git} disabled={catalog.saving} onCheckedChange={on => catalog.setGit(on === true)} />
          {status ? <Text variant="muted" role="status">{status}</Text> : null}
          {catalog.error ? <Note tone="error" title={catalog.error} /> : null}
        </Column>
      </form>
    </Dialog>
  );
}
