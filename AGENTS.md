# AGENTS.md

Working notes for anyone — human or agent — changing this repository. For how the
site is built and why, read [ARCHITECTURE.md](ARCHITECTURE.md) first.

## The shape of the repo

A static personal site: plain HTML, CSS and vanilla JavaScript, no build step, no
dependencies, served by GitHub Pages from `main`. There is nothing to install and
nothing to compile. Edit the files directly.

Do not introduce a bundler, framework or package manager to solve a problem that a
few lines of CSS would solve. The absence of a toolchain is a design decision, not
an oversight — see [ARCHITECTURE.md](ARCHITECTURE.md#no-build-step).

## Local development

Opening `index.html` in a browser works for most changes. To match production —
absolute paths resolve, MIME types are correct — serve it:

```sh
python3 -m http.server 8000
# then open http://localhost:8000
```

**Browsers cache aggressively on localhost.** If a change to `css/style.css` or
`js/script.js` appears not to have taken effect, confirm the server is sending the
new bytes (`curl -s localhost:8000/js/script.js | head`) before concluding the code
is wrong. Starting a server on a different port gives you a fresh cache key.

## Verifying a change

There is no test suite. Verify by hand, in roughly this order:

1. `node --check js/script.js` if the JavaScript changed.
2. Load the page and confirm the console is clean.
3. Tab through the whole page. Every interactive control must be reachable and
   show a visible focus ring.
4. Operate the carousel **from the keyboard**, not just by clicking. Focus the
   Next arrow, activate it, and check that focus is still on that arrow
   afterwards. Clicking with a mouse does not exercise this and will hide a
   regression.
5. Check the layout at each breakpoint (1200 / 850 / 750px) and with
   `prefers-reduced-motion: reduce` enabled.
6. For anything touching markup, metadata or colour, run a Lighthouse navigation
   audit. Accessibility, Best Practices and SEO are all at 100 — keep them there.

## Accessibility invariants

Four things in this codebase look like mistakes and are load-bearing. Each has an
inline comment; do not "clean them up".

- **The hero tab radios and the mobile menu checkbox are visually hidden, not
  `display: none`.** `display: none` removes an element from the tab order *and*
  the accessibility tree. Using it here made the "My Services" and "For Recruiters"
  panels unreachable by keyboard and invisible to screen readers, because both
  controls are CSS-only patterns that depend on the input staying focusable. Use
  the `.visually-hidden` class.
- **The carousel arrows use `aria-disabled` while a slide is in flight, never the
  `disabled` property.** Disabling the element the user just pressed moves focus to
  `<body>`, stranding keyboard users outside the carousel with no way back. The
  `isSliding` guard in `js/script.js` is what actually prevents re-entry;
  `aria-disabled` only communicates the state, and a CSS opacity rule provides the
  visual affordance.
- **Decorative Boxicons glyphs carry `aria-hidden="true"`, and the tech stack
  marquee rows are hidden as a whole.** Without this a screen reader announces
  hundreds of meaningless list items.
- **The case study and tech stack sections carry a `visually-hidden` `<h2>`.** Both
  are nav destinations whose visible content is decorative or image-based. Without
  the heading, heading navigation lands in an unnamed empty region.

Two further rules for any change:

- **Animations stay behind a `prefers-reduced-motion` guard.** `js/script.js`
  checks the same media query, so removing the CSS guard alone would leave the
  carousel arrows locked forever waiting on an animation that never runs.
- **Text contrast stays at or above 4.5:1.** This is why
  `--primary-color-text` exists alongside `--primary-color`; do not substitute one
  for the other to "keep the colours consistent".

## Other things not to break

- **`.nojekyll` must stay.** Deleting it makes any path starting with `_` or `.`
  disappear from the published site with no build error. See
  [ARCHITECTURE.md](ARCHITECTURE.md#why-nojekyll-matters).
- **`@keyframes fromItem1/2/3` have no `to` block.** That is correct; the end state
  comes from the `:nth-child` rules. Adding one breaks the slide.
- **`void carousel.offsetWidth` is not dead code.** It forces the reflow that lets
  the animation restart.
- **`--casestudy-slide-duration` must stay in `ms`.** `js/script.js` parses it.
- **`404.html` styles are inline on purpose.** It must render even if the
  stylesheet is what failed.

## Contributing

Every change goes through a pull request; nothing is committed to `main` directly,
because pushing to `main` publishes to the live site immediately.

Branch names follow `type/tatangharyadi/short-description`.

Keep `sitemap.xml` in step when adding or removing a page.
