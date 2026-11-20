# Design system

> **The design system now lives in the Imago package** at `D:/Projects/imago` (repository
> `acaprino/imago`). The files in this folder stay until Shinatama migrates to it. New work reads
> Imago's documents: `docs/design-file.md` (the design file), `docs/desktop-shell.md` and
> `docs/using-imago.md`.

Shinatama's interface as a reusable design system, for Shinatama and for the other apps of the
package (Muro, Mukade, Xiox, Ethel).

| File | What it holds |
|---|---|
| [DESIGN.md](DESIGN.md) | The design file: colour, typography, spacing, radius and component tokens for Smooth dark, Smooth light, ONI MODE and the ONI skins, with the rules for using them. Lint-clean with zero-slop-design 0.2.1. |
| [desktop-shell.md](desktop-shell.md) | The glass technique on Windows 11 (acrylic, window theme, layers, measurements), the Tauri window techniques and the native-feeling components, with a reuse checklist. |
| `tokens/*.css` | The tokens as CSS custom properties (`--zsd-*`), one file per look. |
| `tokens/*.tokens.json` | The same tokens in DTCG 2025.10 JSON. |
| `tokens/*.report.json` | The export mapping reports. |

Shinatama's own code keeps its token source in `apps/desktop/src/presentation.ts`; each colour
bullet in DESIGN.md names the code variable a token comes from. A change to the interface's
colours or glass updates DESIGN.md and the exports in the same change.

## Use it in an app

1. Copy `tokens/` (or the looks the app needs) into the app and map `--zsd-*` to the app's names.
2. Follow [desktop-shell.md](desktop-shell.md), section 4, for the window and the glass.
3. Point the app's agent instructions at the design file, so every coding session reads it:

   `AGENTS.md`:

   ```markdown
   Before UI work, read docs/design-system/DESIGN.md and the component or style files it references.
   Apply its tokens and confirmed design choices within the requested scope.
   Report conflicts and keep unresolved choices explicit.
   ```

   `CLAUDE.md`:

   ```markdown
   For UI work, apply the design file and inspect the project files it references.

   @docs/design-system/DESIGN.md
   ```
