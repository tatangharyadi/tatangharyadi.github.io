---
version: alpha
name: Tatang Haryadi — Personal Site
description: >-
  A personal portfolio site themed with Catppuccin: Latte in light mode,
  Mocha in dark, selected by prefers-color-scheme. Calm and layered rather than
  high-contrast, with accessibility floors treated as hard constraints.

# The unprefixed tokens are Catppuccin Latte — light mode, and the default
# because it is the tighter of the two flavours for contrast. The `mocha-*`
# tokens are the dark-mode counterparts. See "Why the dark tokens are
# duplicated" under Colors for why this is two flat sets rather than one set of
# { light, dark } pairs.
colors:
  primary: "#026389"
  surface: "#eff1f5"
  surface-alt: "#e6e9ef"
  surface-chrome: "#dce0e8"
  on-surface: "#4c4f69"
  on-surface-muted: "#5c5f77"
  border: "#acb0be"
  mocha-primary: "#89dceb"
  mocha-surface: "#1e1e2e"
  mocha-surface-alt: "#181825"
  mocha-surface-chrome: "#11111b"
  mocha-on-surface: "#cdd6f4"
  mocha-on-surface-muted: "#a6adc8"
  mocha-border: "#585b70"

# Two families, both already on the machine. See "Type" for why there is no
# webfont. `system-ui` and `ui-monospace` are the heads of the two stacks the
# stylesheet declares as --font-ui and --font-mono; the fallbacks are in the CSS.
typography:
  display:
    fontFamily: system-ui
    fontSize: 6rem
    fontWeight: 400
    lineHeight: 1
  headline-lg:
    fontFamily: system-ui
    fontSize: 3.25rem
    fontWeight: 650
    lineHeight: 1.1
    letterSpacing: -0.03em
  headline-md:
    fontFamily: system-ui
    fontSize: 1.75rem
    fontWeight: 600
    lineHeight: 1.25
    letterSpacing: -0.02em
  logo:
    fontFamily: ui-monospace
    fontSize: 1.125rem
    fontWeight: 600
    letterSpacing: -0.02em
  body-md:
    fontFamily: system-ui
    fontSize: 1rem
    fontWeight: 400
    lineHeight: 1.6
  body-sm:
    fontFamily: system-ui
    fontSize: 0.9375rem
    fontWeight: 400
  label-caps:
    fontFamily: ui-monospace
    fontSize: 0.8125rem
    fontWeight: 600
    letterSpacing: 0.14em
  label-caps-sm:
    fontFamily: ui-monospace
    fontSize: 0.75rem
    fontWeight: 400
    letterSpacing: 0.12em

rounded:
  full: 9999px

spacing:
  xs: 0.5rem
  sm: 1rem
  md: 1.5rem
  lg: 2rem
  xl: 3rem
  xxl: 4rem
  max-width: 1100px

components:
  page:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.on-surface}"
    typography: "{typography.body-md}"
  nav:
    backgroundColor: "{colors.surface-chrome}"
    textColor: "{colors.on-surface}"
    typography: "{typography.logo}"
  masthead-lead:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.on-surface-muted}"
    typography: "{typography.body-md}"
  section-label:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.primary}"
    typography: "{typography.label-caps}"
  social-pill:
    backgroundColor: "{colors.surface-alt}"
    textColor: "{colors.on-surface}"
    rounded: "{rounded.full}"
    typography: "{typography.body-sm}"
  work-entry:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.on-surface}"
    typography: "{typography.headline-md}"
  readout-label:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.on-surface-muted}"
    typography: "{typography.label-caps-sm}"
  divider:
    backgroundColor: "{colors.border}"
    height: 1px
  focus-ring:
    textColor: "{colors.primary}"
  page-dark:
    backgroundColor: "{colors.mocha-surface}"
    textColor: "{colors.mocha-on-surface}"
  nav-dark:
    backgroundColor: "{colors.mocha-surface-chrome}"
    textColor: "{colors.mocha-on-surface}"
  masthead-lead-dark:
    backgroundColor: "{colors.mocha-surface}"
    textColor: "{colors.mocha-on-surface-muted}"
  section-label-dark:
    backgroundColor: "{colors.mocha-surface}"
    textColor: "{colors.mocha-primary}"
  social-pill-dark:
    backgroundColor: "{colors.mocha-surface-alt}"
    textColor: "{colors.mocha-on-surface}"
  work-entry-dark:
    backgroundColor: "{colors.mocha-surface}"
    textColor: "{colors.mocha-on-surface}"
  readout-label-dark:
    backgroundColor: "{colors.mocha-surface}"
    textColor: "{colors.mocha-on-surface-muted}"
  divider-dark:
    backgroundColor: "{colors.mocha-border}"
  focus-ring-dark:
    textColor: "{colors.mocha-primary}"
---

# Design

The visual system for this site. [ARCHITECTURE.md](ARCHITECTURE.md) covers how the
site is built; this file covers what it looks like and why each value was chosen.

This file follows the [DESIGN.md format](https://github.com/google-labs-code/design.md):
machine-readable tokens in the front matter above, rationale in the prose below.
**The tokens are the normative values.** The prose explains how to apply them, and
in a few places records a constraint the token schema cannot express — those are
called out where they occur.

To check it after an edit:

```sh
npx @google/design.md lint DESIGN.md
```

That is a one-off command, not a dependency — the repo still has no build step and
no `package.json`. See [ARCHITECTURE.md](ARCHITECTURE.md#no-build-step).

## Overview

A single-page portfolio for a cloud and platform engineer. Its stated audience is
recruiters and hiring managers, so the tone is professional and unfussy: it should
read as considered and calm rather than energetic, and nothing should get in the
way of scanning the page quickly.

The palette is [Catppuccin](https://catppuccin.com):

- **[Latte](https://catppuccin.com/palette#flavor-latte)** in light mode
- **[Mocha](https://catppuccin.com/palette#flavor-mocha)** in dark mode

Which one applies is decided by `prefers-color-scheme` — the visitor's operating
system setting. Hex values were taken from the upstream
[`catppuccin/palette`](https://github.com/catppuccin/palette) `palette.json`, not
transcribed by hand.

The resulting composition is deliberately flatter and more layered than a
high-contrast design would be; that is a property of Catppuccin rather than a
compromise, and [Elevation & Depth](#elevation--depth) explains what carries
hierarchy instead of tone.

Accessibility floors are treated as constraints on the design, not as a
verification step afterwards. Where the palette and a WCAG floor disagree, the
floor wins — that is the whole reason `primary` is not a Catppuccin colour.

### Deliberate omissions

- **No in-page theme toggle.** `prefers-color-scheme` only. A toggle would need
  JavaScript, `localStorage`, and an inline blocking script to avoid a flash of
  the wrong theme on load — a lot of machinery for a site with no other stateful
  UI. Reasonable future work, not present today.
- **No other Catppuccin flavours.** Frappé and Macchiato are both dark; Mocha
  covers that mode. Adding them would need a toggle to be reachable at all.
- **No accent colour beyond the cyan family.** Catppuccin offers fourteen
  accents; using more would dilute a single-page personal site. Semantic colours
  (red for errors, green for success) can be added from the palette if the site
  ever grows UI that needs them.

## Colors

Nothing in the stylesheet refers to a Catppuccin colour by its palette name.
Every rule goes through a semantic CSS custom property, defined once for Latte in
`:root` and overridden for Mocha inside a single `prefers-color-scheme: dark`
block. Adding a third flavour would mean adding one block, not auditing every
rule.

The token names in the front matter map one-to-one onto those custom properties:

| Token               | CSS property    | Role                                     | Latte              | Mocha              |
| ------------------- | --------------- | ---------------------------------------- | ------------------ | ------------------ |
| `primary`           | `--accent-text` | all accent text + every focus ring       | **`#026389`**      | `sky` `#89dceb`    |
| `surface`           | `--bg`          | page base, search input field            | `base` `#eff1f5`   | `base` `#1e1e2e`   |
| `surface-alt`       | `--bg-alt`      | social pills, work panels, results, sidebar | `mantle` `#e6e9ef` | `mantle` `#181825` |
| `surface-chrome`    | `--bg-chrome`   | the nav band, and nothing else           | `crust` `#dce0e8`  | `crust` `#11111b`  |
| `on-surface`        | `--text`        | primary text; control outlines           | `text` `#4c4f69`   | `text` `#cdd6f4`   |
| `on-surface-muted`  | `--text-muted`  | secondary text                           | `subtext1` `#5c5f77` | `subtext0` `#a6adc8` |
| `border`            | `--border`      | decorative dividers and hairlines        | `surface2` `#acb0be` | `surface2` `#585b70` |

`on-surface-muted` intentionally uses a different palette slot per flavour: Latte
`subtext0` measures 4.37:1 on `base`, just under the floor, so Latte steps up to
`subtext1` while Mocha stays on `subtext0`.

There is no `surface0`-based token. The obvious candidate — Latte `surface0`
`#ccd0da` as the fill for the social pills — puts their hover state at 4.32:1,
just under AA. The pills use `surface-chrome` instead, which measures 5.04:1.

### Why the dark tokens are duplicated

The format has no light/dark axis. `colors` is a flat map of name to a single
colour value, and neither the spec nor the released tooling has a notion of modes,
themes or schemes. So the two flavours are carried as two flat sets — unprefixed
for Latte, `mocha-*` for Mocha — with a matching `-dark` component variant for
every component that names a colour.

This is not just a workaround. Because each `mocha-*` pair is reachable through a
component, the linter's WCAG check runs over the dark flavour too; a single set of
light-only tokens would leave half the system unverified by tooling. Verified by
deliberately breaking a `-dark` component and confirming the rule fires.

Upstream is moving towards inline per-token modes
([issue #13](https://github.com/google-labs-code/design.md/issues/13),
[PR #128](https://github.com/google-labs-code/design.md/pull/128)):

```text
colors:
  surface: { light: "#eff1f5", dark: "#1e1e2e" }
```

(Deliberately not tagged as a `yaml` block: the parser also reads fenced YAML as a
token source, so an illustrative example tagged `yaml` collides with the front
matter and the linter stops early.)

That is the shape to collapse to **once it ships**. It does not work today: the
released `alpha` linter reads the nested object as a token group, so every
`{colors.surface}` reference becomes a `broken-ref` **error** and `primary` stops
counting as a defined colour. Do not adopt it early. The format is version
`alpha` and expects to change.

### There is only one accent, and it is `primary`

**Every accent on this site — text of any size, and every focus ring — is
`primary`. Catppuccin `sky` is not in the token set at all.**

There is no decoration-only accent, and adding one back is a decision, not a
tidy-up. Latte `sky` fails even the **3:1** large-text and UI-component floor,
not just the 4.5:1 small-text one, so there is no size at which it becomes legal:
the 404 page's `<h1>` at `clamp(3rem, 12vw, 6rem)` is the largest type on the
site and it still uses `primary`. Do not reach for `sky` because it is the
Catppuccin-looking choice.

The split exists because Catppuccin Latte's entire cyan family is too light to
carry text on any Latte surface. Measured against Latte `base` `#eff1f5`:

| Latte accent          | Contrast on `base` | Verdict                     |
| --------------------- | ------------------ | --------------------------- |
| `sky` `#04a5e5`       | 2.47:1             | fails even the 3:1 UI floor |
| `sapphire` `#209fb5`  | 2.78:1             | fails even the 3:1 UI floor |
| `teal` `#179299`      | 3.31:1             | large text only             |
| `blue` `#1e66f5`      | 4.34:1             | large text only             |
| `mauve` `#8839ef`     | 4.79:1             | passes on `base` only       |

This matters beyond text: a `sky` focus ring in Latte would be 2.47:1 against its
own background, below the 3:1 that WCAG 1.4.11 requires of a non-text indicator.
The focus ring is the one thing on the page a keyboard user cannot do without, so
it uses `primary` in both flavours.

### `#026389` is the one non-Catppuccin value

`primary` in Latte is Latte `sky` deepened to 60% lightness with hue and
saturation preserved. It measures 5.90:1 on `surface`, 5.48:1 on `surface-alt` and
5.04:1 on `surface-chrome` — above 4.5:1 on every surface it can appear against.

It is the **only** colour in this system that is not straight from the palette.
Do not "correct" it back to `sky`; that reintroduces a 2.47:1 accent and an
illegible focus ring.

`mauve` `#8839ef` was the closest in-palette alternative and was rejected twice
over: it reaches 4.79:1 on `base` but only 4.09:1 on `crust`, so it would still
fail in the nav, and it shifts the brand hue from cyan to purple.

For what it is worth, the site's previous hand-picked accessible cyan was
`#006d94` — within 3% of the deepened Latte sky. The same problem had already been
solved the same way once.

### Contrast

Every foreground/background pair that actually occurs is verified in both
flavours: 20 pairs × 2 flavours, 0 failures. Small text is held to 4.5:1
(WCAG 1.4.3 AA), large text and non-text indicators to 3:1 (1.4.3 / 1.4.11).

Tightest margins, worth knowing before changing a value:

| Pair                                | Flavour | Ratio  | Floor |
| ----------------------------------- | ------- | ------ | ----- |
| `primary` on `surface-chrome`       | Latte   | 5.04:1 | 4.5   |
| `on-surface-muted` on `surface-alt` | Latte   | 5.14:1 | 4.5   |
| `primary` on `surface-alt`          | Latte   | 5.48:1 | 4.5   |

Latte is always the tighter flavour — Mocha's worst text pair is 7.37:1. **If you
change a colour, check it against Latte.**

Two categories sit below 3:1 deliberately, and the token schema has no way to say
so — hence this paragraph:

- **Decorative hairlines** — the `border` rules under the nav and the mobile
  menu, the rules between sections, and the rules between entries in the work
  index and the colophon readout. Purely ornamental, so they are tuned to be
  quiet rather than legible. The `divider` component therefore declares only a
  `backgroundColor` and no foreground: a contrast pair would assert a legibility
  requirement that does not apply.
- **The social pill outline** (`border` on `surface`, 1.91:1 in Latte). WCAG
  1.4.11 requires 3:1 only for a boundary *needed to identify* a control. These
  pills are now labelled — the link text is inside them, at 6.57:1 — so the outline
  is not what identifies them and the text is not relying on it. This is the change
  that retired the exemption the old icon-only pills needed: an unlabelled 40px
  circle really did depend on its shape, and now nothing does. The focus ring,
  which is never ornament, is `primary` at 5.9:1 on `surface`.

The schema has no `borderColor` property, so every 1px outline on the site lives
only in the CSS and in this prose.

## Typography

**Two families, no webfont.** Prose is `system-ui` and everything that is data
rather than prose is `ui-monospace`, both resolved from stacks declared as
`--font-ui` and `--font-mono` in the stylesheet. Poppins was removed in the
redesign, and its absence is the point: a webfont is a render-blocking request to
a host this site does not control, and a personal site whose whole argument is
about what it does and does not fetch cannot open by fetching a typeface for
decoration. See [ARCHITECTURE.md](ARCHITECTURE.md#no-build-step).

**The two families carry meaning, not variety.** Monospace marks anything that
describes the content rather than being it: section labels, the job title under
the name, work categories, and every label in the colophon readout. A reader does
not have to be told that rule for it to work, but a change that sets a paragraph
in mono, or a label in the UI face, breaks it.

- **`display`** — the 404 page heading, and the only type on the site with no
  upper bound worth recording. Its real value is `clamp(3rem, 12vw, 6rem)`; the
  token records the 6rem ceiling because `Dimension` cannot express a clamp.
- **`headline-lg`** — the name in the masthead, `clamp(2rem, 6vw, 3.25rem)` at
  weight 650 with `-0.03em` tracking. The negative tracking is what keeps a large
  system font from reading as a default, and it is the one place the weight is not
  400 or 600.
- **`headline-md`** — a work index title, `clamp(1.25rem, 3vw, 1.75rem)`. Fluid for
  the same reason the name is: these are the two sizes large enough for a narrow
  viewport to notice.
- **`logo`** — the wordmark, and the only monospace item that is not a label. It is
  set in mono because the wordmark is a name rendered as an identifier.
- **`body-md`** / **`body-sm`** — page copy at `line-height: 1.6`, and the denser
  copy in the colophon readout.
- **`label-caps`** / **`label-caps-sm`** — every section heading and the masthead
  role at the larger size; work categories and readout labels at the smaller. Both
  are uppercase monospace with open tracking, which is what makes them legible at
  13px and 12px.

**Body text is capped at `--measure` (68ch), not at `--max-width`.** Line length is
a legibility constraint counted in characters and the two are not
interchangeable; 1100px of running text is roughly twice a comfortable measure.
`--measure` is a `ch` value, which the token schema's `Dimension` type cannot
express, so it is recorded here only.

One principle survives the tab labels that used to demonstrate it: state is
signalled by **both** colour and a weight or shape change, never colour alone,
because WCAG 1.4.1 forbids the second and because a weight change survives in
Latte where the accent is less vivid than the old bright cyan.

## Layout

A single centred column, capped at `spacing.max-width` (1100px) and padded
`spacing.sm` (1rem) at the edges. The nav is fixed to the top.

**No section fills the viewport.** The page used to be a stack of `100dvh` bands,
one screenful each, alternating `surface` and `surface-alt`. That is the shape that
makes a page read as a template before a word of it is read: it forces a scroll
between every idea and tells the reader nothing about how much is left. Sections
are now as tall as their content and separated by a `border` hairline, so the page
has a length you can feel. The alternating fills went with the bands — the page is
`surface` throughout, and depth comes from rules rather than from tone.

Spacing follows an 8px rhythm expressed in `rem` — every value in the stylesheet
is a multiple of `0.5rem`, from `xs` through `xxl`, with the masthead's asymmetric
top padding clearing the fixed nav. There is no separate 4px half-step.

`html` carries `scroll-padding-top: 6.5rem`. Every nav link is an in-page anchor
and the nav is fixed, so without it a section heading lands underneath the bar.

Three breakpoints, all written as `width <` queries:

| Breakpoint | What changes                                                     |
| ---------- | ---------------------------------------------------------------- |
| `1200px`   | the colophon readout drops to one label/value pair per row       |
| `850px`    | the masthead portrait moves below the identity block             |
| `750px`    | the nav links move into the CSS-only slide-out sidebar           |

Breakpoints are not spacing tokens and the schema has no place for them, so they
are recorded here only.

## Elevation & Depth

There are no shadows anywhere on the site. Depth is tonal — the
`surface-chrome` / `surface-alt` / `surface` triad maps directly onto Catppuccin's
`crust` / `mantle` / `base` — and that triad is built for *stacked* surfaces that
read as gently layered, not as separate zones. The three sit close together in
**both** flavours:

| Band pair                         | Latte  | Mocha  |
| --------------------------------- | ------ | ------ |
| `surface-chrome` vs `surface`     | 1.17:1 | 1.14:1 |
| `surface-alt` vs `surface`        | 1.09:1 | 1.07:1 |

This is worth knowing because it is the most visible consequence of adopting
Catppuccin. The previous design was a high-contrast, mixed-brightness page:
near-black navy nav and tech stack bands (`#081b29`) framing near-white hero and
case study sections (`#faf5ff`) — a ratio of about 15:1 between them. Dark mode
does not rescue it either; Mocha's `crust` band is, if anything, marginally
flatter against its page than Latte's.

**So tone alone will not separate anything from anything in either flavour, and a
1px hairline does the work instead.** This was true of the nav before the redesign
and it is now the site's entire structural vocabulary: the nav edge, the boundary
between sections, the rules between work entries, the rows of the colophon readout
and the left rule on an expanded work panel. It measures 1.78–2.63:1 against its
neighbour, which is plenty for a line even though it would be far too little for
text.

The one hairline that is not `border` is the left rule on a work panel, which is
`primary`. There it is not a divider but an indicator that the content beside it
arrived in response to something the reader did, so it is allowed to be seen.

The slide-out menu is the case where this is not optional. It is an overlay panel
on top of the page at `surface-alt` over `surface` — 1.07:1 in Mocha — so without
the `border-left` its links appear to float on the page rather than sit in a menu.

## Shapes

Almost nothing is rounded. The site's shape language is squared-off rectangular
bands, with roundness reserved for one thing:

- **The profile photo**, a literal `border-radius: 50%`.
- **The three social pills**, `border-radius: 999px` on a shape wider than it is
  tall, so the radius resolves to a stadium rather than a circle. `rounded.full`
  is the token for both cases, since `Dimension` cannot express a percentage and
  the schema has no stadium.

Nothing else. There was a `rounded.xs` (2px), softening the focus ring on the
old hero tab labels so the outline did not read as a hard box around text; it left
with the tabs. Every remaining focus ring sits on a control with its own shape,
so the ring follows that shape and needs no radius of its own.

Sections, panels and the nav have no radius at all. Do not add one to a
rectangular container to "modernise" it; the flat bands are what the hairlines in
[Elevation & Depth](#elevation--depth) are tuned against.

## Components

Every component below is CSS-only — there is no component framework, and the
mobile menu is driven by a hidden checkbox rather than JavaScript. See [AGENTS.md](AGENTS.md#accessibility-invariants) before changing any
of them; several look like mistakes and are load-bearing.

- **`nav`** — fixed band on `surface-chrome`, wordmark in `primary`, links in
  `on-surface`. Carries a `border` hairline on its bottom edge.
- **`masthead-lead`** — the positioning paragraph under the name, in
  `on-surface-muted`, capped at `--measure`. Plain prose, not a control.
- **`section-label`** — the uppercase monospace heading over each section, in
  `primary`. It is an `<h2>` doing the job of a label: small, tracked open, and
  deliberately not competing with the entry titles beneath it.
- **`social-pill`** — a labelled stadium, `surface-alt` fill with a `border`
  outline, text in `on-surface`. The text is the accessible name; the fill and the
  outline are both ornament. It used to be an icon-only circle, which made the
  shape load-bearing for identification — see [Contrast](#contrast).
- **`work-entry`** — a title link over a line of scope, separated from its
  neighbours by a `border` rule. The title is underlined in `border` and turns
  `primary` on hover or focus, so it is never colour alone that marks it as a link.
  While its fragment is in flight the link takes reduced opacity and nothing else:
  no `disabled`, ever, which would move focus to `<body>`.
- **`readout-label`** — the monospace term in the colophon's measurement grid, in
  `on-surface-muted` against its `on-surface` value. The contrast between the two
  is the whole hierarchy of that block.
- **`divider`** — 1px `border` hairline. Decorative; the section separator
  described under Elevation & Depth.
- **`focus-ring`** — `2px solid` `primary` with a 2–4px offset, on every
  interactive control in both flavours. Not optional and not restyleable per
  component.

One schema gap to read past: component properties cover fill and text, but there
is no stroke or `borderColor`. `focus-ring` therefore carries its *outline* colour
in `textColor` — that is the only colour slot available, so read it as a stroke,
not a label. Its `2px` width and 2–4px offset have no schema home at all and live
only in this prose, as do the `border` outline on `social-pill`, the underline on
a `work-entry` title and the `primary` left rule on an expanded work panel.

Each of these has a `-dark` variant in the front matter carrying its Mocha
colours. Those variants exist to make the dark flavour machine-readable and
contrast-checkable; they are not separate components in the markup.

## Do's and Don'ts

- **Do** change the semantic token, never the rule that uses it.
- **Do** check every colour change against **Latte**. It is always the tighter
  flavour, and a value that passes in Mocha can fail in Latte by a wide margin.
- **Do** keep small text at 4.5:1 or above, and large text (≥24px, or ≥18.66px
  bold), focus rings and control boundaries at 3:1 or above.
- **Do** run `npx @google/design.md lint DESIGN.md` after editing this file, and a
  Lighthouse navigation audit **in both schemes** after editing colour — see
  [AGENTS.md](AGENTS.md#verifying-a-change).
- **Don't** reintroduce Catppuccin `sky` as a colour token. In Latte it is
  2.47:1, illegal at every font size and below the 3:1 focus-ring floor too. It
  was removed with the tech stack marquee, which was the only decoration quiet
  enough to carry it legally.
- **Don't** "fix" `primary` back to a Catppuccin colour. `#026389` is deliberate;
  no Latte cyan reaches 4.5:1 as text, and `primary` is also every focus ring.
- **Don't** signal state by colour alone. Nothing on the site does so today, and
  WCAG 1.4.1 makes that a requirement rather than a stylistic choice.
- **Don't** add a `surface0`-based fill. Latte `surface0` puts pill hover at
  4.32:1, just under AA.
- **Don't** rely on tone to separate a band from the page. Use a `border`
  hairline; the Catppuccin triad is too tight in both flavours.
- **Don't** adopt the inline `{ light, dark }` token shape until upstream ships
  it. Today it produces `broken-ref` errors.

### Changing a colour

1. Change the semantic token, not the rule that uses it — in `css/style.css`, and
   in the hand-kept copy inside `404.html`.
2. Update the table under [Colors](#colors) and the matching `colors` entry in the
   front matter. Both flavours, if both change.
3. Re-verify against **Latte** on every surface the token can appear against — for
   `primary` and `on-surface` that is all three of `surface`, `surface-alt` and
   `surface-chrome`.
4. Run the linter and a Lighthouse navigation audit in both schemes.
