# tatangharyadi.github.io

Personal site for Tatang Haryadi — CTO, digital transformation and product innovation.

Live at **<https://tatangharyadi.github.io/>**

## Stack

Plain HTML and CSS with no build step and no package manager, served directly by
GitHub Pages from `main`. Interaction is [htmx](https://htmx.org): each entry in
the work index fetches a static HTML fragment and swaps it in, so there are only
two JavaScript files of our own. The first is the search on the home page, which
runs a sentence-transformer over WebAssembly against an index committed to this
repository, which is why it can answer a question without sending it anywhere.

The second is `js/game.js`, which draws a hex-grid trading simulation written in
Rust and compiled to WebAssembly. It is the one thing here that needs a toolchain,
and it is quarantined so that nobody else does: the binary is committed, and a
visitor, a prose contributor and eight of the nine CI checks all run with no Rust
installed. See [ARCHITECTURE.md](ARCHITECTURE.md#the-simulation) for why that
exception was worth making and what it costs.

One runtime dependency, htmx, pinned by version and Subresource Integrity digest.
No webfont and no icon font: the icons are inline SVG and the type is the system
stack. Keeping the rest build-free is deliberate: the site stays editable years
from now without reviving a toolchain.

## Local development

Serve the site rather than opening `index.html` from the filesystem — htmx fetches
its fragments over HTTP, and a `file://` page cannot:

```sh
python3 -m http.server 8000
# then open http://localhost:8000
```

Pushing to `main` publishes automatically.

## Further reading

- **[ARCHITECTURE.md](ARCHITECTURE.md)** — file layout, design tokens, the
  hypermedia interaction patterns, how the work index, the search and the
  simulation work, breakpoints and the GitHub Pages deployment model.
- **[DESIGN.md](DESIGN.md)** — the Catppuccin palette, the semantic colour
  tokens, and the contrast floors every colour is verified against. Written to the
  [DESIGN.md format](https://github.com/google-labs-code/design.md), so the tokens
  are machine-readable as well as documented.
- **[AGENTS.md](AGENTS.md)** — how to verify a change, and the accessibility
  invariants that are easy to break by accident.

## License

Code is MIT licensed — see [LICENSE](LICENSE). Written content and images are
not covered by it.
