# Architecture

How this site is put together, and why. For the rules that keep it working, see
[AGENTS.md](AGENTS.md).

## No build step

Plain HTML, CSS and vanilla JavaScript, served directly by GitHub Pages from
`main`. There is no bundler, no package manager, and no lockfile.

This is deliberate rather than incidental. The site is a personal homepage whose
content changes a few times a year; a toolchain would need reviving every time,
and a dependency tree would need patching in between. The tradeoff accepted in
exchange is that shared values live in CSS custom properties instead of build-time
constants, and the two external resources load from a CDN at runtime:

- [Poppins](https://fonts.google.com/specimen/Poppins) via Google Fonts
- [Boxicons](https://boxicons.com/) 2.1.4 via unpkg

Fonts are loaded with `<link rel="preconnect">` plus `<link rel="stylesheet">` in
`index.html`, **not** `@import` in the stylesheet. An `@import` would serialize the
request chain — the browser must parse `style.css` before it discovers the font
request — and delay first paint.

## Layout

```
index.html              single page: nav + home, case study, tech stack sections
404.html                self-contained not-found page
css/style.css           all styles, including the carousel slide animations
js/script.js            carousel controller (the only JavaScript)
assets/favicon.svg      favicon
assets/images/          profile photo (webp) and the 1200x630 social share card
.nojekyll               opt out of Jekyll processing (see Deployment)
robots.txt, sitemap.xml crawler hints
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

- `--casestudy-slide-duration` is read by `js/script.js`, so it must stay in
  milliseconds. See the carousel section below.
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

Two interactive controls are pure CSS, with no JavaScript involved:

- **Hero tabs** ("Dear All" / "My Services" / "For Recruiters") are three
  `<input type="radio">` elements, each followed by its `<label>` and its panel.
  The `input:checked + label + div` sibling chain reveals the matching panel.
- **Mobile sidebar** is the checkbox hack: a single `<input type="checkbox">`
  toggles the menu, with a full-screen `<label>` acting as the click-away overlay.

Both rely on the input being *focusable*, which is why they are visually hidden
rather than `display: none`. That constraint is documented in
[AGENTS.md](AGENTS.md#accessibility-invariants).

## The case study carousel

The only JavaScript in the site. The mechanism is unusual enough to be worth
spelling out.

**Position comes from DOM order.** Three `.casestudy--item` elements are styled by
`:nth-child`, and each slot has a fixed role:

| Slot              | Role       | State                                                     |
| ----------------- | ---------- | --------------------------------------------------------- |
| `:nth-child(1)`   | just left  | `opacity: 0`, off to the left, blurred, `pointer-events: none` |
| `:nth-child(2)`   | centre     | visible, unblurred, the only slot whose text is readable  |
| `:nth-child(3)`   | on deck    | visible but small and blurred, offset to the right        |

Advancing the carousel does not change any styles — it moves a node.
`showSlider('next')` appends the first item to the end of the list, so the centre
card slides out to slot 1 and the on-deck card is promoted into the centre.
`'prev'` prepends the last item instead, running the same shuffle backwards. The
`:nth-child` rules then re-apply themselves to whatever now sits in each slot.

**The animation runs backwards.** Because the new layout is already correct the
instant the DOM changes, the slide has to animate *out of* the slot each element
just left, *into* the one it now occupies. That is why `@keyframes fromItem1`,
`fromItem2` and `fromItem3` each contain **only a `from` block and no `to`**. The
end state is whatever the `:nth-child` rule assigns — writing a `to` block would
override it and break the effect. The `--casestudy-item1/2/3-*` custom properties
hold those starting positions, which is why they read as animation state rather
than theme values.

**Restarting requires a reflow.** Re-adding the same class does not replay a CSS
animation, so `js/script.js` removes the class, forces layout with
`void carousel.offsetWidth`, then adds it back.

**Duration has one source of truth.** `--casestudy-slide-duration` (700ms) drives
the CSS animations, and `js/script.js` reads the computed value to decide how long
to keep the arrows locked. Changing the CSS value alone is sufficient and cannot
drift out of sync — hence the millisecond requirement, since the parser only
handles `ms` and `s`.

**Reduced motion short-circuits the whole thing.** Under
`prefers-reduced-motion: reduce` the CSS animations are off, so the JavaScript
skips straight to its settled state instead of waiting on an animation that will
never fire.

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
branch, root directory. There is no CI: the site has no build to verify and no
test suite, so a workflow would only add latency.

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
