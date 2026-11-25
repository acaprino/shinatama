// Files, folders and images into a terminal. Every agent reads a path, so each becomes one: a dropped or Explorer-copied file is its own
// path, a clipboard image is saved by the native side under `<home>\pastes` first. The paths go in as one paste, so the agent sees a
// paste. This file is the pure part (how paths are written for whoever reads the line); the listening is `screens/workspace/terminal-io.ts`.

/** The native side refuses larger images; the page does not read or send them. */
export const IMAGE_LIMIT = 64 * 1024 * 1024;
/** Image types every agent reads; any other image is converted to PNG first. */
const READABLE_IMAGES = new Set(['image/png', 'image/jpeg', 'image/gif', 'image/webp']);
export const isReadableImage = (type: string): boolean => READABLE_IMAGES.has(type);

/**
 * Who reads the pasted line. `powershell`: a PowerShell prompt, where double quotes still expand `$` and the backtick, so paths go in
 * single quotes. `agent`: an agent's prompt, which reads plain double-quoted paths.
 */
export type PathReader = 'powershell' | 'agent';

/** The reader of a terminal, from the shell its session reports. */
export const readerOf = (shell: string): PathReader => shell === 'PowerShell' ? 'powershell' : 'agent';

/** Paths as the reader takes them: quoted when they hold a space or a character its syntax reads. */
export function formatPaths(paths: readonly string[], reader: PathReader = 'agent'): string {
  const quoted = paths.filter(path => path.length > 0).map(path => {
    if (reader === 'powershell') return /[\s&()[\]{}^=;!'+,`~$#@%|<>"]/.test(path) ? `'${path.replaceAll("'", "''")}'` : path;
    return /[\s&()[\]{}^=;!'+,`~$#@%|<>]/.test(path) ? `"${path}"` : path;
  });
  return quoted.length ? `${quoted.join(' ')} ` : '';
}
