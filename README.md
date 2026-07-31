# tatangharyadi.github.io

Personal site for Tatang Haryadi — CTO, digital transformation and product innovation.

Live at **<https://tatangharyadi.github.io/>**

## Stack

No build step, no dependencies. Plain HTML, CSS and vanilla JavaScript, served
directly by GitHub Pages from `main`. Keeping it build-free is deliberate: the
site stays editable years from now without reviving a toolchain.

External resources are loaded from a CDN at runtime:

- [Poppins](https://fonts.google.com/specimen/Poppins) via Google Fonts
- [Boxicons](https://boxicons.com/) 2.1.4 via unpkg

## Layout

```
index.html              single page: nav + home, case study, tech stack sections
404.html                self-contained not-found page
css/style.css           all styles, including the carousel slide animations
js/script.js            carousel controller (the only JavaScript)
assets/favicon.svg      favicon
assets/images/          profile photo (webp) and the 1200x630 social share card
.nojekyll               opt out of Jekyll processing (see below)
robots.txt, sitemap.xml crawler hints
```

## Local development

No server is strictly required — opening `index.html` in a browser works. To
match production more closely (absolute paths, correct MIME types), serve it:

```sh
python3 -m http.server 8000
# then open http://localhost:8000
```

## Deployment

Pushing to `main` publishes automatically. GitHub Pages is configured to build
from the `main` branch, root directory.

### Why `.nojekyll` matters

Pages runs a legacy Jekyll build for this repository, and Jekyll silently
excludes any file or directory whose name starts with `_` or `.`. The
`.nojekyll` file disables that processing. Do not delete it — without it,
anything published under a directory like `_framework/` or `_next/` (which is
what most game engines and bundlers emit) would 404 with no build error to
explain why.

## Accessibility notes

Two patterns here are easy to break by accident:

- The hero tab radios and the mobile menu checkbox are **visually hidden, not
  `display: none`**. `display: none` removes an element from the tab order and
  the accessibility tree, which made the "My Services" and "For Recruiters"
  panels unreachable by keyboard and invisible to screen readers.
- Every decorative Boxicons glyph is `aria-hidden="true"`, and the tech stack
  marquee rows are hidden as a whole. Without this, a screen reader announces
  hundreds of meaningless items. The two sections whose content is decorative or
  image-based carry a `visually-hidden` `<h2>` so heading navigation still names
  every region the nav links to.
- The carousel arrows use `aria-disabled` while a slide is in flight, **never
  the `disabled` property**. Disabling the element the user just pressed moves
  focus to `<body>`, stranding keyboard users outside the carousel; re-entry is
  already blocked by the `isSliding` guard in JavaScript.

Animations are wrapped in a `prefers-reduced-motion` guard. `js/script.js`
checks the same media query, so the carousel arrows are not left disabled
waiting on an animation that never runs.

## License

Code is MIT licensed — see [LICENSE](LICENSE). Written content and images are
not covered by it.
