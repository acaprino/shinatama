# shinatama

Shinatama: a local-first terminal runtime in Rust that hosts Claude Code, Codex, OpenCode and Pi, and hops a live chat between them.

Status: design draft, not approved. No code yet.

- Functional design: [docs/functional-design.md](docs/functional-design.md)
- Technical design (threads, hop, exchange between instances, shared state): [docs/technical-design.md](docs/technical-design.md)
- Browser integration (Moli fast path, explicit Patchright UI, routing, security and lifecycle gates; proposed 2026-10-29): [docs/browser-integration.md](docs/browser-integration.md)
- Mukade integration for native session handover (API, host responsibilities and gates): [docs/mukade-handover.md](docs/mukade-handover.md)
- Native support for Daodan (decided 2026-10-27; proposed, not approved): [docs/daodan-native-support.md](docs/daodan-native-support.md)
- Session import/export: the library is now its own repository, [mukade](https://github.com/acaprino/mukade), which ships the spec ([session-interchange-spec.md](https://github.com/acaprino/mukade/blob/master/docs/session-interchange-spec.md)) and the tool tables ([tool-conversion-tables.md](https://github.com/acaprino/mukade/blob/master/docs/tool-conversion-tables.md)); technical design 5.4.1, register row H27.
- What still has to be proved by running something: [docs/verification-register.md](docs/verification-register.md)

Naming (decided 2026-10-26): the project is called `shinatama` (earlier suffixes `-sld` and `-mux` were dropped). The obsolete Daodan marketplace checkout (`acaprino/shinatama`, register row D1) was deleted on 2026-10-26; the local folder is renamed to `D:\Projects\shinatama` once JanuraCode is not holding it, because its services keep `.jcode` open.

D1 (closed 2026-10-27, [record](docs/verification/2026-10-27-D1.md)): the **remote** `acaprino/shinatama` repository turned out to be public and alive (MIT; last push 2026-10-25; 1 011 commits, 17 tags, 24 plugins) although the local checkout was gone. The user decided that only Daodan is the plugin marketplace, so the remote repository was **deleted on 2026-10-27** after a verified bundle backup (`C:\Users\alfio\Backups\acaprino-shinatama-2026-10-27.bundle`, plus a mirror clone beside it). Every one of its 24 plugins exists in Daodan at an equal or newer version. The name collision with this product is gone. Left behind: prose that still names the repository in Daodan's `README.md` and `AGENTS.md` (history of marketplace 36.0.0) and a dead link in Mukade's `README.md` line 8.
