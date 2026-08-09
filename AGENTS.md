# AGENTS.md

Working notes for anyone — human or agent — changing this repository. For how the
site is built and why, read [ARCHITECTURE.md](ARCHITECTURE.md) first.

## The shape of the repo

A static personal site: plain HTML and CSS, no build step, no package manager,
served by GitHub Pages from `main`. There is nothing to install and nothing to
compile. Edit the files directly.

**There are exactly six JavaScript files of our own: `js/ask.js`, `js/game.js`,
`js/mocap.js`, `js/mocap-retarget.js`, `js/iris.js` and `js/chief.js`.** Everything else — the work index on the home page — is
[htmx](https://htmx.org) asking for static HTML fragments under `fragments/` and
swapping them in. htmx arrives from a CDN pinned by version and SRI digest.

**There is no webfont and no icon font.** Poppins and Boxicons were both removed:
type is the system UI stack, and the handful of glyphs the site draws are inline
SVG. Adding a font request back is the same kind of decision as adding a `js/`
file, and for the same reason — it is a render-blocking request to a host this
site does not control. That remains the default: adding a second `js/` file is a
real decision, not a shortcut. Read
[ARCHITECTURE.md](ARCHITECTURE.md#interaction-patterns) first and say in the pull
request why markup could not carry it.

`js/ask.js` earns its exception because the search is the one interaction that
is not a request for HTML. Every htmx swap on this site fetches markup a server
already holds; the search multiplies a question against 384-dimensional vectors
that exist only in the visitor's tab. No endpoint could answer it without being
sent the question, which is precisely the property the search exists to avoid. It is
a plain ES module with no bundler and no package manager, consistent with
everything else here.

`js/game.js` earns the second exception on narrower grounds, and the grounds are
worth being precise about because it is the first thing here that breaks a rule
outright. It is the only consumer of `assets/game.wasm`, and its whole job is
the boundary: instantiate the module, call exported functions, read strings back
out of linear memory, draw SVG. The simulation itself is Rust in `game/`. Nothing
on any other page loads it, and `game.html` loads nothing else.

The rule it breaks is not "no JavaScript" and not "no WebAssembly" — the search
has shipped WebAssembly since the day it landed. It is **no build step**: `game/`
needs cargo and a `wasm32-unknown-unknown` target before it produces a byte the
site can serve. That is a real toolchain and this repo said it would not have
one. The containment is that the toolchain is never on the critical path for
anyone: the binary is committed, GitHub Pages serves it as a file like any other,
and a visitor, a contributor editing prose and CI all run without Rust installed.
`scripts/build_game.sh --check` verifies the committed binary against a committed
hash and needs nothing but `sha256sum`.

Be clear about what that hash is worth. It proves the binary has not changed
since it was committed. It does not prove the binary is what `game/src` compiles
to, because nothing here reproduces the build. `scripts/check_corpus.py` is
strictly stronger — it re-derives its claim from the source text. Anyone who
wants the guarantee has to run `scripts/build_game.sh` themselves and read the
diff, which is the honest instruction and is in the script's own header. So that
this is worth attempting, the hash file carries the rustc version that produced
the committed binary on a second line, written by the script rather than kept by
hand. A different rustc gives a different hash, and that is not a fault: it is
why the version is recorded.

`js/mocap.js` earns the third exception on the same grounds as `js/game.js`: it
is a boundary and a renderer, nothing more. It asks the browser for a camera,
asks LiteRT.js (running MediaPipe's BlazePose) to turn each video frame into 33
body landmarks, and asks Three.js to draw a rigged character. Nothing captured
here is sent anywhere — there is no endpoint this page talks to at all, the same
structural privacy property `js/ask.js` has for the same reason: the model, the
runtime and the character are fetched same-origin, once, and every frame after
that is inference against WebAssembly already sitting in the tab. GitHub Pages
cannot set the COOP/COEP headers cross-origin isolation needs, so only the two
non-threaded LiteRT WASM builds are reachable — the same constraint `js/ask.js`
already lives under with `onnxruntime`.

`js/mocap-retarget.js` earns the fourth exception on different grounds, and they
are worth stating precisely because this file is not a boundary the way the other
three are. Turning a landmark pair into a bone rotation is a rule with a real
failure mode if it is wrong, so it is not folded into `js/mocap.js`'s "instantiate,
call, draw" framing — it is argued on its own terms in
[specs/F03_MOCAP.md](specs/F03_MOCAP.md). It touches no DOM and makes no network
call; its only inputs are landmark coordinates, a bone map and a Three.js
namespace, which is what keeps it a rule and not a second boundary.

`js/iris.js` earns the fifth exception on the same "boundary and renderer"
grounds as `js/mocap.js`: camera in, MediaPipe Tasks Vision's
`FaceLandmarker` out, a Breakout game drawn to a `<canvas>`. It is a second,
independent ML runtime rather than a reuse of `js/mocap.js`'s LiteRT —
argued from scratch in
[specs/F05_IRIS.md](specs/F05_IRIS.md#why-this-needs-mediapipe-tasks-vision-instead-of-echos-litert)
because Iris needs the iris-position landmarks (indices 468–477) that
`FaceLandmarker` returns unconditionally, which LiteRT.js's body-only
BlazePose pipeline has no equivalent output for at all.

Unlike the other three, this boundary is not structurally leak-proof by
vendoring alone. The vendored bundle carries its own usage-telemetry client
that attempts a real cross-origin `fetch()` to a Google-controlled endpoint
during ordinary inference, with no consumer-facing opt-out — see
[specs/F05_IRIS.md](specs/F05_IRIS.md#the-vendored-bundle-phones-home-and-iris-has-to-mitigate-it)
for how that was traced. `iris.html` closes that gap with a page-level
`Content-Security-Policy` meta tag scoped to `connect-src 'self' blob:`,
which makes the browser refuse the request outright. This is the one place
on the site where "vendored means same-origin means private" needed a second
mechanism to actually be true, and the CSP violation line it produces in the
console is deliberate, not a regression — see that section before "cleaning
up" the console output on `iris.html`.

`js/chief.js` earns the sixth exception on grounds none of the first five
share. Every prior exception's network story is either "there is no endpoint
this page talks to at all" (`js/ask.js`'s vectors, `js/mocap.js`'s camera
frames) or "one same-origin fetch, once, at load" (the models and runtimes
`js/mocap.js` and `js/iris.js` pull in). `js/chief.js` breaks the site's other
standing default — no host we do not control, the same rule that keeps
webfonts off this site — deliberately and repeatedly: it holds open three
independent polling loops, to Hacker News, GitHub and the NVD, for the entire
time `chief.html` is open, because a demonstration of "keeping up with
technology" that fetched once and froze would not demonstrate anything. The
full argument for why that is worth doing, and how it degrades when one of
the three goes quiet or rate-limits, is in
[specs/F06_CHIEF.md](specs/F06_CHIEF.md#the-site-first-cross-origin-connect-src).
`chief.html` closes the resulting gap the same way `iris.html` closes its
own: a page-level `Content-Security-Policy` meta tag, here scoped to
`connect-src 'self'` plus exactly the three feed hosts, so the browser
refuses every other request outright. No new dependency is vendored to build
it — the flight camera is hand-written against `THREE.Euler`/`THREE.Vector3`
already in the vendored core, and the neon glow sprites are canvas-drawn
textures, because `vendor/three/examples/jsm` carries no postprocessing or
alternate-controls module and adding one to get bloom or pointer-lock flight
would be a second, unrelated exception layered onto this one.

Do not introduce a bundler, framework or package manager to solve a problem that a
few lines of CSS would solve. The absence of a toolchain is a design decision, not
an oversight — see [ARCHITECTURE.md](ARCHITECTURE.md#no-build-step). `game/` is
the one exception on the whole site and it is deliberately quarantined: it is a
separate page, a separate stylesheet, a separate script and a separate language.
A second exception should be argued from scratch, not from this one.

## Local development

Opening `index.html` in a browser works for most changes. To match production —
absolute paths resolve, MIME types are correct — serve it:

```sh
python3 -m http.server 8000
# then open http://localhost:8000
```

**A file:// page cannot fetch fragments.** Opening `index.html` directly now leaves
the work index unable to expand, because htmx issues real requests and the browser
blocks cross-origin `file://` reads. The entries still navigate, since each is a
real link, so this failure looks like a design choice rather than a broken page. Serve the site for anything touching
interaction.

**Browsers cache aggressively on localhost.** If a change to `css/style.css` or to
a file under `fragments/` appears not to have taken effect, confirm the server is
sending the new bytes (`curl -s localhost:8000/fragments/work/ghost-kitchen-startup.html | head`)
before concluding the markup is wrong. Starting a server on a different port gives
you a fresh cache key.

## Verifying a change

There is no test suite. Verify by hand, in roughly this order.

Steps marked **[CI]** also run automatically on every pull request — see
[ARCHITECTURE.md](ARCHITECTURE.md#continuous-integration). The rest are yours:
they need a real browser and someone looking at the result, which is why CI does
not pretend to cover them. **A green CI does not mean a change is verified.**

1. **[CI]** `python3 scripts/check_htmx.py` if any markup, fragment or `hx-`
   attribute changed. It catches the three things a green page load hides: a
   fragment that 404s, a work fragment whose prose drifted from `portfolio.html`,
   and a control that lost the `id` htmx re-focuses by. It also checks that no
   trigger sits inside the region it swaps, which is the failure that is invisible
   to a mouse and strands a keyboard user on `<body>`.
2. **[CI]** `python3 scripts/propagate_work.py --check` if you touched the work
   prose. It is the other half of step 1: that check proves the fragments agree
   with `index.html`, this one proves they are still exactly what the generator
   emits. When it fails, re-run the generator — do not hand-patch the file it
   named.
3. **[CI]** `python3 scripts/check_corpus.py` if you touched `portfolio.html`. The
   Ask page searches `corpus.json`, a generated index of that page; this asserts
   every passage in it is still on the page. Regenerate with
   `scripts/build-corpus.html` — see the bullet under "Other things not to break".
4. If you **added** prose to `portfolio.html`, regenerate the corpus, then load
   the home page, press the load button and confirm the **passage count went up**.
   This is the one half of the bargain CI cannot hold up: text that was never
   indexed looks exactly like text the generator was never meant to see, so
   nothing static can tell the difference. Step 3 catches prose that changed;
   only this catches prose that is missing.
5. Load the page **from a server** and confirm the console is clean *and* that no
   request in the network panel returned 404. A missing fragment is a silent
   no-op on click, not an error.
6. Tab through the whole page. Every interactive control must be reachable and
   show a visible focus ring.
7. Operate the work index **from the keyboard**, not just by clicking. Tab to an
   entry title, activate it, and check that focus is still on that title afterwards
   and that the detail appeared below it. Do every entry, not one: a fragment that
   404s is a silent no-op, so an entry that does nothing looks identical to one you
   have not pressed yet.
8. Check the layout at each breakpoint (1200 / 850 / 750px) and with
   `prefers-reduced-motion: reduce` enabled.
9. **Check both colour schemes.** The site themes itself off
   `prefers-color-scheme`, so every visual change has two results. A
   default-scheme-only check leaves half the work unverified.
10. For anything touching markup, metadata or colour, run a Lighthouse navigation
   audit — **once per scheme**. Accessibility, Best Practices and SEO are all at
   100 in both; keep them there.
11. **[CI]** If `DESIGN.md` changed, `npx @google/design.md lint DESIGN.md`. It is
    at **0 errors and 0 warnings**; keep it there. Note the linter exits `0` on
    warnings, so read the summary rather than the exit code — CI gates on the JSON
    for that reason. This is a one-off command, not a dependency — the repo still
    has no `package.json`.
12. **[CI]** If any colour changed, `python3 scripts/check_palette.py`. The palette
    is written out in **five** places and nothing but this script keeps them in
    step; see the bullet under "Other things not to break".

## Accessibility invariants

Six things in this codebase look like mistakes and are load-bearing. Each has an
inline comment; do not "clean them up".

- **Every control htmx drives keeps a stable `id`, and swapped-in fragments reuse
  the same ones.** This is the invariant the shape of the work index exists to
  protect. After a swap htmx looks the previously focused element up again by
  `document.getElementById`; if the incoming markup has no element with that `id`,
  focus falls to `<body>` and a keyboard user is stranded outside the component
  with no way back.

  The work index avoids this structurally rather than carefully: every trigger is
  a sibling of the panel it fills, so a swap cannot remove it. That is the shape to
  copy. `scripts/check_htmx.py` fails CI on an `hx-get` with no `id` *and* on a
  trigger whose `id` appears inside its own target, so the arrangement is checked
  rather than remembered. The rule still binds anything htmx swaps in future,
  including a control that does replace itself: that one needs a stable `id` in
  every incoming fragment, and step 7 of "Verifying a change" is how you catch it.
- **The mobile menu checkbox is visually hidden, not `display: none`.**
  `display: none` removes an element from the tab order *and* the accessibility
  tree. The sidebar is a CSS-only pattern that depends on its checkbox staying
  focusable, so hiding it that way makes the menu unopenable by keyboard and
  invisible to screen readers. Use the `.visually-hidden` class. (The sidebar
  stays CSS because it has nothing to fetch.)
- **Nothing on this site uses the `disabled` property.** Disabling the element the
  user just pressed moves focus to `<body>`, which is the same failure as losing an
  `id`. In the work index the `.work--title a.htmx-request` opacity rule is the
  whole of the in-flight affordance, and a second press simply supersedes the
  first. `aria-disabled` is acceptable where a state genuinely needs announcing;
  `disabled` is not, ever.

  The two buttons in the search —
  `#ask--load` and `#ask--submit` — set `aria-disabled` and guard re-entry with a
  plain flag in `js/ask.js`, for exactly the same reason. An earlier version used
  `disabled` on the load button, reasoning that it is removed from the flow once
  the model has loaded. That is true, but it is removed when the load *finishes*:
  disabling it on press stranded a keyboard user on `<body>` for the whole
  multi-second download. If you find yourself justifying a `disabled` here, check
  when the element actually leaves the page.
- **Decorative inline SVG carries `aria-hidden="true"` and `focusable="false"`.**
  Without the first a screen reader announces meaningless graphics; without the
  second the SVG is a tab stop in some browsers. Every glyph on the site is
  decoration next to a real text label, so this applies to all of them.
- **Lists styled with `list-style: none` keep `role="list"`.** Removing the marker
  removes the list semantics in Safari with VoiceOver, so `.work--index`,
  `.masthead--socials` and `.colophon--checks` all carry the role back explicitly.
- **The work panels carry `aria-live="polite"`.** Detail arrives in a region the
  visitor did not navigate to; without this it appears in silence. Focus stays on
  the trigger deliberately, so the announcement is the only signal a screen reader
  user gets.

Three further rules for any change:

- **Animations stay behind a `prefers-reduced-motion` guard.** Nothing on the page
  is revealed by an animation any more, so the guard cannot strand content and does
  not need a matching `opacity: 1` override the way the old carousel entrances did.
  It is now purely about honouring the preference, which is reason enough: keep it.
- **Text contrast stays at or above 4.5:1, in both flavours.** This is why
  `--accent-text` exists alongside `--accent`; do not substitute one for the other
  to "keep the colours consistent". `--accent` is `sky`, which measures 2.47:1 on
  Catppuccin Latte's page background — it is legal on `aria-hidden` decoration and
  nowhere else, at any font size. Check any colour change against **Latte**, which
  is always the tighter of the two flavours. See [DESIGN.md](DESIGN.md).
- **`--accent-text` is Latte `#026389`, which is deliberately not a Catppuccin
  colour.** No Latte cyan reaches 4.5:1 as text. Do not "fix" it back to `sky`;
  that makes every focus ring on the site fall below the 3:1 floor.

## Other things not to break

- **`.nojekyll` must stay.** Deleting it makes any path starting with `_` or `.`
  disappear from the published site with no build error. See
  [ARCHITECTURE.md](ARCHITECTURE.md#why-nojekyll-matters).
- **`portfolio.html` is out of the nav on purpose, and in `sitemap.xml` on
  purpose.** Those are not in conflict. It is a destination a visitor arrives at
  rather than one they pick: from a search result, from a work entry's title link,
  from the link under the work index, or from a search engine. Being absent from a
  menu costs it nothing that matters, because it is still reachable by four routes
  and still crawlable. It is also the reason
  nothing in its own nav carries `aria-current`.

  So do not "tidy" this in either direction. Adding it back to the nav undoes a
  deliberate decision; deleting the page because nothing in the nav points at it
  breaks every anchor in `corpus.json`, the `<noscript>` fallback, and the only
  crawlable copy of the detail prose.
- **The nav is written out three times and nothing but `scripts/check_repo.py`
  keeps them in step.** In `index.html` and `portfolio.html`, because those links
  are content a crawler should see and the site has to navigate with no JavaScript
  at all, and in `fragments/404-links.html`. Adding or removing an entry means
  editing all three. The check compares the visible labels rather than the `href`s,
  because the hrefs legitimately differ: `index.html` links its own sections as
  bare `#ask`, `portfolio.html` has to prefix them with `index.html`, and the 404
  fragment must use root-absolute `/#ask`.

  `ask.html` deliberately has no `nav-links--container`, so it is not a fourth
  copy. A page whose only job is to forward a reader should not offer a menu, and
  `check_repo.py` only compares pages that have one.
- **Every work entry exists twice: in `portfolio.html` and in its fragment.** Four
  entries currently, so four files under `fragments/work/`. This is the same
  bargain the palette makes across several files, with better enforcement:
  `portfolio.html` is the source of truth, `scripts/propagate_work.py` regenerates
  every fragment from it, and `scripts/check_htmx.py` plus
  `propagate_work.py --check` both fail CI if a copy drifts. **Never edit a
  fragment.** Edit `portfolio.html`, run the generator, commit what it wrote.

  How many entries there are is deliberately not written down as a number
  anywhere, and unlike the carousel this replaced, adding one needs nothing written
  by hand in the stylesheet: the index is a list, so a fifth entry styles itself.
- **`corpus.json` is generated; never hand-edit it.** It is the search
  index: one entry per passage of `portfolio.html`, each carrying the
  384-dimensional embedding of that passage. `portfolio.html` is the source of
  truth and `scripts/build-corpus.html` is the only thing that writes the index.
  Regenerate it by serving the repository, opening
  `/scripts/build-corpus.html`, pressing the button and saving the result over
  `corpus.json`.

  **The generator embeds one passage at a time, and that is not an oversight to
  optimise.** Batching pads every sequence in a batch to its longest member, and
  under 8-bit quantisation the padding changes the output: a two-passage prose edit
  once moved the vectors of eleven untouched passages to a cosine of 0.997 against
  themselves. Nothing catches that, because `check_corpus.py` compares text. Left
  unbatched, re-running the generator with no prose change reproduces the committed
  file byte for byte, which is a property worth more than the few seconds it costs.

  It is a page rather than a Python script for one reason: the vectors have to be
  the ones the shipped code would have produced, and a vector that differs does
  not raise an error, it just retrieves worse. Running the generator in the same
  browser, against the same vendored runtime and weights, makes that true by
  construction. Node cannot do it — the vendored build is the web build, its ONNX
  backend registry is empty outside a browser — and a Python generator would mean
  taking on `onnxruntime`, which the no-toolchain stance rules out. The page
  carries `noindex`, is absent from `sitemap.xml` and sits under `scripts/`, which
  is outside the `*.html` globs `check_repo.py` and `check_htmx.py` walk.

  Editing the text in `corpus.json` without re-running the generator leaves the
  passage describing one thing and its vector describing another, which is exactly
  the silent failure `scripts/check_corpus.py` exists to catch.

- **Fragments carry no colour and no `<head>`.** They are pieces of a page. Putting
  a hex value in one would escape `scripts/check_palette.py`, which reads only the
  five files that hold the palette. `robots.txt` disallows `/fragments/` for the
  same reason they are not pages, and they are correctly absent from `sitemap.xml`.
- **The htmx `<script>` stays pinned by version *and* SRI digest, on every page.** A
  CDN this site does not own must not be able to change what executes here. If you
  bump the version you must recompute the digest and update `HTMX_SRI` in
  `scripts/check_htmx.py`, which is what stops the two from drifting apart.
- **`hx-get` paths in `404.html` must be root-absolute.** GitHub Pages serves that
  file at whatever URL was missed, so a relative path resolves against a directory
  that does not exist. CI checks this.
- **`404.html` styles are inline on purpose.** It must render even if the
  stylesheet is what failed. Its palette is therefore a hand-kept copy of the one
  in `css/style.css` — change one and change the other.
- **The palette is written out in five places.** `css/style.css` is the source of
  truth; `404.html`, `DESIGN.md`'s front matter, the `theme-color` metas and
  `css/game.css` are copies. The last of those is Mocha-only and writes literal
  hexes, so the check holds its shared colours to the dark block and makes it name
  any extra Catppuccin colour it uses. `scripts/check_palette.py` is what holds them together — run it after any
  colour change, and do not work around it by editing only the copy you are looking
  at. Each duplication is justified in its own bullet below.
- **`DESIGN.md`'s front matter is the third copy of the palette.** It follows the
  [DESIGN.md format](https://github.com/google-labs-code/design.md), so the tokens
  are machine-readable and must stay in step with `css/style.css` and `404.html`.
  Two traps: the dark values are duplicated as `mocha-*` tokens because the format
  has no light/dark axis yet, and an illustrative fenced block in that file is
  tagged `text` rather than `yaml` on purpose — the parser reads fenced YAML as a
  token source and a second `colors:` key makes the linter stop early.
- **The `theme-color` metas come in pairs**, one per `prefers-color-scheme`, in
  both `index.html` and `404.html`, and their values are the fourth copy of the
  palette — each equals `--bg-chrome` in its flavour. A single unconditional tag
  would paint the browser chrome the wrong colour for half of all visitors, so
  `scripts/check_palette.py` rejects a tag with no media attribute as well as a
  stale hex.

## Contributing

Every change goes through a pull request; nothing is committed to `main` directly,
because pushing to `main` publishes to the live site immediately.

Branch names follow `type/tatangharyadi/short-description`.

Wait for CI to pass before merging. It is fast and it only checks mechanical
invariants, so a failure is a real finding rather than flake — read the message
instead of re-running it.

Keep `sitemap.xml` in step when adding or removing a page. CI checks this, and
exempts pages carrying `<meta name="robots" content="noindex">` — that is why
`404.html` is correctly absent from the sitemap.
