// The page itself: the surface the frames of the page are drawn on, and where the person's pointer, wheel and keys are taken.
//
// pending Sigrid frame-surface: Sigrid 6.11.0 has no part that draws pictures pushed to it. `FrameSurface` is built (branch
// `claude/frame-surface` of acaprino/sigrid, components entry) and waits for a release. Until the app pins it this draws the page's
// place and wires what does not depend on the part (the tab is connected, in sight, and sends nothing). With the release, the body of
// `PageSurface` becomes:
//
//   const surface = useRef<FrameSurfaceHandle>(null);
//   useEffect(() => app.browser.connect(viewId, bytes => { void surface.current?.draw(bytes); }), [app, viewId]);
//   useEffect(() => { app.browser.setVisible(viewId, visible); }, [app, viewId, visible]);
//   return (
//     <FrameSurface
//       ref={surface}
//       label="Web page"
//       onSize={size => app.browser.resize(viewId, size)}
//       onPointer={event => app.browser.input(viewId, framePointerInput(event))}
//       onWheel={event => app.browser.input(viewId, frameWheelInput(event))}
//       onKey={event => app.browser.input(viewId, frameKeyInput(event))}
//       onPaste={text => app.browser.input(viewId, { type: 'text', text })}
//       appKeys={keepsForApp}
//     />
//   );
//
// (`FrameSurface` and `FrameSurfaceHandle` from '@acaprino/sigrid/components'; the three adapters are in browser-input.ts.)
import { useEffect } from 'react';
import { Icon, Text } from '@acaprino/sigrid/primitives';
import { Empty } from '@acaprino/sigrid/components';
import { useApp } from '../../hooks.ts';

export function PageSurface({ viewId, visible }: { viewId: string; visible: boolean }) {
  const app = useApp();
  useEffect(() => app.browser.connect(viewId, () => {}), [app, viewId]);
  useEffect(() => { app.browser.setVisible(viewId, visible); }, [app, viewId, visible]);
  return (
    <Empty fill icon={<Icon name="globe" />} title="Page" description={<Text variant="note">The page appears here once Sigrid&apos;s FrameSurface is released.</Text>} />
  );
}
