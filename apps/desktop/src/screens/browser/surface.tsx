// The page itself: the surface the frames of the page are drawn on, and where the person's pointer, wheel and keys are taken.
import { useEffect, useRef } from 'react';
import { FrameSurface, type FrameSurfaceHandle } from '@acaprino/sigrid/components';
import { useApp } from '../../hooks.ts';
import { frameKeyInput, framePointerInput, frameWheelInput, keepsForApp } from '../../browser-input.ts';

export function PageSurface({ viewId, visible }: { viewId: string; visible: boolean }) {
  const app = useApp();
  const surface = useRef<FrameSurfaceHandle>(null);
  useEffect(() => app.browser.connect(viewId, bytes => { void surface.current?.draw(bytes); }), [app, viewId]);
  useEffect(() => { app.browser.setVisible(viewId, visible); }, [app, viewId, visible]);
  return (
    <FrameSurface
      ref={surface}
      label="Web page"
      onSize={size => app.browser.resize(viewId, size)}
      onPointer={event => app.browser.input(viewId, framePointerInput(event))}
      onWheel={event => app.browser.input(viewId, frameWheelInput(event))}
      onKey={event => app.browser.input(viewId, frameKeyInput(event))}
      onPaste={text => app.browser.input(viewId, { type: 'text', text })}
      appKeys={keepsForApp}
    />
  );
}
