// Pure helpers on folder paths, as the windows of Windows write them (either slash, a `\\?\` prefix, any case).

/** The last part of a folder's path: its name. */
export const folderName = (path: string): string => path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || path;

const key = (path: string): string => path.replace(/^\\\\\?\\/, '').replace(/[\\/]+$/, '').toLowerCase();

/** Whether two paths are the same folder. */
export const samePath = (a: string, b: string): boolean => key(a) === key(b);
