# specs/

What the site's two pieces of real software are supposed to do, and what holds
them to it.

| File | Purpose |
| --- | --- |
| [PRD.md](PRD.md) | The site as a whole: users, architecture, what is checked and by whom, what is out of scope |
| [F01_ASK.md](F01_ASK.md) | Client-side semantic search over the portfolio |
| [F02_GAME.md](F02_GAME.md) | Helm, the Rust and WebAssembly trading simulation |

## Where this fits

| Document | Answers |
| --- | --- |
| [ARCHITECTURE.md](../ARCHITECTURE.md) | How the site is built, and why it is built that way |
| [AGENTS.md](../AGENTS.md) | The rules that bind a change, and how to verify one |
| [DESIGN.md](../DESIGN.md) | Tokens, palette, type |
| `specs/` | What the two features are for, and which check proves each claim |

## Why this is `README.md` and not `AGENTS.md`

The convention this folder is modelled on names the index `AGENTS.md`, so that AI
assistants load it without being asked. That slot is already taken here: the
repository root has an `AGENTS.md` holding the rules for changing the site, and
`CLAUDE.md` imports it with `@AGENTS.md`. A second auto-loading `AGENTS.md` that
is actually a table of contents would cause exactly the confusion the naming
convention exists to prevent. So this one is a plain `README.md`.

## Reading a spec

Each feature spec carries a `**Status:**` line, an Overview, the files that matter,
the architecture, acceptance criteria, and what was deliberately not built.

Acceptance criteria name their evidence. Where the evidence is a script or a test,
that check runs in CI and a failure is a real finding. Where it is marked
**Human**, no static check can reach it, and the criterion is written down
precisely because a green CI does not cover it. That distinction is the point of
the table.
