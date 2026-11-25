// The files of the project, in the inspector: the project (any recent one), a search by name, the tree of folders (Sigrid's `Tree`, a folder
// is read when it is first opened) and below it the file chosen: its text in Sigrid's `CodeView` (the language comes from the file's name),
// and for Markdown its Source or its Preview. A file is edited in place and written only by Save, checked against the version that was
// read. Opening a file never runs it.
import { useEffect, useState } from 'react';
import { Tree } from '@acaprino/sigrid/assemblies';
import { CodeView, codeLanguageOf } from '@acaprino/sigrid/code';
import { Empty, MarkdownBlocks, Note, SearchField } from '@acaprino/sigrid/components';
import { Lightbox } from '@acaprino/sigrid/assemblies';
import { Button, Cell, Cluster, Column, Icon, Progress, Select, Spacer, Text, ToggleGroup, ToggleGroupItem } from '@acaprino/sigrid/primitives';
import { useApp, useAfter, useController } from '../../hooks.ts';
import { dirty, isMarkdown, isPage } from '../../files-model.ts';
import { Frame } from './frame.tsx';

export function FilesPanel() {
  const app = useApp();
  const files = useController(app.files);
  const [picture, setPicture] = useState(false);
  useEffect(() => { void app.files.start(); }, [app]);
  const waiting = useAfter(files.loading);
  const { current, image, project } = files;
  const leave = async () => { if (await files.release()) app.ui.closePanel(); };
  const status = files.failed ? <Note tone="error" title={files.status} /> : <Text role="status" variant="muted" truncate title={files.status}>{files.status}</Text>;
  return (
    <Frame title="Project files" onLeave={leave} fill>
      {files.problem && !project ? (
        <Empty title="No project to browse" description={files.problem} />
      ) : !project ? (
        files.loading && waiting ? <Progress aria-label="Reading the projects" value={null} /> : null
      ) : (
        <Column gap="sm" fill onKeyDownCapture={event => {
          if ((event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLowerCase() === 's') { event.preventDefault(); void files.save(); }
        }}>
          <Select
            aria-label="Project"
            value={project}
            options={files.projects.map(one => ({ value: one.path, label: one.name }))}
            onValueChange={path => { void files.chooseProject(path); }}
          />
          <SearchField aria-label="Find a file by name" placeholder="Find a file by name" value={files.query} onValueChange={value => files.setQuery(value)} />
          <Cell grow>
            <Tree
              key={project}
              aria-label="Folder tree"
              fill
              nodes={files.results ?? files.nodes}
              selected={files.selected}
              onSelect={(_, node) => files.press(node)}
              expanded={files.expanded}
              onExpandedChange={expanded => files.setExpanded(expanded)}
              loadChildren={node => files.children(node)}
            />
          </Cell>
          {status}
          {current ? (
            <>
              <Cluster gap="sm" justify="between">
                <Text variant="strong" truncate title={current.path}>{current.path}</Text>
                <Cluster gap="sm" wrap={false}>
                  <Text variant="muted">{codeLanguageOf(current.path)?.label ?? 'Plain text'}</Text>
                  <Button variant="outline" size="icon-sm" aria-label="Open in a pane" title="Open in a pane: move this file to a tab of the workspace, beside the terminals" onClick={() => files.openInPane()}><Icon name="split" /></Button>
                </Cluster>
              </Cluster>
              <Cluster gap="sm" justify="between">
                {isMarkdown(current.path) ? (
                  <ToggleGroup aria-label="View" value={files.mode} onValueChange={value => files.setMode(value as 'source' | 'preview')}>
                    <ToggleGroupItem value="source">Source</ToggleGroupItem>
                    <ToggleGroupItem value="preview">Preview</ToggleGroupItem>
                  </ToggleGroup>
                ) : <Spacer />}
                <Cluster gap="sm">
                  {isPage(current.path) ? <Button variant="outline" size="sm" title="Open this page in a browser tab, with its own folder only" onClick={() => files.openInBrowser()}>Browser</Button> : null}
                  <Button size="sm" disabled={!files.canSave} loading={files.saving} loadingText="Saving…" onClick={() => { void files.save(); }}>Save</Button>
                </Cluster>
              </Cluster>
              {dirty(current) ? <Text variant="muted">Unsaved changes</Text> : null}
              <Cell grow>
                {isMarkdown(current.path) && files.mode === 'preview'
                  ? <MarkdownBlocks text={current.text} />
                  : <CodeView value={current.text} fileName={current.path} readOnly={!current.editable} aria-label={`Source of ${current.path}`} onChange={text => files.edit(text)} />}
              </Cell>
            </>
          ) : image ? (
            <Cluster gap="sm">
              <Text variant="muted">{image.path}</Text>
              <Button size="sm" variant="outline" onClick={() => setPicture(true)}>Show the picture</Button>
              <Lightbox open={picture} onOpenChange={setPicture} title={image.path} images={[{ id: image.path, src: image.dataUrl, alt: image.path }]} index={0} onIndexChange={() => {}} />
            </Cluster>
          ) : (
            <Empty title="Choose a file in the tree, or find one by name." />
          )}
        </Column>
      )}
    </Frame>
  );
}
