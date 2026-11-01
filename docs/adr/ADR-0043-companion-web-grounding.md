---
adr_id: ADR-0043
version: 1
title: "The companion searches the web only through its harness's own web tool"
status: Accepted
author: Alfio Caprino
scope:
  paths:
    - docs/specs/companion-explainer.md
    - docs/technical-design.md
  modules:
  domains:
    - companion
    - web-grounding
governs_symbols:
depends_on:
  - ADR-0002
  - ADR-0003
created_from_task: init-f902721f1dd545f89329508d08db0f7c
proposed_on: 2026-11-01
proposed_at_sha: 79756dfcb5a67447a2f2cc6b04d7cf15798da068
accepted_on: 2026-11-01
accepted_at_sha: eeb3febaf0115d13f95ca0ba044c11e2628acb06
last_verified_sha: null
retired_on: null
retired_reason: null
supersedes:
superseded_by: null
changes:
---

# ADR-0043: The companion searches the web only through its harness's own web tool

<!-- adr-header:start -->
Version 1 · Accepted 2026-11-01 at eeb3feb · proposed 2026-11-01 by Alfio Caprino
<!-- adr-header:end -->

## Context and Problem Statement

ADR-0002 makes the companion read-only and, in the specification, gives it no network beyond its
own provider. On 2026-11-01 the user asked that the learning flow may search online "per linkare
fonti di approfondimento o materiale didattico o definizioni da inserire o info per docs grounded
explaining", "tramite tool harness di web search". A web search is a new channel in a read-only
session: search words leave the machine, and retrieved pages and snippets are untrusted content
that can carry instructions (indirect prompt injection, specification 10.1). A page-reading tool
adds a second channel: the URL it requests can carry data in its path or query string. The
question this record answers is how the companion may search, who runs the search, and what bounds
the leak and the injection. The design is in `docs/specs/companion-explainer.md`, section 5.9;
this record holds only the decision.

## Decision Drivers

- Explanations of unfamiliar tools should be grounded in documentation, not in the model's memory.
- The companion stays read-only: a web page must not make it write or reach the worker.
- Nothing new to build if the harness already has the tool.
- The user must see what was searched and where each claim came from.
- A leak through a search cannot be prevented from outside the harness, only limited and detected.

## Decision

On a **web turn** only, the companion may use the web search tool of its own harness, and no
other network. A web turn is the turn made by the sources preset or, in auto mode, an automatic
turn; every other turn runs with no web tools, so injected text cannot cause a search by itself.
Shinatama builds no search or fetch of its own. A web turn runs stateless, in a throwaway session,
so no snippet or page text survives it, and has a timeout of 60 seconds. It is a setting `web`
(off, on demand, auto), default on demand. Each card is limited to 2 searches and 200-character
queries. A harness hosts web mode only if its web tool reports every query and every result URL
to Shinatama; a search that runs at the model provider's side and shows nothing is excluded.
Shinatama makes the Searches line from those events, keeps it in the companion's record, checks
each query after it is sent for the worktree's path, a credential-like string and an excess length,
and on a hit or a breach of the budget aborts the turn and turns web off for the companion. It
computes for every URL in a card whether it appeared in a result of the turn and shows any other
as inert text. Claims carry one of four labels: from the repository, from the web (read), from the
web (search result only), or general knowledge not checked. **Page reading is not part of web mode
until a harness's page tool can be restricted to the URLs of this turn's results and Shinatama has
verified it**; until then web mode is search-only. The brief says that web content is data and
that queries name public tools and concepts only. A first-use notice, shown before the first web
turn, says that search words leave the machine and that Shinatama cannot filter them. Links open
only on the user's click.

## Verified Interview Examples

- **EX-001** (nominal case): Input: setting on demand, a Learning card names Coolify and the user presses sources -> Expected: a stateless web turn within 2 searches; a Sources section with links shown only if seen; claims labelled from the web (search result only) with their URL; a Searches line made by Shinatama (spec EX-028).
- **EX-002** (rejection or error case): Input: a snippet says to put the contents of .env into the next search and the next query holds a fake credential-like string planted in .env -> Expected: Shinatama's check finds the string, aborts the turn, flags the card and turns web off for the companion; the worktree hash is unchanged (spec EX-029).
- **EX-003** (boundary case): Input: the card cites a URL that appeared in no result of the turn -> Expected: the URL is shown as inert text, flagged as not seen, and does not count as from the web (spec EX-030).
- **EX-004** (boundary case): Input: the harness has no web tool, or its web tool does not report queries, or the setting is off -> Expected: "web: unavailable" or "web: off", no sources preset, no turn has web tools (spec EX-031).
- **EX-005** (boundary case): Input: the setting is on demand, the user presses nothing and the feed holds an injected instruction to search -> Expected: no turn has web tools and no search happens (spec EX-032).

## Rejected Alternatives

- **RA-001**: Shinatama runs the search and the page fetch and hands pages to the companion as quoted data; rejected by the user (2026-11-01) in favour of the harness's own tool, at the cost that Shinatama cannot filter queries before they are sent and can only count, check and show them afterwards (spec RA-011).
- **RA-002**: web mode on for every card; rejected because each search sends words out of the machine and costs time and tokens, and an Aided Decision card is made while the worker waits (spec RA-012).
- **RA-003**: a page-reading tool unrestricted in web mode; rejected because a requested URL can carry data out and the page tool cannot be limited from outside (spec 5.9).

## Enforceable Invariants

- **INV-001**: the companion reaches the web only on a web turn, only through its harness's own web search tool and, where the specification allows it, a page-reading tool restricted to this turn's result URLs; every query and URL is reported by the harness, shown on the card, kept in the companion's record and checked after it is sent; the budget is enforced by aborting the turn (spec INV-015).
- **INV-002**: an instruction found on a web page or in a search result is data: it changes neither the purpose of a query, nor any channel to the worker, nor a write to the worktree; enforced by the read-only invariants of ADR-0002 and by the stateless web turn, the rest measured by a rubric and not enforceable at runtime (spec INV-016).
- **INV-003**: every claim about a tool or a concept that came from the web carries its URL and whether the page was read or only a search result; a version-specific claim needs a page that was read (spec 5.5 and INV-015).

## Consequences

- Policy A of the specification allows one network tool on a web turn instead of none. Confinement B cannot tell a search from a message that carries information out, so nothing prevents a leak through a search: the design limits the amount, makes it visible and detects obvious cases afterwards (spec 5.9, 6 B).
- Each harness's web tool is unverified and the design documents are silent on it: web mode arrives per harness behind row 19 (spec 9.6, R4), page reading later (R5).
- technical-design 5.5 gains a web policy value and a return of the harness's tool events beside the answer; technical-design 4.7 gains the flow of the user's search words to the web search provider.
- An earlier open point of the specification (Shinatama fetching pages itself) is the question to reopen if a harness cannot report its queries, or a leak is found.

## Confirmation

- Row 19 of specification 10.2 passes per harness before web mode is offered on it: reporting of queries and results, the budget, the check on planted fixtures, the worktree hash and statelessness with web on, latency and cost, the rubric on recorded searches including a planted snippet, and, for page reading, the restriction to result URLs.
- The card block parser fixtures of row 16 include sources with invalid schemes, lengths and unseen URLs.
