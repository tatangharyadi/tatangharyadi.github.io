# specs/

What the site's two pieces of real software are supposed to do, and what holds
them to it.

**These are reconstruction specifications.** The bar the two feature specs are
written to is that someone with this folder and no repository could rebuild the
Ask page and the game in the same shape: the same corpus format, the same
exported ABI, the same byte encodings, the same DOM ids. That is why they state
constants rather than describe them, and why they repeat things that also appear
in [ARCHITECTURE.md](../ARCHITECTURE.md). A specification you have to read
another document to act on is not one.

The repetition has a cost, and it is worth naming rather than pretending away: a
constant written in two places can drift. The mitigation is that almost every
value in these specs is one a check in `scripts/` or a test in `game/` already
holds to the source, so drift shows up as a CI failure against the code long
before anyone notices the prose. Where a value is *not* mechanically held, the
spec says where it lives instead of copying it. The measured download figures in
F01 are the worked example: they stay in `ARCHITECTURE.md` because they are
measurements that will move.

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

Each feature spec carries a `**Status:**` line, a statement of what the document
is for, an Overview, the files that matter, the interfaces in full, the rules and
their constants, acceptance criteria, and what was deliberately not built.

**Interfaces are specified exhaustively; rules are specified by shape and
constant.** An interface is a seam two halves have to meet at exactly, so a spec
that approximates one is useless. A rule set is thousands of lines of branching,
and transcribing it would produce a worse copy of the source that goes stale the
first time the source changes. So the specs give every export signature, every
byte encoding, every JSON key and every id, and for the rules they give the
formula's shape, the constants that tune it, and the module that holds the
arithmetic.

Acceptance criteria name their evidence. Where the evidence is a script or a test,
that check runs in CI and a failure is a real finding. Where it is marked
**Human**, no static check can reach it, and the criterion is written down
precisely because a green CI does not cover it. That distinction is the point of
the table.
