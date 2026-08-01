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

There is no test suite. Verify by hand, in roughly this order.

Steps marked **[CI]** also run automatically on every pull request — see
[ARCHITECTURE.md](ARCHITECTURE.md#continuous-integration). The rest are yours:
they need a real browser and someone looking at the result, which is why CI does
not pretend to cover them. **A green CI does not mean a change is verified.**

1. **[CI]** `node --check js/script.js` if the JavaScript changed.
2. Load the page and confirm the console is clean.
3. Tab through the whole page. Every interactive control must be reachable and
   show a visible focus ring.
4. Operate the carousel **from the keyboard**, not just by clicking. Focus the
   Next arrow, activate it, and check that focus is still on that arrow
   afterwards. Clicking with a mouse does not exercise this and will hide a
   regression.
5. Check the layout at each breakpoint (1200 / 850 / 750px) and with
   `prefers-reduced-motion: reduce` enabled.
6. **Check both colour schemes.** The site themes itself off
   `prefers-color-scheme`, so every visual change has two results. A
   default-scheme-only check leaves half the work unverified.
7. For anything touching markup, metadata or colour, run a Lighthouse navigation
   audit — **once per scheme**. Accessibility, Best Practices and SEO are all at
   100 in both; keep them there.
8. **[CI]** If `DESIGN.md` changed, `npx @google/design.md lint DESIGN.md`. It is
   at **0 errors and 0 warnings**; keep it there. Note the linter exits `0` on
   warnings, so read the summary rather than the exit code — CI gates on the JSON
   for that reason. This is a one-off command, not a dependency — the repo still
   has no `package.json`.
9. **[CI]** If any colour changed, `python3 scripts/check_palette.py`. The palette
   is written out in **four** places and nothing but this script keeps them in
   step; see the bullet under "Other things not to break".

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

Three further rules for any change:

- **Animations stay behind a `prefers-reduced-motion` guard.** `js/script.js`
  checks the same media query, so removing the CSS guard alone would leave the
  carousel arrows locked forever waiting on an animation that never runs.
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
- **`@keyframes fromItem1/2/3` have no `to` block.** That is correct; the end state
  comes from the `:nth-child` rules. Adding one breaks the slide.
- **`void carousel.offsetWidth` is not dead code.** It forces the reflow that lets
  the animation restart.
- **`--casestudy-slide-duration` must stay in `ms`.** `js/script.js` parses it.
- **`404.html` styles are inline on purpose.** It must render even if the
  stylesheet is what failed. Its palette is therefore a hand-kept copy of the one
  in `css/style.css` — change one and change the other.
- **The palette is written out in four places.** `css/style.css` is the source of
  truth; `404.html`, `DESIGN.md`'s front matter and the `theme-color` metas are
  copies. `scripts/check_palette.py` is what holds them together — run it after any
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
