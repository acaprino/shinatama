import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "tsup";

export default defineConfig({
    entry: ["src/index.ts", "src/mcp.ts", "src/claude-mcp-entry.ts", "src/claude-native-bootstrap.ts", "src/agent/pi.ts", "src/agent/pi-native.ts", "src/agent/opencode.ts", "src/agent/opencode-native.ts"],
    format: ["esm"], target: "node20", platform: "node", outDir: "dist",
    clean: true, sourcemap: true, splitting: false, shims: false,
    esbuildOptions(options) {
        options.minifyWhitespace = true;
        options.alias = { "@earendil-works/pi-coding-agent": resolve(dirname(fileURLToPath(import.meta.url)), "src/agent/pi-host-stub.ts") };
    },
    noExternal: ["acp-kernel", "billion-context-pi-subagents", "fzstd", "node-forge", "semver", "tar", "undici", "jsonc-parser", "ws"],
    external: ["jimp", "mysql2"],
    banner: { js: "import { createRequire as __biliCreateRequire } from 'node:module';\nconst require = __biliCreateRequire(import.meta.url);" },
});
