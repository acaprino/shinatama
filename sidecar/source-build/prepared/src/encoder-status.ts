// Whether the lazily imported image encoder has ever loaded in this process.
// Kept dependency-free so the capabilities report can read it without pulling
// the rescale pipeline in.

export type EncoderState = "not_observed" | "loaded" | "missing";

let state: EncoderState = "not_observed";

/** Run the dynamic import and record its outcome. Rejections are rethrown so the
 *  caller keeps its own degrade path. A load that already succeeded stays loaded. */
export async function trackEncoderImport<T>(load: () => Promise<T>): Promise<T> {
    try {
        const mod = await load();
        state = "loaded";
        return mod;
    } catch (err) {
        if (state !== "loaded") state = "missing";
        throw err;
    }
}

/** `loaded` after one successful import, `missing` after a failed one, `not_observed` before any attempt. */
export function encoderState(): EncoderState {
    return state;
}

export function resetEncoderStateForTest(): void {
    state = "not_observed";
}
