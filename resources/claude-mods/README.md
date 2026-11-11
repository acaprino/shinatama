# Claude Code mods

Display-only extensions that Shinatama hands to Claude Code alone, next to the
bundled plugins in `../agent-plugins`. They are kept apart on purpose
(`crates/engine/src/claude_mods.rs`): their own folder and their own lock, so a
mod that is missing, altered or not tried with the running Claude Code is
dropped alone. The bundled plugins and Codex, Pi and OpenCode launch exactly as
without it, and a mod never blocks a launch.

## shn-render

Draws LaTeX math and simple Mermaid flowcharts in Claude's answers as Unicode
text. It rewrites only what the terminal draws: the stored answer, the next
model request and `ctrl+o` keep the original. Every change is checked by parsing
the whole answer again (marked, GFM); a change is kept only when code, HTML,
link addresses and the structure stay exactly as they were. Mermaid is drawn
only for top-level blocks in a strict grammar, as a tree (left to right with at
most one branch, top-down only as a chain); anything else stays code.

- Loaded only with the Claude Code versions in `TRIED_CLAUDE_VERSIONS`
  (`claude_mods.rs`). Claude's mod interface is early access: add a version
  only after running Claude with the mod on it.
- Off: `SHINATAMA_CLAUDE_MODS=off`, or `SHINATAMA_BUNDLED_PLUGINS=off` (both).
- Not in the portable package yet: `scripts/package-local.ps1` copies only
  `agent-plugins`. The development build (`Shinatama.cmd`) uses this folder.

Tests (Node 22 or later, from this folder):

```sh
node --experimental-strip-types --test tests/rewrite.test.ts tests/rewrite2.test.ts tests/rewrite3.test.ts tests/rewrite4.test.ts
```

`hooks/lib/markdown.js` and `hooks/lib/mermaid.js` are bundles of marked 18.0.11
(`Lexer.lex(text, { gfm: true })`) and of beautiful-mermaid 1.1.3
(`src/ascii/index.ts` `renderMermaidASCII` and `src/parser.ts` `parseMermaid`),
built with rolldown as single ES modules. Licences: `shn-render/THIRD-PARTY-NOTICES.md`.

After any change to a file under `shn-render/`, regenerate `mods.lock.json`
(bytes and SHA-256 of every file, sorted by path):

```sh
python -I -c "import hashlib,json,os;r='shn-render';f=sorted(({'path':os.path.relpath(os.path.join(d,n),r).replace(os.sep,'/'),'bytes':len(b:=open(os.path.join(d,n),'rb').read()),'sha256':hashlib.sha256(b).hexdigest()} for d,_,ns in os.walk(r) for n in ns),key=lambda x:x['path']);open('mods.lock.json','w').write(json.dumps({'formatVersion':1,'mods':{r:{'version':json.load(open(r+'/.claude-plugin/plugin.json'))['version'],'files':f}}},indent=2)+'\n')"
```
