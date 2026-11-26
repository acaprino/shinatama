// Settings, Appearance: the theme (the palette's: no switch of its own here), glass (the person's choice, in a window of its own only), the
// interface zoom (the shell's: a picker on `useWindow().setZoom`) and the terminal (font, size, line height, whether it follows the theme).
// Everything here applies as it changes and is saved a moment later (`SettingsController.setPresentation`).
import { useEffect, useState } from 'react';
import { useGlass, useWindow } from '@acaprino/imago';
import { useCommandPalette } from '@acaprino/sigrid/assemblies';
import { SettingRow } from '@acaprino/sigrid/components';
import { Button, Input, Select, Switch, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { SettingsController, TERMINAL_FONTS } from '../../controllers/settings.ts';

const LEVELS = Array.from({ length: 16 }, (_, i) => (5 + i) / 10);
const percent = (level: number) => `${Math.round(level * 100)}%`;

/** A whole number in a range, typed: it applies when it is in range, and says why when it is not. */
function NumberField({ label, value, problem, onValid, help }: { label: string; value: number; problem: (text: string) => string; onValid: (text: string) => void; help: string }) {
  const [text, setText] = useState(String(value));
  useEffect(() => { setText(String(value)); }, [value]);
  const bad = problem(text);
  return (
    <SettingRow
      label={label}
      description={bad ? <Text tone="danger">{bad}</Text> : help}
      control={(
        <Input
          aria-label={label} type="number" short="xs" inputMode="numeric" value={text} aria-invalid={bad ? true : undefined}
          onChange={event => { setText(event.target.value); if (!problem(event.target.value)) onValid(event.target.value); }}
          onBlur={() => { if (problem(text)) setText(String(value)); }}
        />
      )}
    />
  );
}

export function Appearance() {
  const app = useApp();
  const settings = useController(app.settings);
  const [glass, setGlass] = useGlass();
  const window_ = useWindow();
  const palette = useCommandPalette();
  const { presentation } = settings;
  const level = LEVELS.find(step => Math.abs(step - window_.zoom) < 0.001) ?? 1;
  return (
    <>
      <SettingRow
        label="Theme"
        description="Light or dark with the switch of the title bar; the system's look, with light and dark, is in the palette of every app. There are no skins."
        control={<Button variant="outline" size="sm" onClick={() => palette.setOpen(true)}>Open the commands</Button>}
      />
      {/* A browser has no glass: the setting is in a window of its own. */}
      {window_.own ? (
        <SettingRow
          label="Glass window"
          description="Windows' acrylic behind the page. Off by default; it changes at once. The same choice is the command Glass of the palette."
          control={<Switch aria-label="Glass window" checked={glass} onCheckedChange={setGlass} />}
        />
      ) : null}
      <SettingRow
        label="Interface zoom"
        description="Ctrl+= and Ctrl+- change it too; Ctrl+0 returns to 100%."
        control={(
          <Select
            aria-label="Interface zoom"
            size="sm"
            options={LEVELS.map(step => ({ value: String(step), label: percent(step) }))}
            value={String(level)}
            disabled={!window_.own}
            onValueChange={value => { void window_.setZoom(Number(value)).then(took => { if (!took) app.ui.info('This window cannot change its zoom.'); }); }}
          />
        )}
      />
      <SettingRow
        label="Terminal font"
        description="Local fonts, with a system monospace fallback."
        control={(
          <Select
            aria-label="Terminal font"
            size="sm"
            options={TERMINAL_FONTS.map(name => ({ value: name, label: name === 'profile' ? 'Default' : name }))}
            value={presentation.fontFamily}
            onValueChange={fontFamily => settings.setPresentation({ fontFamily })}
          />
        )}
      />
      <NumberField label="Terminal font size" help="From 8 to 32." value={presentation.fontSize} problem={SettingsController.sizeProblem} onValid={text => settings.setFontSize(text)} />
      <NumberField label="Terminal line height (%)" help="From 100 to 180." value={presentation.lineHeight} problem={SettingsController.heightProblem} onValid={text => settings.setLineHeight(text)} />
      <SettingRow
        label="Follow the interface theme in terminals"
        description="Changes the default and the ANSI colours with the theme. The colours an application writes in truecolour stay as they are."
        control={<Switch aria-label="Follow the interface theme in terminals" checked={presentation.followSkin} onCheckedChange={followSkin => settings.setPresentation({ followSkin })} />}
      />
    </>
  );
}
