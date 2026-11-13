// Names written before the product was renamed from Hornyness to Musashi
// Terminal. The current names are always the ones written; the old ones are
// still read, so a launcher, an environment or a session store from before the
// rename keeps working.

const LEGACY_ENV_PREFIX = "HORNYNESS_";
const ENV_PREFIX = "MUSASHI_TERMINAL_";
const LEGACY_METADATA_PREFIX = "hornyness";
const METADATA_PREFIX = "musashiTerminal";

/** A variable set by the launcher, read under its current name first, then
 *  under the name an older launcher sets. */
export function launcherEnv(name: string, env: NodeJS.ProcessEnv = process.env): string | undefined {
    return env[ENV_PREFIX + name] ?? env[LEGACY_ENV_PREFIX + name];
}

/** Session metadata with every key written under the old product name moved
 *  to its current key. A current key wins over its legacy twin. */
export function adoptLegacyMetadata(metadata: Record<string, unknown>): Record<string, unknown> {
    for (const key of Object.keys(metadata)) {
        const rest = key.slice(LEGACY_METADATA_PREFIX.length);
        if (!key.startsWith(LEGACY_METADATA_PREFIX) || !/^[A-Z]/.test(rest)) continue;
        const current = METADATA_PREFIX + rest;
        if (!(current in metadata)) metadata[current] = metadata[key];
        delete metadata[key];
    }
    return metadata;
}
