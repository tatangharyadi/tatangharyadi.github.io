# Design

The visual system for this site. [ARCHITECTURE.md](ARCHITECTURE.md) covers how the
site is built; this file covers what it looks like and why each value was chosen.

## Palette

The site uses [Catppuccin](https://catppuccin.com):

- **[Latte](https://catppuccin.com/palette#flavor-latte)** in light mode
- **[Mocha](https://catppuccin.com/palette#flavor-mocha)** in dark mode

Which one applies is decided by `prefers-color-scheme` — the visitor's operating
system setting. There is no in-page theme toggle; see [Deliberate
omissions](#deliberate-omissions).

Hex values were taken from the upstream
[`catppuccin/palette`](https://github.com/catppuccin/palette) `palette.json`, not
transcribed by hand.

## Semantic tokens

Nothing in the stylesheet refers to a Catppuccin colour by its palette name.
Everything goes through a semantic token, defined once for Latte in `:root` and
overridden for Mocha inside a single `prefers-color-scheme: dark` block. Adding a
third flavour would mean adding one block, not auditing every rule.

| Token           | Role                                  | Latte                      | Mocha                      |
| --------------- | ------------------------------------- | -------------------------- | -------------------------- |
| `--bg`          | page base, case study section         | `base` `#eff1f5`           | `base` `#1e1e2e`           |
| `--bg-alt`      | alternating section (hero, sidebar)   | `mantle` `#e6e9ef`         | `mantle` `#181825`         |
| `--bg-chrome`   | nav band, tech stack band, pills, arrows | `crust` `#dce0e8`     | `crust` `#11111b`          |
| `--border`      | decorative dividers and hairlines     | `surface2` `#acb0be`       | `surface2` `#585b70`       |
| `--text`        | primary text; control outlines        | `text` `#4c4f69`           | `text` `#cdd6f4`           |
| `--text-muted`  | secondary text                        | `subtext1` `#5c5f77`       | `subtext0` `#a6adc8`       |
| `--text-subtle` | decorative glyphs only                | `surface2` `#acb0be`       | `surface2` `#585b70`       |
| `--accent`      | decoration only (see below)           | `sky` `#04a5e5`            | `sky` `#89dceb`            |
| `--accent-text` | all accent text + every focus ring    | **`#026389`** (see below)  | `sky` `#89dceb`            |

`--text-muted` intentionally uses a different palette slot per flavour: Latte
`subtext0` measures 4.37:1 on `base`, just under the floor, so Latte steps up to
`subtext1` while Mocha stays on `subtext0`.

There is no `surface0`-based token. The obvious candidate — Latte `surface0`
`#ccd0da` as the fill for the social pills — puts their hover state at 4.32:1,
just under AA. The pills use `--bg-chrome` instead, which measures 5.04:1.

## The accent rule

**`--accent` may only be used on `aria-hidden` decoration. Anything a visitor has
to read or aim at — text of any size, and every focus ring — uses
`--accent-text`.** This is a rule, not a description; the contrast figures below
only hold if it is followed.

The rule is stricter than the usual "large text may be lighter" allowance, because
Latte `sky` fails even the **3:1** large-text and UI-component floor, not just the
4.5:1 small-text one. There is no size at which it becomes legal. `--accent`
consequently survives in exactly one place today: the hover colour of the tech
stack marquee glyphs, which are decorative and `aria-hidden` as a whole.

The 404 page's `<h1>` is the case that proves the point — at `clamp(3rem, 12vw,
6rem)` it is the largest type on the site, and it still uses `--accent-text`.

The split exists because Catppuccin Latte's entire cyan family is too light to
carry text on any Latte surface. Measured against Latte `base` `#eff1f5`:

| Latte accent          | Contrast on `base` | Verdict                    |
| --------------------- | ------------------ | -------------------------- |
| `sky` `#04a5e5`       | 2.47:1             | fails even the 3:1 UI floor |
| `sapphire` `#209fb5`  | 2.78:1             | fails even the 3:1 UI floor |
| `teal` `#179299`      | 3.31:1             | large text only            |
| `blue` `#1e66f5`      | 4.34:1             | large text only            |
| `mauve` `#8839ef`     | 4.79:1             | passes on `base` only       |

This matters beyond text: a `sky` focus ring in Latte would be 2.47:1 against its
own background, below the 3:1 that WCAG 1.4.11 requires of a non-text indicator.
The focus ring is the one thing on the page a keyboard user cannot do without, so
it uses `--accent-text` in both flavours.

### `#026389` is the one non-Catppuccin value

`--accent-text` in Latte is Latte `sky` deepened to 60% lightness with hue and
saturation preserved. It measures 5.90:1 on `base`, 5.48:1 on `mantle` and 5.04:1
on `crust` — above 4.5:1 on every surface it can appear against.

It is the **only** colour in this system that is not straight from the palette.
Do not "correct" it back to `sky`; that reintroduces a 2.47:1 accent and an
illegible focus ring.

`mauve` `#8839ef` was the closest in-palette alternative and was rejected twice
over: it reaches 4.79:1 on `base` but only 4.09:1 on `crust`, so it would still
fail in the nav, and it shifts the brand hue from cyan to purple.

For what it is worth, the site's previous hand-picked accessible cyan was
`#006d94` — within 3% of the deepened Latte sky. The same problem had already been
solved the same way once.

## Contrast

Every foreground/background pair that actually occurs is verified in both
flavours: 20 pairs × 2 flavours, 0 failures. Small text is held to 4.5:1
(WCAG 1.4.3 AA), large text and non-text indicators to 3:1 (1.4.3 / 1.4.11).

Tightest margins, worth knowing before changing a value:

| Pair                             | Flavour | Ratio  | Floor |
| -------------------------------- | ------- | ------ | ----- |
| `--accent-text` on `--bg-chrome` | Latte   | 5.04:1 | 4.5   |
| `--text-muted` on `--bg-alt`     | Latte   | 5.14:1 | 4.5   |
| `--accent-text` on `--bg-alt`    | Latte   | 5.48:1 | 4.5   |

Latte is always the tighter flavour — Mocha's worst text pair is 7.37:1. **If you
change a colour, check it against Latte.**

Two categories sit below 3:1 deliberately:

- **Decorative fills and glyphs** — `--text-subtle` and `--accent` on the marquee,
  the `--border` hairlines, the case study dividers. All `aria-hidden` or purely
  ornamental, so they are tuned to be quiet rather than legible.
- **The social pill shape** (`--bg-chrome` on `--bg-alt`, 1.09:1). WCAG 1.4.11
  requires 3:1 only for a boundary *needed to identify* a control; here the icon
  inside does that at 6.04:1 and carries the accessible name. The circle is
  ornament. Its focus ring, which is not ornament, is `--accent-text` at 5.04:1.

The carousel arrows are the opposite case: their border is `--text`, not
`--border`, because there the 1px outline genuinely is the control's boundary and
`--border` would be 1.91:1 in Latte.

## The bands are quieter than they used to be, in both flavours

Worth knowing, because it is the most visible consequence of adopting Catppuccin.
The previous design was a high-contrast, mixed-brightness page: near-black navy nav
and tech stack bands (`#081b29`) framing near-white hero and case study sections
(`#faf5ff`) — a ratio of about 15:1 between them.

Catppuccin does not work that way. Its `crust` / `mantle` / `base` triad is
designed for *stacked* surfaces that read as gently layered, not as separate
zones, and the three sit close together in **both** flavours:

| Band pair          | Latte  | Mocha  |
| ------------------ | ------ | ------ |
| `crust` vs `base`  | 1.17:1 | 1.14:1 |
| `mantle` vs `base` | 1.09:1 | 1.07:1 |

So this is not a light-mode-only problem, and dark mode does not rescue it —
Mocha's `crust` band is, if anything, marginally flatter against its page than
Latte's. Tone alone will not separate the nav from the content in either flavour.

That is why `nav`, `.techstack` and the mobile slide-out menu each carry a 1px
`--border` hairline on the edge facing the content. The hairline, not the fill, is
what delineates them; it measures 1.78–2.63:1 against its neighbour, which is
plenty for a line even though it would be far too little for text.

The slide-out menu is the case where this is not optional. It is an overlay panel
on top of the page at `mantle` over `base` — 1.07:1 in Mocha — so without the
`border-left` its links appear to float on the page rather than sit in a menu.

The net effect is a calmer, flatter, more layered composition than the original
high-contrast design. That is inherent to the palette rather than a defect — but
it is a real change in how the page reads, in both light and dark, and it is the
thing most likely to prompt a second opinion.

## Type

Poppins, loaded from Google Fonts via `preconnect` + `<link>` (see
[ARCHITECTURE.md](ARCHITECTURE.md#no-build-step) for why not `@import`). Weights
in use are 400 for body copy and 600 for headings and the active tab label.

The active hero tab is marked by **both** `--accent-text` and a heavier weight.
That redundancy is deliberate: WCAG 1.4.1 forbids signalling state by colour
alone, and it also means the tab state survives in Latte where the accent is less
vivid than the old bright cyan.

## Deliberate omissions

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

## Changing a colour

1. Change the semantic token, not the rule that uses it.
2. Re-verify against **Latte**, the tighter flavour, on every surface the token
   can appear against — for `--accent-text` and `--text` that is all three of
   `base`, `mantle` and `crust`.
3. Small text needs 4.5:1; large text (≥24px, or ≥18.66px bold) and focus rings
   need 3:1.
4. Run a Lighthouse navigation audit **in both schemes** — see
   [AGENTS.md](AGENTS.md#verifying-a-change).
