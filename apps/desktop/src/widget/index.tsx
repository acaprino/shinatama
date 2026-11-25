// WIDGET (placeholder, Phase A). The small page of the attention widget: the window that floats over the desktop when Shinatama is behind
// another app, always on top, never taking the focus (src-tauri/src/widget_window.rs builds it). It is made of Sigrid's Card, Avatar, Heading,
// Text and Button, and its text stays in the language it has today (Italian). Owned by the agent that fills the widget in.
import { Card, Text } from '@acaprino/sigrid/primitives';

export function Widget() {
  return (
    <Card>
      <Text>Shinatama</Text>
    </Card>
  );
}
