// The frame every panel of the inspector is drawn in: Sigrid's `Inspector` with the panel's title and its actions at the foot, and, when
// the panel covers another (the stack of `app.ui.panels`), a way back to the one it covers. The close and Escape give back the panel
// below, or close the inspector; a panel that holds work (the files) asks first through `onLeave`.
import type { ReactNode } from 'react';
import { Inspector as Side } from '@acaprino/sigrid/layout';
import { Button, Cluster, Column, Icon } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { PANEL_TITLES } from './spec.ts';

export function Frame({ title, footer, onLeave, fill, children }: { title: string; footer?: ReactNode; onLeave?: () => void | Promise<unknown>; fill?: boolean; children: ReactNode }) {
  const app = useApp();
  const ui = useController(app.ui);
  const below = ui.panels.length > 1 ? ui.panels[ui.panels.length - 2] : null;
  const leave = () => { void (onLeave ? onLeave() : ui.closePanel()); };
  return (
    <Side title={title} open onOpenChange={open => { if (!open) leave(); }} {...(footer ? { footer } : {})}>
      <Column gap="md" {...(fill ? { fill: true } : {})}>
        {below ? <Cluster><Button variant="ghost" size="sm" onClick={leave} aria-label={`Back to ${PANEL_TITLES[below.kind]}`}><Icon name="back" />{PANEL_TITLES[below.kind]}</Button></Cluster> : null}
        {children}
      </Column>
    </Side>
  );
}
