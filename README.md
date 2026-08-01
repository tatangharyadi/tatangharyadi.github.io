# tatangharyadi.github.io

Personal site for Tatang Haryadi — CTO, digital transformation and product innovation.

Live at **<https://tatangharyadi.github.io/>**

## Stack

Plain HTML and CSS with no build step and no package manager, served directly by
GitHub Pages from `main`. Interaction is [htmx](https://htmx.org): the hero tabs
and the case study carousel fetch static HTML fragments and swap them in, so there
is no JavaScript of our own to maintain. Poppins, Boxicons and htmx load from a CDN
at runtime, each pinned. Keeping it build-free is deliberate: the site stays
editable years from now without reviving a toolchain.

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
  hypermedia interaction patterns, how the case study carousel works, breakpoints
  and the GitHub Pages deployment model.
- **[DESIGN.md](DESIGN.md)** — the Catppuccin palette, the semantic colour
  tokens, and the contrast floors every colour is verified against. Written to the
  [DESIGN.md format](https://github.com/google-labs-code/design.md), so the tokens
  are machine-readable as well as documented.
- **[AGENTS.md](AGENTS.md)** — how to verify a change, and the accessibility
  invariants that are easy to break by accident.

## License

Code is MIT licensed — see [LICENSE](LICENSE). Written content and images are
not covered by it.
