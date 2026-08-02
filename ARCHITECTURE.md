# Architecture

How this site is put together, and why. For the rules that keep it working, see
[AGENTS.md](AGENTS.md).

## No build step

Plain HTML and CSS plus one runtime library, served directly by GitHub Pages from
`main`. There is no bundler, no package manager, and no lockfile.

**One exception, and it is quarantined.** `game/` is Rust and needs cargo to
produce `assets/game.wasm`. The binary is committed, so the toolchain is on
nobody's critical path: visitors, contributors editing prose, and eight of the
nine CI checks all run without Rust installed. It is a separate page, a separate
stylesheet, a separate script and a separate language, and it is argued for in
[The simulation](#the-simulation) and in
[AGENTS.md](AGENTS.md#the-shape-of-the-repo). Everything below still describes
the rest of the site, which has no build step at all.

This is deliberate rather than incidental. The site is a personal homepage whose
content changes a few times a year; a toolchain would need reviving every time,
and a dependency tree would need patching in between. The tradeoff accepted in
exchange is that shared values live in CSS custom properties instead of build-time
constants, and one external resource loads from a CDN at runtime:

- [htmx](https://htmx.org/) 2.0.10 via unpkg

It used to be three. [Poppins](https://fonts.google.com/specimen/Poppins) from
Google Fonts and [Boxicons](https://boxicons.com/) from unpkg both went in the
redesign. A webfont and an icon font are render-blocking requests to hosts this
site does not control, bought nothing the system UI stack and a handful of inline
SVG paths do not already do, and made the page open by asking two strangers for
permission to draw itself. Removing them is also what lets the colophon state that
the site issues no third-party request at all until a visitor asks for the model.

htmx is a `<script>` tag, not a dependency in the package-manager sense: pinned to
an exact version and verified by an SRI digest, which is how Boxicons was loaded
before it was dropped.
Nothing installs it and nothing builds against it. The rule this repo actually
holds is *no toolchain*, and adding a library that is one URL does not breach it —
but note the site has a script it did not have before, and if unpkg is unreachable
the work index stops expanding in place. It does not stop working: every trigger in
that index is an `<a href>` pointing at the same prose on `portfolio.html`, so
without htmx a click is a navigation rather than a swap. The enhancement is the
in-place detail, not the content, which is why that degradation is a change of
route and not a dead control.

## Layout

```
index.html              the whole site above the detail: masthead, the search,
                        the work index, the colophon. Nav is three anchors into it
portfolio.html          the work in full, plus now/skills/earlier; the source the
                        search indexes. Not in the nav: arrived at, not picked
ask.html                a noindex stub. The search moved to index.html#ask and a
                        static host cannot 301, so the old URL says so in markup
corpus.json             generated search index: one embedding per portfolio passage
404.html                self-contained not-found page
game.html               the trading simulation; reached from the job title on the
                        home page, not from the nav. Crawlable and in the sitemap
css/style.css           all styles for the site
css/game.css            all styles for game.html, and only game.html
js/ask.js               first-party script; see The search
js/game.js              first-party script; the boundary to assets/game.wasm
game/                   Rust source for the simulation, and the TSVs it is built
  data/                 from. Not served. See The simulation
  src/
scripts/gen_game_data.py generates game/src/world.rs from game/data/*.tsv
scripts/build_game.sh   compiles game/ to assets/game.wasm, or --check verifies it
fragments/              the HTML states htmx fetches; pieces of a page, not pages
  work/                 one per entry in the work index, generated from portfolio.html
  404-links.html        section shortcuts, loaded by 404.html
vendor/transformers/    Transformers.js and the ORT WebAssembly, served from origin
assets/game.wasm        the compiled simulation, committed, with its sha256 beside it
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

**The nav offers three destinations and none of them is a page.** Ask, Work and
Colophon are anchors into `index.html`; the site's second real page,
`portfolio.html`, is not a menu choice. It holds the prose everything else is built
out of, and the two things that read it are the search and a crawler, neither of
which uses a nav. A visitor reaches it from a search result, from a work entry's
title link, from the link under the work index, or from a search engine.

Leaving it in the nav made the site ask the visitor to choose between reading the
portfolio and searching it, which is a choice with an obviously better answer and no
reason to be offered. Taking it out costs nothing measurable: the page is still in
`sitemap.xml`, still linked from several places on the home page, and still the
only crawlable copy of the detail. What it changes is what the site puts forward, which is the search. That
is also why nothing in that page's own nav carries `aria-current`.

`404.html` intentionally duplicates its styles inline. It has to render correctly
even if `css/style.css` is the thing that failed to load, so its only external
reference is the favicon. Its `font-family` is the same system UI stack the
stylesheet uses, so it fetches nothing at all. The cost of that independence is that its
palette is a copy of the one in `css/style.css` and has to be kept in step by hand.

## Design tokens

Everything themeable lives in the `:root` block at the top of `css/style.css`,
with a `prefers-color-scheme: dark` block immediately after it holding the dark
overrides. Colour is [Catppuccin](https://catppuccin.com) — Latte in light mode,
Mocha in dark. **[DESIGN.md](DESIGN.md) is the reference for which token to reach
for and what contrast each one is verified at**; the rules below are the
constraints that are not about colour at all:

- `--measure` is a line-length limit in characters, not a container width, and the
  two are not interchangeable. Body text is capped at `--measure`; the sections
  that hold it are capped at `--max-width`. Setting prose to the wider of the two
  is the single easiest way to make this page hard to read.
- `--font-mono` is load-bearing rather than decorative. Section labels, the
  colophon readout and the category lines are all set in it because they are data
  about the content rather than content, and the change of face is what says so.
- `color-scheme: light dark` on `:root` is what makes native scrollbars and form
  controls follow the active flavour. Removing it leaves them light under Mocha.

One colour constraint is worth repeating here because it has already been
rediscovered once: the accent exists in two variants, and they are not
interchangeable. `--accent` is decoration only; `--accent-text` is for text and
every focus ring. See [DESIGN.md](DESIGN.md#the-accent-rule).

## Interaction patterns

Interaction is hypermedia. Every state the page can reach is a file under
`fragments/`, and a control asks for the state it moves to. There is no
client-side state to get out of step with the markup, because the markup *is* the
state.

The related [tabs (hateoas)](https://htmx.org/examples/tabs-hateoas/) documentation
says that pattern "requires dynamic server-side routing", which is true of a
templated server and not true here: each response is fixed and depends on nothing
about the request, so a static file serves it exactly. That is the whole reason
this works on GitHub Pages, which cannot compute a response or set a header.

- **The work index** expands an entry in place. See the section below.
- **Mobile sidebar** is still the pure-CSS checkbox hack: a single `<input
  type="checkbox">` toggles the menu, with a full-screen `<label>` as the
  click-away overlay. It stays CSS because it has nothing to fetch — there is no
  state to get from a server, and htmx would only add a network round trip to
  something that already works offline.
- **The search is the one exception**, and it is worth being precise about why.
  Every pattern above works because the state being requested is markup that
  already exists somewhere. Semantic search has no such file: the answer is a
  matrix multiply against 384-dimensional vectors computed in the visitor's tab
  from a model running there. Serving it as a fragment would mean sending the
  question to a server, which is the exact property the page is built to avoid. So
  `js/ask.js` is a plain ES module — no bundler, no package manager, consistent
  with the rest of the repo — and it is the only first-party script on the site.
  See [the search](#the-search) below.

One constraint runs through all of it. **Every control htmx can swap away needs a
stable `id`.** After a swap htmx looks the previously focused element up again by
`document.getElementById`; a control that arrives back without the same id leaves
keyboard focus on `<body>`, stranded outside the widget. `scripts/check_htmx.py`
enforces this. The work index satisfies it structurally rather than by care, since
no trigger is inside its own target, but the rule binds anything swapped in future
and [AGENTS.md](AGENTS.md#accessibility-invariants) explains the failure in full.

## The work index

Four entries, all on screen at once, each one a title and a line of scope. Pressing
a title fetches that entry's detail and drops it into the panel below it.

**This replaced a carousel, and the replacement is mostly a subtraction.** The deck
showed one case study at a time behind a slide animation and kept the other three
in off-screen slots addressed by `:nth-child`. That cost eight fragments (one per
rotation per direction), four sets of slot tokens, four `@keyframes` blocks with a
deliberate missing `to`, an arrow modulus that could only be wrong on the wrap, and
a CSS-token check in CI to catch the one drift none of the rest would see. What it
bought was a way to compare four short items by looking at one of them. The index
is a list, so the comparison is free and all of that machinery is gone.

**Triggers sit outside the region they swap.** Each entry is

```html
<a id="work--open-{slug}" href="portfolio.html#{slug}"
   hx-get="fragments/work/{slug}.html"
   hx-target="#work--panel-{slug}" hx-swap="innerHTML">
```

and `#work--panel-{slug}` is a sibling that starts empty. A swap therefore never
removes the control that caused it, which means the focus invariant the carousel
had to be shaped around cannot be violated here rather than merely being avoided.
`scripts/check_htmx.py` asserts the structure directly: it fails if a trigger's
`id` appears anywhere inside its own target.

**The `href` is the fallback and it is a real one.** With htmx unavailable the same
click navigates to the same prose on `portfolio.html`, so the index degrades to a
table of contents rather than to a set of dead links. This is also why the trigger
is an `<a>` rather than a `<button>`: there is a URL behind it.

**The fragments are generated, and two checks keep them honest.**
`portfolio.html` is the source of truth, `scripts/propagate_work.py` writes every
file under `fragments/work/` from it, and `--check` fails CI when a committed
fragment is not what the generator would emit. `scripts/check_htmx.py` separately
proves each fragment resolves and still agrees with the page. Editing a fragment by
hand is a caught error, not a silent one. The number of entries is derived from
`portfolio.html` throughout; no script holds it as a constant, and unlike the
carousel nothing in the stylesheet has to be written by hand to add one.

**The panel carries `aria-live="polite"`.** Content arriving in a region the user
did not navigate to is announced rather than appearing in silence. Nothing is
focused on swap: the trigger keeps focus, which is where a keyboard user expects to
still be, and the new content is the next thing in reading order.

## The search

`index.html#ask` runs semantic search over the portfolio without a server. The
visitor's question is embedded by a sentence-transformer executing in their own tab and
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
pinning htmx by SRI digest in one file while pulling 20 MB of unpinned executable
WebAssembly from someone else's CDN in another — the same integrity problem, an
order of magnitude larger. The cost is repository size; the benefit is a page that
makes zero third-party requests, which is verifiable in a network panel rather
than merely asserted here.

**The model loads on a click, never on page load.** Measured against the deployed
URLs, `Accept-Encoding: gzip`:

| Asset               | On disk   | Transferred | Saved |
| ------------------- | --------- | ----------- | ----- |
| Quantised weights   | 22.97 MB  | 16.22 MB    | 29%   |
| ORT WebAssembly     | 12.94 MB  | 3.35 MB     | 74%   |
| Tokenizer, JS, corpus | 1.41 MB | 0.43 MB    | 70%   |
| **Total**           | 37.32 MB  | **20.00 MB** | 46%   |

Pages serves gzip and not brotli: asking for `br` alone returns the file
unencoded. The weights are the part that resists compression, at 29% against the
runtime's 74%, so they end up **81% of the transfer** despite being under two
thirds of the bytes on disk. Gating the download behind an explicit button is
therefore the entire optimisation, and shaving the runtime would not matter.

Quote the transferred column, never the on-disk one, and re-measure rather than
recomputing by hand:

```sh
curl -s -H 'Accept-Encoding: gzip' -o /dev/null -w '%{size_download}\n' \
  https://tatangharyadi.github.io/assets/models/all-MiniLM-L6-v2/onnx/model_quantized.onnx
```

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
bargain this repository already takes for the palette and the work index. One
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

**It embeds one passage per call, and the slow way round is the correct one.** An
earlier version batched sixteen at a time. A batch is padded to its longest member,
and under 8-bit quantisation that padding moves the result: editing the prose of
two passages shifted the committed vectors of the eleven unedited passages sharing
a batch with them, to a cosine of 0.997 against their own previous values. Nothing
catches that. `check_corpus.py` compares text, so a vector that moved because a
neighbour got longer is indistinguishable from one that did not move at all.
Unbatched, a vector is a function of its own text and nothing else, so a
regeneration after an unrelated edit produces a diff a reviewer can read. It also
matches the live path more closely, since `js/ask.js` embeds a single query string
and never a batch.

**CI checks the meaning, not the model.** `scripts/check_corpus.py` cannot recompute
an embedding, so it asserts that the text each embedding describes is still on
`portfolio.html`, that each anchor still resolves to a real `id`, that every vector
is 384-dimensional and unit-length, and that the corpus names the model `js/ask.js`
expects. What it cannot catch is prose *added* to the portfolio and never indexed,
because an unindexed paragraph is indistinguishable from one the generator was
never meant to see. That half stays human: load the home page, press the load button and confirm the
passage count went up.

**Neither button uses the `disabled` property.** `#ask--load` and `#ask--submit`
set `aria-disabled` and guard re-entry with a flag: disabling the element under a keyboard user's focus
sends it to `<body>` and re-enabling does not bring it back. On success the input
is focused *before* the gate is hidden, so the pressed control never vanishes from
under a live focus.

## The simulation

`game.html` is an age-of-sail trading game on a hexagonal grid. Rust in `game/`,
compiled to `wasm32-unknown-unknown`, drawn by `js/game.js` as inline SVG.

**Why WebAssembly, honestly.** Not for speed. A full turn measured in Chrome is
46 microseconds: 0.23 for the simulation step and the rest for serialising the
425-cell viewport and the status block to text the page can read. That is a third
of a percent of a frame at 60Hz, and any of it would run fine in JavaScript. The
reason is the other one: the simulation is about 2,950 hand-written lines with a
world model, a market, navigation and fog of war, and it has 46 tests. (That
count excludes `game/src/world.rs`, which is generated and would flatter it.) Rust gives that a type
system, exhaustive matching and `cargo test`. Claiming a performance need would
be the easier argument and it would not be true.

**No `wasm-bindgen`, no `wasm-pack`.** The exports are `#[no_mangle] extern "C"`
functions taking and returning `i32`. Strings cross the boundary as UTF-8 in
linear memory: the module writes into a buffer and exposes a pointer and a
length, and `js/game.js` decodes it. The compiled binary therefore has **no
import section at all**, which is why `WebAssembly.instantiateStreaming(fetch(…))`
is called with no import object. The whole glue layer is a few dozen lines that
a reader can hold in their head, in place of a generated one they cannot.

**`memory.grow` detaches every `ArrayBuffer` view in JavaScript.** This is the
one boundary rule that bites silently: a `Uint8Array` cached over
`instance.exports.memory.buffer` becomes zero-length the moment the module grows
its heap, with no error. So `js/game.js` never caches a view. Both readers,
`renderBytes()` and `takeText()`, re-derive the pointer *and* a fresh view on
every single call.

**The world is generated, not hand-written.** `game/data/*.tsv` holds coastline
outlines, port positions, trade data and the goods matrix;
`scripts/gen_game_data.py` rasterises them into `game/src/world.rs`. Editing
`world.rs` by hand is the mistake — change a TSV and regenerate, the same bargain
as the work fragments. The generator asserts three things about the result and
fails the build otherwise: no two ports in one hex, every port with land in an
adjoining hex, and all 70 ports reachable from one another by sea. The middle one
was added after 21 harbours shipped floating in open ocean, because the original
check only asked whether ports could reach each other by sea and a port in the
middle of the Pacific passes that easily.

**Position and trade data come from different files on purpose.**
`game/data/ports.tsv` is a faithful transcription of a 1990 reference table and
stays the source of record for economy and specialty. Its coordinates are that
game's own sextant readings rather than geography — it puts London at 65N — so
`game/data/port_place.tsv` carries real degrees and is the source of record for
where a port sits. Splitting them means the transcription can still be checked
against the source line by line, and it uses *less* of the reference than the
first version did, not more.

**`game.html` is Catppuccin Mocha regardless of system theme,** which is the one
place on the site that ignores `prefers-color-scheme`. A terminal chart in Latte
is not a lighter version of the same artifact, it is a different one. The
deviation is argued in the header of `css/game.css` rather than left to be
discovered.

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
| `width < 1200px` | the colophon readout drops to one label/value pair per row |
| `width < 850px`  | the masthead portrait moves below the identity block       |
| `width < 750px`  | nav collapses into the sidebar; sections lose vertical padding |

They are **ordered widest-first**. These are max-width queries, so a later,
narrower rule must win by source order — reordering them silently breaks the
mobile layout.

No section is a full viewport any more. The one remaining viewport measurement is
the mobile sidebar's height, and it uses `100dvh` rather than `100vh` so mobile
browser chrome does not crop it.

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
| `scripts/check_htmx.py` | A renamed fragment 404s in silence; a fragment that drifted from the prose looks fine; a control without an `id` strands focus on `<body>`; a trigger moved inside its own target strands it too, and only on a keyboard |
| `scripts/propagate_work.py --check` | Proves the fragments are still what the generator emits, so a drift is fixed by regenerating rather than by hand-patching one copy |
| `scripts/check_corpus.py` | A stale embedding does not raise, it just retrieves worse; a dead anchor sends a result nowhere; a corpus built for another model is the right shape in the wrong space |
| `scripts/check_palette.py` | The palette exists in five copies (below) |
| `scripts/check_repo.py` | Deleting `.nojekyll` breaks paths with no build error; `sitemap.xml` drifts silently; the nav exists in three copies and editing two of them looks fine on the page you are reading |
| `scripts/gen_game_data.py --check` | Proves `game/src/world.rs` is still what the TSVs produce, and re-runs the three map assertions: a coastline edit that walls a port in, strands one in open ocean or drops two into one hex fails here instead of being found by sailing into it |
| `scripts/build_game.sh --check` | Verifies `assets/game.wasm` against the hash committed beside it. Needs no Rust |
| `npx @google/design.md lint DESIGN.md` | Holds the file at 0 errors and 0 warnings |
| `cargo test --manifest-path game/Cargo.toml` | The simulation tests. The only check that needs a toolchain, so it runs alone in a second job and a red mark there means one thing |

**The palette check is the one that earns its keep.** The Catppuccin values are
written out in five places — the `:root` and dark blocks in `css/style.css`,
`404.html`'s inline subset, `DESIGN.md`'s front matter under semantic names, the
`theme-color` meta pair in each page, and the literal hexes in `css/game.css`.
Every one of those duplications is justified, and none of them is enforced by
anything at runtime. Changing one and not the others produces no error, no visual
break in the scheme you happen to be testing, and no reviewable signal.
`scripts/check_palette.py` reads `css/style.css` as the source of truth and holds
the other four against it.

`css/game.css` is the awkward one and was almost left out, which would have been
the wrong call: an unchecked copy is exactly the drift the script exists to
prevent, and "it is a special case" is how a five-copy palette becomes a
four-copy check. It is special in a real way, though. That page is Mocha under
both colour schemes, so it writes hexes rather than `var()`, and it needs
Catppuccin colours the site's seven tokens do not carry, because a map has to
tell a pirate from a port. So the check has two halves: any colour the site also
defines must equal the Mocha value there, and anything else has to be named in
`MOCHA_EXTRA` with its upstream Catppuccin name. That makes "I picked a nice
blue" into something a reviewer can see. A Latte value appearing there is called
out separately, because a light-theme colour on a permanently dark page looks
almost right, and an allowlist entry nothing uses any more fails too.

Two things this deliberately does **not** do. It does not check the parts that
matter most — keyboard operability, focus order, both colour schemes, Lighthouse
— because those need a real browser and human judgement;
[AGENTS.md](AGENTS.md#verifying-a-change) remains the authority and CI covers only
its mechanical subset. And it does not introduce a toolchain: the checkers are
stdlib Python, the linter is a pinned one-off `npx` invocation, and there is still
**no `package.json`, no lockfile and no dependency to patch**. The no-build-step
rule above is intact, and a future reader should not "tidy" this into a Node
project.
