// Presentation state only. Never store terminal IDs, output, project paths or process handles.
export type Layout = { version: 1; axis: 'columns' | 'rows'; ratio: number };
export const DEFAULT_LAYOUT: Layout = { version: 1, axis: 'columns', ratio: 0.5 };
export function parseLayout(raw: string | null): Layout {
  try {
    const value = JSON.parse(raw ?? 'null');
    if (value?.version === 1 && ['columns', 'rows'].includes(value.axis)
      && typeof value.ratio === 'number' && Number.isFinite(value.ratio) && value.ratio >= 0.2 && value.ratio <= 0.8) {
      return { version: 1, axis: value.axis, ratio: value.ratio };
    }
  } catch { /* Corrupt preferences cannot prevent the workspace from opening. */ }
  return { ...DEFAULT_LAYOUT };
}
export function boundedRatio(value: number): number { return Math.max(0.2, Math.min(0.8, value)); }
