---
version: alpha
name: Tatang Haryadi — Personal Site
description: >-
  A single-page personal portfolio themed with Catppuccin: Latte in light mode,
  Mocha in dark, selected by prefers-color-scheme. Calm and layered rather than
  high-contrast, with accessibility floors treated as hard constraints.

# The unprefixed tokens are Catppuccin Latte — light mode, and the default
# because it is the tighter of the two flavours for contrast. The `mocha-*`
# tokens are the dark-mode counterparts. See "Why the dark tokens are
# duplicated" under Colors for why this is two flat sets rather than one set of
# { light, dark } pairs.
colors:
  primary: "#026389"
  accent: "#04a5e5"
  surface: "#eff1f5"
  surface-alt: "#e6e9ef"
  surface-chrome: "#dce0e8"
  on-surface: "#4c4f69"
  on-surface-muted: "#5c5f77"
  on-surface-subtle: "#acb0be"
  border: "#acb0be"
  mocha-primary: "#89dceb"
  mocha-accent: "#89dceb"
  mocha-surface: "#1e1e2e"
  mocha-surface-alt: "#181825"
  mocha-surface-chrome: "#11111b"
  mocha-on-surface: "#cdd6f4"
  mocha-on-surface-muted: "#a6adc8"
  mocha-on-surface-subtle: "#585b70"
  mocha-border: "#585b70"

typography:
  display:
    fontFamily: Poppins
    fontSize: 6rem
    fontWeight: 400
    lineHeight: 1
  headline-lg:
    fontFamily: Poppins
    fontSize: 2.5rem
    fontWeight: 400
    lineHeight: 3rem
  headline-md:
    fontFamily: Poppins
    fontSize: 2em
    fontWeight: 500
    lineHeight: 1.2
  logo:
    fontFamily: Poppins
    fontSize: 1.5rem
    fontWeight: 600
  body-md:
    fontFamily: Poppins
    fontSize: 1rem
    fontWeight: 400
  body-sm:
    fontFamily: Poppins
    fontSize: 15px
    fontWeight: 400
  label:
    fontFamily: Poppins
    fontSize: 1rem
    fontWeight: 400
  label-active:
    fontFamily: Poppins
    fontSize: 1rem
    fontWeight: 600
  label-caps:
    fontFamily: Poppins
    fontSize: 1rem
    fontWeight: 600
    letterSpacing: 2px

rounded:
  xs: 2px
  full: 9999px

spacing:
  xs: 0.5rem
  sm: 1rem
  md: 1.5rem
  lg: 2rem
  xl: 3rem
  xxl: 4rem
  max-width: 1200px

components:
  page:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.on-surface}"
    typography: "{typography.body-md}"
  nav:
    backgroundColor: "{colors.surface-chrome}"
    textColor: "{colors.on-surface}"
    typography: "{typography.logo}"
  hero-tab:
    backgroundColor: "{colors.surface-alt}"
    textColor: "{colors.on-surface-muted}"
    typography: "{typography.label}"
  hero-tab-active:
    backgroundColor: "{colors.surface-alt}"
    textColor: "{colors.primary}"
    typography: "{typography.label-active}"
  social-pill:
    backgroundColor: "{colors.surface-chrome}"
    textColor: "{colors.on-surface}"
    rounded: "{rounded.full}"
    size: 40px
  carousel-arrow:
    backgroundColor: "{colors.surface-chrome}"
    textColor: "{colors.on-surface}"
    rounded: "{rounded.full}"
    size: 40px
  divider:
    backgroundColor: "{colors.border}"
    height: 1px
  techstack-glyph:
    textColor: "{colors.on-surface-subtle}"
  techstack-glyph-hover:
    textColor: "{colors.accent}"
  focus-ring:
    textColor: "{colors.primary}"
    rounded: "{rounded.xs}"
  page-dark:
    backgroundColor: "{colors.mocha-surface}"
    textColor: "{colors.mocha-on-surface}"
  nav-dark:
    backgroundColor: "{colors.mocha-surface-chrome}"
    textColor: "{colors.mocha-on-surface}"
  hero-tab-dark:
    backgroundColor: "{colors.mocha-surface-alt}"
    textColor: "{colors.mocha-on-surface-muted}"
  hero-tab-active-dark:
    backgroundColor: "{colors.mocha-surface-alt}"
    textColor: "{colors.mocha-primary}"
  social-pill-dark:
    backgroundColor: "{colors.mocha-surface-chrome}"
    textColor: "{colors.mocha-on-surface}"
  carousel-arrow-dark:
    backgroundColor: "{colors.mocha-surface-chrome}"
    textColor: "{colors.mocha-on-surface}"
  divider-dark:
    backgroundColor: "{colors.mocha-border}"
  techstack-glyph-dark:
    textColor: "{colors.mocha-on-surface-subtle}"
  techstack-glyph-hover-dark:
    textColor: "{colors.mocha-accent}"
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
| `accent`            | `--accent`      | decoration only (see below)              | `sky` `#04a5e5`    | `sky` `#89dceb`    |
| `surface`           | `--bg`          | page base, case study section            | `base` `#eff1f5`   | `base` `#1e1e2e`   |
| `surface-alt`       | `--bg-alt`      | alternating section (hero, sidebar)      | `mantle` `#e6e9ef` | `mantle` `#181825` |
| `surface-chrome`    | `--bg-chrome`   | nav band, tech stack band, pills, arrows | `crust` `#dce0e8`  | `crust` `#11111b`  |
| `on-surface`        | `--text`        | primary text; control outlines           | `text` `#4c4f69`   | `text` `#cdd6f4`   |
| `on-surface-muted`  | `--text-muted`  | secondary text                           | `subtext1` `#5c5f77` | `subtext0` `#a6adc8` |
| `on-surface-subtle` | `--text-subtle` | decorative glyphs only                   | `surface2` `#acb0be` | `surface2` `#585b70` |
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
deliberately breaking `hero-tab-dark` and confirming the rule fires.

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

### The accent rule

**`accent` may only be used on `aria-hidden` decoration. Anything a visitor has
to read or aim at — text of any size, and every focus ring — uses `primary`.**
This is a rule, not a description; the contrast figures below only hold if it is
followed.

The rule is stricter than the usual "large text may be lighter" allowance, because
Latte `sky` fails even the **3:1** large-text and UI-component floor, not just the
4.5:1 small-text one. There is no size at which it becomes legal. `accent`
consequently survives in exactly one place today: the hover colour of the tech
stack marquee glyphs, which are decorative and `aria-hidden` as a whole.

The 404 page's `<h1>` is the case that proves the point — at `clamp(3rem, 12vw,
6rem)` it is the largest type on the site, and it still uses `primary`.

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

- **Decorative fills and glyphs** — `on-surface-subtle` and `accent` on the
  marquee, the `border` hairlines, the case study dividers. All `aria-hidden` or
  purely ornamental, so they are tuned to be quiet rather than legible. The
  `techstack-glyph` components therefore declare only a `textColor` and no
  background: they are decoration, and a contrast pair would assert a
  legibility requirement that does not apply.
- **The social pill shape** (`surface-chrome` on `surface-alt`, 1.09:1). WCAG
  1.4.11 requires 3:1 only for a boundary *needed to identify* a control; here the
  icon inside does that at 6.04:1 and carries the accessible name. The circle is
  ornament. Its focus ring, which is not ornament, is `primary` at 5.04:1.

The carousel arrows are the opposite case: their 1px border is `on-surface`, not
`border`, because there the outline genuinely is the control's boundary and
`border` would be 1.91:1 in Latte. The schema has no `borderColor` property, so
that value lives only in the CSS and in this sentence.

## Typography

Poppins throughout, loaded from Google Fonts via `preconnect` + `<link>` (see
[ARCHITECTURE.md](ARCHITECTURE.md#no-build-step) for why not `@import`). Only two
weights are in use — 400 for body copy and 600 for headings, the logo and the
active tab label — with 500 appearing once on the case study card title.

- **`display`** — the 404 page heading, and the only fluid type on the site. Its
  real value is `clamp(3rem, 12vw, 6rem)`; the token records the 6rem ceiling
  because `Dimension` cannot express a clamp.
- **`headline-lg`** — the hero title. Set at 400, not a heavier weight: at 2.5rem
  the size alone carries the hierarchy.
- **`headline-md`** — the case study card title, at `2em` of its card's own 15px
  context rather than a root-relative size, so it scales with the card.
- **`logo`** — the wordmark in the nav, and the one place a heavier weight is used
  purely for brand presence.
- **`body-md`** / **`body-sm`** — page copy, and the denser copy inside a case
  study card.
- **`label`** / **`label-active`** — the hero tab labels, inactive and active.
- **`label-caps`** — the hero subtitle, letterspaced at 2px.

The active hero tab is marked by **both** `primary` and the heavier
`label-active` weight. That redundancy is deliberate: WCAG 1.4.1 forbids
signalling state by colour alone, and it also means the tab state survives in
Latte where the accent is less vivid than the old bright cyan.

## Layout

A single centred column, capped at `spacing.max-width` (1200px) and padded
`spacing.sm` (1rem) at the edges. Sections alternate between `surface` and
`surface-alt` down the page; the nav is fixed to the top and the tech stack band
is full-bleed.

Spacing follows an 8px rhythm expressed in `rem` — every value in the stylesheet
is a multiple of `0.5rem`, from `xs` through `xxl`, with the hero's asymmetric
`10rem 1rem 5rem 1rem` clearing the fixed nav. There is no separate 4px half-step.

Three breakpoints, all written as `width <` queries:

| Breakpoint | What changes                                                     |
| ---------- | ---------------------------------------------------------------- |
| `1200px`   | the column stops being capped and becomes fluid                  |
| `850px`    | the hero's two columns collapse to one                           |
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

**So tone alone will not separate the nav from the content in either flavour, and
a 1px hairline does the work instead.** `nav`, `.techstack` and the mobile
slide-out menu each carry a `border` hairline on the edge facing the content. It
measures 1.78–2.63:1 against its neighbour, which is plenty for a line even
though it would be far too little for text.

The slide-out menu is the case where this is not optional. It is an overlay panel
on top of the page at `surface-alt` over `surface` — 1.07:1 in Mocha — so without
the `border-left` its links appear to float on the page rather than sit in a menu.

## Shapes

Almost nothing is rounded. The site's shape language is squared-off rectangular
bands, with roundness reserved for two things:

- **Circles**, for anything that reads as a token or a control rather than a
  region: the profile photo, the four social pills and the two carousel arrows.
  These are literally `border-radius: 50%` (`100%` on the photo) in the CSS;
  `rounded.full` is the token equivalent, since `Dimension` cannot express a
  percentage.
- **`rounded.xs` (2px)**, used only to soften the focus ring on the hero tab
  labels so the outline does not read as a hard box around text.

Section bands, cards and the nav have no radius at all. Do not add one to a
rectangular container to "modernise" it; the flat bands are what the hairlines in
[Elevation & Depth](#elevation--depth) are tuned against.

## Components

Every component below is CSS-only — there is no component framework, and the tab
panels and mobile menu are driven by hidden radio and checkbox inputs rather than
JavaScript. See [AGENTS.md](AGENTS.md#accessibility-invariants) before changing any
of them; several look like mistakes and are load-bearing.

- **`nav`** — fixed band on `surface-chrome`, wordmark in `primary`, links in
  `on-surface`. Carries a `border` hairline on its bottom edge.
- **`hero-tab`** / **`hero-tab-active`** — the *My Services* / *For Recruiters*
  panel switcher. The radios are visually hidden but **focusable**; the active
  label changes both colour and weight.
- **`social-pill`** — 40px circle, `surface-chrome` fill, icon in `on-surface`.
  The fill is ornament; the icon carries the accessible name.
- **`carousel-arrow`** — 40px circle, same fill, plus a 1px `on-surface` border
  that is the real boundary of the control. While a slide is in flight the arrow
  takes `aria-disabled` and reduced opacity — never the `disabled` property, which
  would move focus to `<body>`.
- **`divider`** — 1px `border` hairline. Decorative; also the band separator
  described under Elevation & Depth.
- **`techstack-glyph`** / **`techstack-glyph-hover`** — decorative marquee glyphs,
  `aria-hidden` as a whole, `on-surface-subtle` resting and `accent` on hover.
  The only place `accent` appears.
- **`focus-ring`** — `2px solid` `primary` with a 2–4px offset, on every
  interactive control in both flavours. Not optional and not restyleable per
  component.

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
- **Don't** use `accent` on anything a visitor reads or aims at. It is `sky`, and
  in Latte that is 2.47:1 — illegal at every font size. Use `primary`.
- **Don't** "fix" `primary` back to a Catppuccin colour. `#026389` is deliberate;
  no Latte cyan reaches 4.5:1 as text, and `primary` is also every focus ring.
- **Don't** signal state by colour alone. The active tab changes weight as well as
  colour, and that is a WCAG 1.4.1 requirement rather than a stylistic choice.
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
