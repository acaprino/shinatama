// The browser tab: one page that Rust drives through the browser engine, shown in a pane (ADR-0056). A bar with Back, Forward, Reload, the
// address and the system's browser; a strip that says when an agent drives the page and offers to take it over; and the page itself, drawn
// from the frames Rust sends. The tab renderer of the workspace mounts `BrowserTab` for a view of kind `browser`. What the tab remembers and
// sends is `app.browser` (controllers/browser.ts); this file only draws it.
import { useEffect, useState } from 'react';
import { Button, Cell, Cluster, Column, Icon, Input } from '@acaprino/sigrid/primitives';
import { Empty, Note } from '@acaprino/sigrid/components';
import { useApp, useController } from '../../hooks.ts';
import { BROWSER_WORDS, controlStrip } from '../../browser-model.ts';
import { PageSurface } from './surface.tsx';

export interface BrowserTabProps {
  /** The view of the tab (`browser-<page>`). */
  readonly viewId: string;
  /** Whether the tab is the one in sight in its pane: Rust draws no frames for a page nobody sees. Default true. */
  readonly visible?: boolean;
}

export function BrowserTab({ viewId, visible = true }: BrowserTabProps) {
  const app = useApp();
  const browser = useController(app.browser);
  const page = browser.pages.get(viewId);
  if (!page) return <ClosedPage viewId={viewId} />;
  return <OpenPage viewId={viewId} visible={visible} />;
}

/** A tab the layout kept whose page is not open (after a restart): its last address and a button that opens it again. */
function ClosedPage({ viewId }: { viewId: string }) {
  const app = useApp();
  const saved = app.browser.savedPage(viewId);
  return (
    <Empty
      fill
      icon={<Icon name="globe" />}
      title={saved?.title || saved?.url || BROWSER_WORDS.closed}
      description={saved?.title ? saved.url : BROWSER_WORDS.closed}
      action={saved ? <Button variant="outline" size="sm" onClick={() => { void app.browser.reopen(viewId); }}>{BROWSER_WORDS.reopen}</Button> : undefined}
    />
  );
}

function OpenPage({ viewId, visible }: Required<BrowserTabProps>) {
  const app = useApp();
  const browser = useController(app.browser);
  const page = browser.pages.get(viewId)!;
  const strip = controlStrip(page);
  // What the person is typing; the address of the page shows again when the field is left or the page moves on.
  const [typed, setTyped] = useState<string | null>(null);
  useEffect(() => { setTyped(null); }, [page.url]);
  const go = async () => { if (typed !== null && await browser.go(viewId, typed)) setTyped(null); };
  return (
    <Column fill>
      <Cluster wrap={false} gap="xs" pad="sm" role="group" aria-label="Browser navigation">
        <Button variant="ghost" size="icon-sm" aria-label="Back" title="Back" onClick={() => { void browser.navigate(viewId, 'back'); }}><Icon name="back" /></Button>
        <Button variant="ghost" size="icon-sm" aria-label="Forward" title="Forward" onClick={() => { void browser.navigate(viewId, 'forward'); }}><Icon name="chevron" /></Button>
        <Button variant="ghost" size="icon-sm" aria-label="Reload" title="Reload" onClick={() => { void browser.navigate(viewId, 'reload'); }}><Icon name="refresh" /></Button>
        <Cell grow>
          <Input
            name="address"
            aria-label="Address"
            spellCheck={false}
            value={typed ?? page.url}
            onChange={event => setTyped(event.target.value)}
            onBlur={() => setTyped(null)}
            onKeyDown={event => { if (event.key === 'Enter' && !event.nativeEvent.isComposing) { event.preventDefault(); void go(); } }}
          />
        </Cell>
        <Button variant="ghost" size="icon-sm" aria-label="Open in the system browser" title="Open in the system browser" onClick={() => { void browser.external(viewId); }}><Icon name="external" /></Button>
      </Cluster>
      {strip ? (
        <Note
          tone="info"
          title={strip.text}
          announce={false}
          actions={<Button variant="outline" size="sm" onClick={() => { void browser.control(viewId, strip.action === 'take'); }}>{strip.action === 'take' ? BROWSER_WORDS.takeControl : BROWSER_WORDS.giveBack}</Button>}
        />
      ) : null}
      <Cell grow><PageSurface viewId={viewId} visible={visible} /></Cell>
    </Column>
  );
}
