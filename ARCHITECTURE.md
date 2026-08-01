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
unreachable the hero tabs and the carousel stop advancing. Everything that matters
for reading the page is in the initial HTML, which is why that degradation costs
interaction and not content.

Fonts are loaded with `<link rel="preconnect">` plus `<link rel="stylesheet">` in
`index.html`, **not** `@import` in the stylesheet. An `@import` would serialize the
request chain — the browser must parse `style.css` before it discovers the font
request — and delay first paint.

## Layout

```
index.html              single page: nav + home, case study, tech stack sections
404.html                self-contained not-found page
css/style.css           all styles, including the carousel slide animations
fragments/              the HTML states htmx fetches; pieces of a page, not pages
  hero/                 one file per hero tab, each carrying the whole tablist
  casestudy/            r{0,1,2}-{next,prev}.html, one per rotation and direction
  404-links.html        section shortcuts, loaded by 404.html
assets/favicon.svg      favicon
assets/images/          profile photo (webp) and the 1200x630 social share card
.nojekyll               opt out of Jekyll processing (see Deployment)
robots.txt, sitemap.xml crawler hints
.github/workflows/      ci.yml, the invariant checks (see Continuous integration)
scripts/                stdlib-Python checkers run by CI; not a build step
ARCHITECTURE.md         this file
DESIGN.md               the Catppuccin palette, semantic tokens and contrast floors
                        (in the google-labs-code/design.md format)
AGENTS.md               working rules and invariants
CLAUDE.md               one line, imports AGENTS.md
LICENSE                 MIT, code only
```

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

Interaction is hypermedia. Every state of the hero tabs and the case study
carousel is a file under `fragments/`, and a control asks for the state it moves
to. There is no client-side state to get out of step with the markup, because the
markup *is* the state: a fragment says which tab is selected, which case studies
are in which slot, and where each control points next.

The htmx examples that shape this are [tabs
(hateoas)](https://htmx.org/examples/tabs-hateoas/) for the hero and a
click-to-load swap for the carousel. Their documentation says the tab pattern
"requires dynamic server-side routing", which is true of a templated server and
not true here: each response is fixed and depends on nothing about the request, so
a static file serves it exactly. That is the whole reason this works on GitHub
Pages, which cannot compute a response or set a header.

- **Hero tabs** ("Dear All" / "My Services" / "For Recruiters") are `<button
  role="tab">` elements. Each response carries the entire tablist plus the panel,
  so `aria-selected` is decided by the fragment that was served. The first state
  is inlined in `index.html` rather than fetched on load, so the hero copy is in
  the initial HTML for crawlers and still renders with no script at all.
- **Case study carousel** swaps the whole deck. See the section below.
- **Mobile sidebar** is still the pure-CSS checkbox hack: a single `<input
  type="checkbox">` toggles the menu, with a full-screen `<label>` as the
  click-away overlay. It stays CSS because it has nothing to fetch — there is no
  state to get from a server, and htmx would only add a network round trip to
  something that already works offline.

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
each slot. There are six fragments, one per rotation and direction of travel:
`fragments/casestudy/r{0,1,2}-{next,prev}.html`. `index.html` holds rotation 0.

**The rotations are copies, and a script keeps them honest.** Seven files contain
the same three case studies. That is the identical bargain the palette makes
across four files, and it is accepted for the same reason — the alternative is a
build step. `index.html` is the source of truth and `scripts/check_htmx.py` fails
CI if any rotation drifts from it, so editing one copy is a caught error rather
than a silent one.

**The animation runs backwards.** Because the new layout is already correct the
instant the DOM changes, the slide has to animate *out of* the slot each element
just left, *into* the one it now occupies. That is why `@keyframes fromItem1`,
`fromItem2` and `fromItem3` each contain **only a `from` block and no `to`**. The
end state is whatever the `:nth-child` rule assigns — writing a `to` block would
override it and break the effect. The `--casestudy-item1/2/3-*` custom properties
hold those starting positions, which is why they read as animation state rather
than theme values.

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

## Tech stack marquee

Seven rows of Boxicons glyphs scrolling horizontally, driven by the
`techstack--animate1` / `techstack--animate2` keyframes. It is decoration: the
rows are `aria-hidden="true"` as a whole, and the section is named by a
visually-hidden `<h2>`.

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
| `scripts/check_htmx.py` | A renamed fragment 404s in silence; a drifted rotation looks fine; a control without an `id` strands focus on `<body>` |
| `scripts/check_palette.py` | The palette exists in four copies (below) |
| `scripts/check_repo.py` | Deleting `.nojekyll` breaks paths with no build error; `sitemap.xml` drifts silently |
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
