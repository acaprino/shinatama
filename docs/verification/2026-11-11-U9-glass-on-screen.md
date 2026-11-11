# U9 — the desktop glass as it is on screen

Item: U9 (verification register, section 14). Date: 2026-11-11. Status: partial.
Machine: Windows 11 Home 10.0.26200, system in the light theme. Build: debug
`shinatama-desktop.exe` with `custom-protocol`, Shinatama skin, PowerShell session with the
Neutral terminal profile, terminal background at 58%.

## What was asked

The user asked for frosted glass on the whole window, in the skin's colours, and then reported
two defects by eye on the running app: "the console is unreadable" and "the colours of the UI
are too washed out". The question is what the window looks like on the real screen, and whether
text on it keeps its contrast.

## Method

`node apps/desktop/tests/glass-capture.mjs` starts the real app with an isolated data folder,
opens a PowerShell session from the harness menu, prints text in the default and ANSI colours,
raises the window above the others and copies it from the screen. It prints the colour of one
point of the terminal background, of the title bar and of the sidebar. A webview screenshot
cannot be used: it draws the acrylic backdrop as white. Contrast is the WCAG ratio against the
skin's `text` (#e6f0ff) and `muted` (#8fa6c9) tokens.

## Results

| Window theme | Behind the window | Terminal | Title bar | Sidebar | text | muted |
|---|---|---|---|---|---:|---:|
| Light (follows the system) | dark terminal window | 63,65,70 | 42,45,58 | 60,64,78 | 8.9 | 4.1 |
| Light (follows the system) | white window | 98,100,105 | 63,66,79 | 88,92,106 | 5.2 | 2.4 |
| Dark (`theme: Dark`) | dark terminal window | 38,40,45 | 29,32,45 | 40,44,58 | 12.8 | 5.9 |

Solving the first row for the backdrop gives about 143 on 255 behind a 58% terminal, and 226
over white: in the light theme the acrylic is a light grey even over a dark window. In the dark
theme the same calculation gives about 84. The `window-vibrancy` 0.6.0 source sets the system
backdrop on this Windows build and does not use the tint colour of `tauri.conf.json`; the
measured backdrops are consistent with that. Changing the tint was not tested.

Before these captures a new terminal painted no background at all (only a restored terminal
and a profile change did), so its text sat directly on the backdrop; fixed in `7f6a3ae`.

## Conclusion

- The cause of both reports is the light system backdrop under dark surfaces. `theme: Dark`
  on the window removes it; the user then confirmed the result by eye ("now it is perfect").
- On Windows 11 the acrylic backdrop cannot be coloured from the window configuration.
- In the dark theme over a dark window, text and muted text pass 4.5:1.

## Not measured

- A bright window behind the dark-theme glass: the capture with a white window placed behind
  was unreliable (the app window did not stay on top) and was removed from the script.
- Section 9, TCTF, Sumi and Neutral dark on glass; the other terminal profiles.
- Windows 10, where the tint colour applies and the backdrop is a different effect.
