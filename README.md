# tatangharyadi.github.io

Personal site for Tatang Haryadi — CTO, digital transformation and product innovation.

Live at **<https://tatangharyadi.github.io/>**

## Stack

Plain HTML, CSS and vanilla JavaScript with no build step and no dependencies,
served directly by GitHub Pages from `main`. Poppins and Boxicons load from a CDN
at runtime. Keeping it build-free is deliberate: the site stays editable years
from now without reviving a toolchain.

## Local development

No server is strictly required — opening `index.html` in a browser works. To
match production more closely (absolute paths, correct MIME types), serve it:

```sh
python3 -m http.server 8000
# then open http://localhost:8000
```

Pushing to `main` publishes automatically.

## Further reading

- **[ARCHITECTURE.md](ARCHITECTURE.md)** — file layout, design tokens, the
  CSS-only interaction patterns, how the case study carousel works, breakpoints
  and the GitHub Pages deployment model.
- **[DESIGN.md](DESIGN.md)** — the Catppuccin palette, the semantic colour
  tokens, and the contrast floors every colour is verified against.
- **[AGENTS.md](AGENTS.md)** — how to verify a change, and the accessibility
  invariants that are easy to break by accident.

## License

Code is MIT licensed — see [LICENSE](LICENSE). Written content and images are
not covered by it.
