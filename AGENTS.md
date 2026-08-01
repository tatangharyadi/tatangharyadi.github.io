# AGENTS.md

Working notes for anyone — human or agent — changing this repository. For how the
site is built and why, read [ARCHITECTURE.md](ARCHITECTURE.md) first.

## The shape of the repo

A static personal site: plain HTML and CSS, no build step, no package manager,
served by GitHub Pages from `main`. There is nothing to install and nothing to
compile. Edit the files directly.

**There is no JavaScript file of our own.** The interaction — the hero tabs and the
case study carousel — is [htmx](https://htmx.org) asking for static HTML fragments
under `fragments/` and swapping them in. htmx arrives from a CDN pinned by version
and SRI digest, the same way Boxicons does. Adding a `js/` file back is a real
decision, not a shortcut: read
[ARCHITECTURE.md](ARCHITECTURE.md#interaction-patterns) first and say in the pull
request why markup could not carry it.

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

**A file:// page cannot fetch fragments.** Opening `index.html` directly now leaves
the tabs and the carousel inert, because htmx issues real requests and the browser
blocks cross-origin `file://` reads. Serve the site for anything touching
interaction.

**Browsers cache aggressively on localhost.** If a change to `css/style.css` or to
a file under `fragments/` appears not to have taken effect, confirm the server is
sending the new bytes (`curl -s localhost:8000/fragments/hero/dear-all.html | head`)
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
   fragment that 404s, a case study copy that drifted, and a control that lost the
   `id` htmx re-focuses by.
2. Load the page **from a server** and confirm the console is clean *and* that no
   request in the network panel returned 404. A missing fragment is a silent
   no-op on click, not an error.
3. Tab through the whole page. Every interactive control must be reachable and
   show a visible focus ring.
4. Operate the carousel and the hero tabs **from the keyboard**, not just by
   clicking. Focus the Next arrow, activate it, and check that focus is still on
   that arrow afterwards; do the same for a hero tab. Clicking with a mouse does
   not exercise this and will hide a regression. This is the check that catches a
   swapped-in control missing its `id` — see the invariant below.
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

Five things in this codebase look like mistakes and are load-bearing. Each has an
inline comment; do not "clean them up".

- **Every control htmx drives keeps a stable `id`, and swapped-in fragments reuse
  the same ones.** This is the invariant the whole shape of the carousel and the
  tabs exists to protect. After a swap htmx looks the previously focused element up
  again by `document.getElementById`; if the incoming markup has no element with
  that `id`, focus falls to `<body>` and a keyboard user is stranded outside the
  component with no way back. So `#prev`, `#next` and `#hero-tab-1..3` are fixed
  names, not decoration — renaming one, or adding a focusable control to a fragment
  without an `id`, breaks keyboard operation while looking perfectly fine to a
  mouse. `scripts/check_htmx.py` fails CI on an `hx-get` with no `id`, and step 4
  of "Verifying a change" is how you catch the rest.
- **The mobile menu checkbox is visually hidden, not `display: none`.**
  `display: none` removes an element from the tab order *and* the accessibility
  tree. The sidebar is a CSS-only pattern that depends on its checkbox staying
  focusable, so hiding it that way makes the menu unopenable by keyboard and
  invisible to screen readers. Use the `.visually-hidden` class. (The hero tabs
  were the same pattern with radios until they became real `role="tab"` buttons;
  the sidebar stays CSS because it has nothing to fetch.)
- **The carousel arrows never use the `disabled` property — re-entry is guarded by
  `hx-sync`.** Disabling the element the user just pressed moves focus to `<body>`,
  which is the same failure as losing an `id`. `hx-sync="closest
  .casestudy--container:replace"` is what actually prevents a second press from
  queueing behind the first, and the `button.htmx-request` opacity rule provides
  the visual affordance while the request is in flight. `aria-disabled` is
  acceptable where a state genuinely needs announcing; `disabled` is not, ever.
- **Decorative Boxicons glyphs carry `aria-hidden="true"`, and the tech stack
  marquee rows are hidden as a whole.** Without this a screen reader announces
  hundreds of meaningless list items.
- **The case study and tech stack sections carry a `visually-hidden` `<h2>`.** Both
  are nav destinations whose visible content is decorative or image-based. Without
  the heading, heading navigation lands in an unnamed empty region.

Three further rules for any change:

- **Animations stay behind a `prefers-reduced-motion` guard.** Nothing waits on an
  animation to finish any more, so the old failure mode — arrows locked forever
  because a timer never fired — is gone. The guard is now purely about honouring the
  preference, which is reason enough: keep it.
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
- **`.next` and `.prev` belong on `.casestudy--list`, not on the container.** The
  direction of travel arrives in the fragment that was served, so it is a property
  of the incoming list. There is no longer any reflow nudge and none is needed: a
  swap inserts freshly parsed nodes, and a new element's animations start on their
  own.
- **`--casestudy-slide-duration` is read only by the stylesheet.** Nothing parses it
  any more, so the unit is free. Older comments insisting on `ms` are obsolete.
- **The three case studies exist in seven files** — `index.html` (rotation 0) plus
  `fragments/casestudy/r{0,1,2}-{next,prev}.html`. This is the same bargain the
  palette makes across four files, and it has the same enforcement: `index.html` is
  the source of truth, and `scripts/check_htmx.py` fails CI if any rotation drifts
  from it. Edit `index.html` and propagate; do not edit the one copy you happen to
  be looking at.
- **Fragments carry no colour and no `<head>`.** They are pieces of a page. Putting
  a hex value in one would escape `scripts/check_palette.py`, which reads only the
  four files that hold the palette. `robots.txt` disallows `/fragments/` for the
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
