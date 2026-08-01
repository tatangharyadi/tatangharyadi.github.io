# Architecture

How this site is put together, and why. For the rules that keep it working, see
[AGENTS.md](AGENTS.md).

## No build step

Plain HTML and CSS plus one runtime library, served directly by GitHub Pages from
`main`. There is no bundler, no package manager, and no lockfile.

This is deliberate rather than incidental. The site is a personal homepage whose
content changes a few times a year; a toolchain would need reviving every time,
and a dependency tree would need patching in between. The tradeoff accepted in
exchange is that shared values live in CSS custom properties instead of build-time
constants, and the three external resources load from a CDN at runtime:

- [Poppins](https://fonts.google.com/specimen/Poppins) via Google Fonts
- [Boxicons](https://boxicons.com/) 2.1.4 via unpkg
- [htmx](https://htmx.org/) 2.0.10 via unpkg

htmx is a `<script>` tag, not a dependency in the package-manager sense: pinned to
an exact version, verified by an SRI digest, and loaded the same way Boxicons is.
Nothing installs it and nothing builds against it. The rule this repo actually
holds is *no toolchain*, and adding a library that is one URL does not breach it —
but note the site now has a script it did not have before, and if unpkg is
unreachable the carousel stops advancing. Everything that matters
for reading the page is in the initial HTML, which is why that degradation costs
interaction and not content.

Fonts are loaded with `<link rel="preconnect">` plus `<link rel="stylesheet">` in
`index.html`, **not** `@import` in the stylesheet. An `@import` would serialize the
request chain — the browser must parse `style.css` before it discovers the font
request — and delay first paint.

## Layout

```
index.html              home: nav + hero, case study carousel
portfolio.html          case studies in full, plus now/skills/earlier; the Ask
                        source. Not in the nav: arrived at, not picked
ask.html                browser-local semantic search over portfolio.html
corpus.json             generated search index: one embedding per portfolio passage
404.html                self-contained not-found page
css/style.css           all styles, including the carousel slide animations
js/ask.js               the only first-party script; see The Ask page
fragments/              the HTML states htmx fetches; pieces of a page, not pages
  casestudy/            r{0..3}-{next,prev}.html, one per rotation and direction
  404-links.html        section shortcuts, loaded by 404.html
vendor/transformers/    Transformers.js and the ORT WebAssembly, served from origin
assets/models/          all-MiniLM-L6-v2 weights and tokenizer, likewise
assets/favicon.svg      favicon
assets/images/          profile photo (webp) and the 1200x630 social share card
.nojekyll               opt out of Jekyll processing (see Deployment)
robots.txt, sitemap.xml crawler hints
.github/workflows/      ci.yml, the invariant checks (see Continuous integration)
scripts/                stdlib-Python checkers run by CI; not a build step
  build-corpus.html     the exception: a page, because generating corpus.json needs
                        a browser. Run by hand, not by CI. See The Ask page
ARCHITECTURE.md         this file
DESIGN.md               the Catppuccin palette, semantic tokens and contrast floors
                        (in the google-labs-code/design.md format)
AGENTS.md               working rules and invariants
CLAUDE.md               one line, imports AGENTS.md
LICENSE                 MIT, code only
```

**The nav offers three destinations and the site has four pages.** `portfolio.html`
is not a menu choice. It holds the prose everything else is built out of, and the
two things that read it are the search on `ask.html` and a crawler, neither of which
uses a nav. A visitor reaches it from an Ask result, from the link under the case
study deck, from the link below the results on `ask.html`, or from a search engine.

Leaving it in the nav made the site ask the visitor to choose between reading the
portfolio and searching it, which is a choice with an obviously better answer and no
reason to be offered. Taking it out costs nothing measurable: the page is still in
`sitemap.xml`, still linked from three places, and still the only crawlable copy of
the detail. What it changes is what the site puts forward, which is the search. That
is also why nothing in that page's own nav carries `aria-current`.

`404.html` intentionally duplicates its styles inline. It has to render correctly
even if `css/style.css` is the thing that failed to load, so its only external
references are the favicon and a `Poppins` font-family that degrades to a system
sans-serif rather than being fetched. The cost of that independence is that its
palette is a copy of the one in `css/style.css` and has to be kept in step by hand.

## Design tokens

Everything themeable lives in the `:root` block at the top of `css/style.css`,
with a `prefers-color-scheme: dark` block immediately after it holding the dark
overrides. Colour is [Catppuccin](https://catppuccin.com) — Latte in light mode,
Mocha in dark. **[DESIGN.md](DESIGN.md) is the reference for which token to reach
for and what contrast each one is verified at**; the rules below are the
constraints that are not about colour at all:

- `--casestudy-slide-duration` drives the slide animations and nothing else reads
  it any more, so its unit is now free. It used to be parsed by JavaScript, which
  is why older comments insisted on milliseconds.
- The `--casestudy-item1/2/3-*` tokens are not styling knobs — they are animation
  state, and they are deliberately *not* duplicated in the dark block because they
  are flavour-independent. See below.
- `color-scheme: light dark` on `:root` is what makes native scrollbars and form
  controls follow the active flavour. Removing it leaves them light under Mocha.

One colour constraint is worth repeating here because it has already been
rediscovered once: the accent exists in two variants, and they are not
interchangeable. `--accent` is decoration only; `--accent-text` is for text and
every focus ring. See [DESIGN.md](DESIGN.md#the-accent-rule).

## Interaction patterns

Interaction is hypermedia. Every state of the case study carousel is a file under
`fragments/`, and a control asks for the state it moves to. There is no
client-side state to get out of step with the markup, because the markup *is* the
state: a fragment says which case studies are in which slot and where each control
points next.

The htmx example that shapes this is a click-to-load swap for the carousel. The
related [tabs (hateoas)](https://htmx.org/examples/tabs-hateoas/) documentation
says that pattern "requires dynamic server-side routing", which is true of a
templated server and not true here: each response is fixed and depends on nothing
about the request, so a static file serves it exactly. That is the whole reason
this works on GitHub Pages, which cannot compute a response or set a header.

The hero used to be a second instance of this, three tabs under `fragments/hero/`.
It was removed once the Ask page existed: two of the three panels were answers to
questions Ask can field, and the third was the positioning statement, which was
never one option among three. The hero is now a single paragraph in `index.html`.

- **Case study carousel** swaps the whole deck. See the section below.
- **Mobile sidebar** is still the pure-CSS checkbox hack: a single `<input
  type="checkbox">` toggles the menu, with a full-screen `<label>` as the
  click-away overlay. It stays CSS because it has nothing to fetch — there is no
  state to get from a server, and htmx would only add a network round trip to
  something that already works offline.
- **The Ask page is the one exception**, and it is worth being precise about why.
  Every pattern above works because the state being requested is markup that
  already exists somewhere. Semantic search has no such file: the answer is a
  matrix multiply against 384-dimensional vectors computed in the visitor's tab
  from a model running there. Serving it as a fragment would mean sending the
  question to a server, which is the exact property the page is built to avoid. So
  `js/ask.js` is a plain ES module — no bundler, no package manager, consistent
  with the rest of the repo — and it is the only first-party script on the site.
  See [the Ask page](#the-ask-page) below.

One constraint runs through all of it. **Every control htmx can swap away needs a
stable `id`.** After a swap htmx looks the previously focused element up again by
`document.getElementById`; a control that arrives back without the same id leaves
keyboard focus on `<body>`, stranded outside the widget. `scripts/check_htmx.py`
enforces this, and [AGENTS.md](AGENTS.md#accessibility-invariants) explains why it
is the same failure the arrows were already shaped to avoid.

## The case study carousel

The mechanism is unusual enough to be worth spelling out.

**Position comes from DOM order.** Three `.casestudy--item` elements are styled by
`:nth-child`, and each slot has a fixed role:

| Slot              | Role       | State                                                     |
| ----------------- | ---------- | --------------------------------------------------------- |
| `:nth-child(1)`   | just left  | `opacity: 0`, off to the left, blurred, `pointer-events: none` |
| `:nth-child(2)`   | centre     | visible, unblurred, the only slot whose text is readable  |
| `:nth-child(3)`   | on deck    | visible but small and blurred, offset to the right        |

Advancing the carousel does not change any styles — it changes which item sits in
which slot. Pressing an arrow fetches the fragment for the rotation it moves to,
and that fragment contains the same three case studies written out in the new
order. The `:nth-child` rules then re-apply themselves to whatever now sits in
each slot. There is one fragment per rotation and direction of travel —
`fragments/casestudy/r{0..3}-{next,prev}.html`, eight for the four case studies
there are now. `index.html` holds rotation 0.

**The rotations are copies, and two scripts keep them honest.** Nine files contain
the same four case studies. That is the identical bargain the palette makes across
four files, and it is accepted for the same reason — the alternative is a build
step. `index.html` is the source of truth;
`scripts/propagate_casestudy.py` regenerates every fragment from it and
`scripts/check_htmx.py` fails CI if any rotation drifts, so editing one copy is a
caught error rather than a silent one.

The generator is what makes the duplication cheap enough to keep. Before it, the
checker could tell you the copies disagreed but you still reconciled them by hand,
which is the part that goes wrong. Now `--check` runs in CI as well, so a drifted
fragment fails with an instruction to re-run the generator rather than an
invitation to patch whichever file the error happened to name. The count of case
studies is derived from `index.html` throughout — no script holds it as a
constant — but the stylesheet cannot be derived, so adding a case study still
means writing its slot tokens, `:nth-child` rule and keyframes by hand.
`check_htmx.py` verifies they exist rather than trusting that someone remembered.

**The animation runs backwards.** Because the new layout is already correct the
instant the DOM changes, the slide has to animate *out of* the slot each element
just left, *into* the one it now occupies. That is why every `@keyframes fromItemN`
contains **only a `from` block and no `to`**. The end state is whatever the
`:nth-child` rule assigns — writing a `to` block would override it and break the
effect. The `--casestudy-itemN-*` custom properties hold those starting positions,
which is why they read as animation state rather than theme values.

Slot 4 is not a visible position. Slots 1 to 3 are off-screen-left, active and
blurred peek; the fourth is an off-screen reserve at `opacity: 0` with
`pointer-events: none`, so a deck longer than three has somewhere to keep the
items not currently in play. Its contents stay in the accessibility tree, which
matches slot 1's long-standing behaviour and means a screen reader still meets
every case study rather than three of them.

**Direction travels in the markup.** Each fragment carries `.next` or `.prev` on
the list, which is what selects the animation for the way the deck just moved.
Previously a script added that class to the container; the CSS selectors moved from
`.casestudy--container.next` to `.casestudy--list.next` accordingly.

**Restarting no longer requires a reflow.** The old script had to remove the class,
force layout with `void carousel.offsetWidth` and add it back, because re-adding
the same class does not replay a CSS animation. Every item in a swapped fragment is
a freshly parsed node, and a new element's animations start on their own, so that
whole dance is gone. This is the one place the conversion genuinely removed
complexity rather than relocating it.

**Nothing is timed against the animation.** `--casestudy-slide-duration` (700ms)
drives the CSS and is no longer read by anything else. The arrows used to be locked
for exactly that long by a timer; now re-entry is handled declaratively by
`hx-sync`, so a second press replaces the in-flight request instead of queueing
behind it.

**Reduced motion needs no special case.** Under `prefers-reduced-motion: reduce`
the durations collapse and the slide becomes an instant cut. There is no longer any
code waiting on an animation to finish, so the failure this used to risk — a
control left disabled forever because the event never fired — is not merely handled
but structurally impossible.

## The Ask page

`ask.html` runs semantic search over the portfolio without a server. The visitor's
question is embedded by a sentence-transformer executing in their own tab and
compared against a precomputed index of `portfolio.html`. Nothing is sent
anywhere, because there is nowhere to send it.

**The stack is [Transformers.js](https://huggingface.co/docs/transformers.js/index)
running all-MiniLM-L6-v2 quantised to 8-bit on ONNX Runtime for WebAssembly.**
Two details of that choice are not obvious:

- The self-contained `transformers.min.js` build is required, not the smaller
  `transformers.web.min.js`. The latter is 0.12 MB lighter but ships bare import
  specifiers (`from "onnxruntime-web/webgpu"`) that only a bundler can resolve,
  and this repo does not have one and is not getting one.
- It runs single-threaded, and that is not tunable. Multi-threaded ORT needs
  `SharedArrayBuffer`, which needs cross-origin isolation, which needs COOP and
  COEP response headers that GitHub Pages cannot set. `crossOriginIsolated` is
  `false` on the deployed page — measured, not assumed — so `numThreads` is
  pinned to 1 rather than left to fall back silently.

**Everything is served from this origin.** `vendor/transformers/` holds the
library and the ORT WebAssembly; `assets/models/all-MiniLM-L6-v2/` holds the
weights and tokenizer. `env.allowRemoteModels` is `false` and
`env.backends.onnx.wasm.wasmPaths` is overridden, because the library's default is
to fetch both from third-party CDNs at runtime. Leaving that default would mean
pinning htmx by SRI digest in one file while pulling 35 MB of unpinned executable
WebAssembly from someone else's CDN in another — the same integrity problem, an
order of magnitude larger. The cost is repository size; the benefit is a page that
makes zero third-party requests, which is verifiable in a network panel rather
than merely asserted here.

**The model loads on a click, never on page load.** The weights are 22 MB and do
not compress — measured: gzip returns them at 21.91 MB, byte for byte — while the
runtime compresses about five-fold. Since the incompressible part dominates,
gating it behind an explicit button is the entire optimisation and no amount of
shaving the runtime would matter.

**Retrieval is an exhaustive scan, deliberately.** A few dozen passages at 384
dimensions is under ten thousand multiply-accumulates per query: microseconds, and
four orders of magnitude below the ~100,000-vector scale where an approximate
index like HNSW starts to earn its build time, memory and recall loss. The
reported query time is almost entirely the model embedding the question.

**The corpus is precomputed and committed, not parsed at runtime.** `js/ask.js`
fetches `corpus.json`: one entry per passage, each carrying its heading, its
anchor, its text and a 384-float vector. The model still loads, but only to embed
the question, which is the one vector that cannot be precomputed because it does
not exist until someone types it.

An earlier version did the opposite. It fetched `portfolio.html`, chunked it on
`.project` sections and embedded the result on every visit, on the argument that a
committed index would be a second copy of the prose free to drift from the first.
That argument was real and it was answered in the wrong currency. It charged every
visitor several seconds of WebAssembly inference to recompute a value identical for
all of them, and it quietly made five class names an interface: rename `.project`
or `.project--impact` and a whole section contributed nothing to retrieval while
the status line still reported a healthy-looking count. So the page now takes the
bargain this repository already takes for the palette and the case study deck. One
source of truth, a generator, and a checker that fails CI when a copy drifts.

**The generator is a page, and that is forced rather than chosen.**
`scripts/build-corpus.html` runs in a browser because that is the only place the
vectors can be produced by the same engine that will later compare them. Node was
tried and cannot do it: the vendored `transformers.min.js` is the web build, so
`env.backends.onnx` is an empty object outside a browser and the model loader
resolves paths through `fetch()`, which throws on a filesystem path. A Python
generator would need `onnxruntime`, a dependency the no-build-step stance rules
out. Running it in the browser gives vector parity by construction: same library,
same quantised weights, same runtime. It carries `<base href="../" />` for the same
reason, so its model configuration can be a verbatim copy of the shipped one rather
than the same thing rewritten with `../` in front of every path.

**CI checks the meaning, not the model.** `scripts/check_corpus.py` cannot recompute
an embedding, so it asserts that the text each embedding describes is still on
`portfolio.html`, that each anchor still resolves to a real `id`, that every vector
is 384-dimensional and unit-length, and that the corpus names the model `js/ask.js`
expects. What it cannot catch is prose *added* to the portfolio and never indexed,
because an unindexed paragraph is indistinguishable from one the generator was
never meant to see. That half stays human: load `ask.html` and confirm the passage
count went up.

**Neither button uses the `disabled` property.** `#ask--load` and `#ask--submit`
set `aria-disabled` and guard re-entry with a flag, for the same reason the
carousel arrows use `hx-sync`: disabling the element under a keyboard user's focus
sends it to `<body>` and re-enabling does not bring it back. On success the input
is focused *before* the gate is hidden, so the pressed control never vanishes from
under a live focus.

## There is no logo marquee, on purpose

The home page used to end in scrolling rows of Boxicons logos. It was removed
because a wall of logos is not a skills list: being `aria-hidden` as a whole it
carried no text, so it was never eligible for `corpus.json`, and nothing kept it
in step with the prose. It drifted until it was the only place on the site that
named no AI at all.

**`portfolio.html#skills` is the one authoritative statement of what this site's
author works with**, because it is prose, it is crawlable, and it is what the Ask
page retrieves. A second, decorative copy has no way to stay true.

## Responsive breakpoints

Three max-width breakpoints, in `css/style.css`, using range syntax:

| Breakpoint       | What changes                          |
| ---------------- | ------------------------------------- |
| `width < 1200px` | container hits the `--max-width` edge |
| `width < 850px`  | layout reflows to narrow tablet       |
| `width < 750px`  | nav collapses into the sidebar        |

They are **ordered widest-first**. These are max-width queries, so a later,
narrower rule must win by source order — reordering them silently breaks the
mobile layout.

Full-height elements use `100dvh`, not `100vh`, so mobile browser chrome does not
crop the viewport.

## Deployment

Pushing to `main` publishes automatically. GitHub Pages builds from the `main`
branch, root directory. There is no build to verify and no test suite, so
deployment is a file copy — but there are invariants worth checking before the
copy happens. See below.

### Why `.nojekyll` matters

Pages runs a legacy Jekyll build for this repository, and Jekyll silently excludes
any file or directory whose name starts with `_` or `.`. The `.nojekyll` file
disables that processing. Do not delete it — without it, anything published under
a directory like `_framework/` or `_next/` (which is what most game engines and
bundlers emit) would 404 with no build error to explain why.

### Hosting constraints

Pages serves static files only and cannot set custom response headers. That rules
out `Cross-Origin-Opener-Policy` / `Cross-Origin-Embedder-Policy`, and therefore
`SharedArrayBuffer` — so any WebAssembly content added here has to be
single-threaded.

## Continuous integration

`.github/workflows/ci.yml` runs on every pull request and on pushes to `main`. It
is not a build, and it is deliberately narrow: it checks only the things this repo
duplicates by necessity or asserts in prose, where a mistake is **silent** rather
than loud.

| Check | Why it cannot be left to review |
| ----- | ------------------------------- |
| `scripts/check_htmx.py` | A renamed fragment 404s in silence; a drifted rotation looks fine; a control without an `id` strands focus on `<body>`; a carousel slot with no CSS tokens breaks the layout with no error |
| `scripts/propagate_casestudy.py --check` | Proves the fragments are still what the generator emits, so a drift is fixed by regenerating rather than by hand-patching one copy |
| `scripts/check_corpus.py` | A stale embedding does not raise, it just retrieves worse; a dead anchor sends a result nowhere; a corpus built for another model is the right shape in the wrong space |
| `scripts/check_palette.py` | The palette exists in four copies (below) |
| `scripts/check_repo.py` | Deleting `.nojekyll` breaks paths with no build error; `sitemap.xml` drifts silently; the nav exists in four copies and editing three of them looks fine on the page you are reading |
| `npx @google/design.md lint DESIGN.md` | Holds the file at 0 errors and 0 warnings |

**The palette check is the one that earns its keep.** The Catppuccin values are
written out in four places — the `:root` and dark blocks in `css/style.css`,
`404.html`'s inline subset, `DESIGN.md`'s front matter under semantic names, and
the `theme-color` meta pair in each page. Every one of those duplications is
justified, and none of them is enforced by anything at runtime. Changing one and
not the others produces no error, no visual break in the scheme you happen to be
testing, and no reviewable signal. `scripts/check_palette.py` reads
`css/style.css` as the source of truth and holds the other three against it.

Two things this deliberately does **not** do. It does not check the parts that
matter most — keyboard operability, focus order, both colour schemes, Lighthouse
— because those need a real browser and human judgement;
[AGENTS.md](AGENTS.md#verifying-a-change) remains the authority and CI covers only
its mechanical subset. And it does not introduce a toolchain: the checkers are
stdlib Python, the linter is a pinned one-off `npx` invocation, and there is still
**no `package.json`, no lockfile and no dependency to patch**. The no-build-step
rule above is intact, and a future reader should not "tidy" this into a Node
project.
