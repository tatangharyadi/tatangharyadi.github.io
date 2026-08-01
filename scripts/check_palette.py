#!/usr/bin/env python3
"""Check that the four hand-kept copies of the palette agree.

The Catppuccin values are duplicated on purpose, for reasons documented in
ARCHITECTURE.md and AGENTS.md:

  1. css/style.css   - the source of truth: a :root block (Latte) plus a
                       prefers-color-scheme: dark block (Mocha).
  2. 404.html        - an inline subset, because that page has to render when
                       css/style.css is the thing that failed to load.
  3. DESIGN.md       - machine-readable tokens in the front matter, under
                       semantic names, with the dark half prefixed `mocha-`.
  4. theme-color     - a <meta> pair per page, one per colour scheme, whose
                       values are --bg-chrome in each flavour.

Nothing enforces that at runtime, so a change to one copy and not the others is
silent. This script is that enforcement. It reads css/style.css as the source of
truth and holds the other three against it.

Stdlib only, by design - the repo has no package manager. See
ARCHITECTURE.md#continuous-integration.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# CSS custom property -> DESIGN.md front-matter token, for the Latte half. The
# Mocha half is the same mapping with a `mocha-` prefix on the right.
SEMANTIC = {
    "--accent-text": "primary",
    "--accent": "accent",
    "--bg": "surface",
    "--bg-alt": "surface-alt",
    "--bg-chrome": "surface-chrome",
    "--text": "on-surface",
    "--text-muted": "on-surface-muted",
    "--text-subtle": "on-surface-subtle",
    "--border": "border",
}

# 404.html carries only these four. It is a deliberate subset, not an omission:
# that page uses no accent decoration, no alternate surfaces and no borders.
SUBSET_404 = ["--bg", "--text", "--text-muted", "--accent-text"]

HEX = re.compile(r"(--[a-z-]+)\s*:\s*(#[0-9a-fA-F]{3,8})\s*;")

failures = []


def fail(message):
    failures.append(message)


def read(relpath):
    path = ROOT / relpath
    if not path.is_file():
        fail(f"{relpath}: file not found")
        return None
    return path.read_text(encoding="utf-8")


def colour_vars(css):
    """Every --token: #hex; declaration in a chunk of CSS, lowercased."""
    return {name: value.lower() for name, value in HEX.findall(css)}


def split_schemes(css, relpath, opener):
    """Split a stylesheet into its light block and its dark-scheme block.

    `opener` is the text that begins the dark block. Everything before it is
    treated as light, everything after as dark. Both files put the dark override
    immediately after :root and nothing colour-bearing follows it, which is the
    layout this relies on -- if that changes, this check needs revisiting rather
    than silently reading the wrong bytes.
    """
    index = css.find(opener)
    if index == -1:
        fail(f"{relpath}: no prefers-color-scheme: dark block found")
        return None, None
    return css[:index], css[index:]


def compare(label, expected, actual, keys):
    """Hold `actual` against `expected` for each key, reporting every mismatch."""
    for key in keys:
        want = expected.get(key)
        got = actual.get(key)
        if want is None:
            fail(f"{label}: {key} is missing from css/style.css")
        elif got is None:
            fail(f"{label}: {key} is missing (expected {want})")
        elif got != want:
            fail(f"{label}: {key} is {got}, but css/style.css says {want}")


def front_matter_colours(text):
    """The `colors:` map from DESIGN.md's YAML front matter.

    Parsed with a regex rather than a YAML library so this stays stdlib-only.
    The block is flat `name: "#hex"` pairs, which is all this needs to handle --
    and if it ever stops being flat, the design.md linter will say so.
    """
    if not text.startswith("---"):
        fail("DESIGN.md: no YAML front matter")
        return {}
    end = text.find("\n---", 3)
    if end == -1:
        fail("DESIGN.md: front matter is not terminated")
        return {}
    front = text[3:end]

    start = re.search(r"^colors:\s*$", front, re.MULTILINE)
    if not start:
        fail("DESIGN.md: no colors: block in the front matter")
        return {}
    colours = {}
    for line in front[start.end():].splitlines():
        if line.strip() == "" or line.lstrip().startswith("#"):
            continue
        if not line.startswith(("  ", "\t")):
            break  # dedented back out to the next top-level key
        match = re.match(r"\s+([a-z0-9-]+):\s*\"?(#[0-9a-fA-F]{3,8})\"?", line)
        if match:
            colours[match.group(1)] = match.group(2).lower()
    return colours


def theme_colours(text, relpath):
    """The two media-query'd theme-color metas, as {scheme: hex}."""
    found = {}
    for tag in re.findall(r"<meta[^>]*name=[\"']theme-color[\"'][^>]*>", text):
        content = re.search(r"content=[\"'](#[0-9a-fA-F]{3,8})[\"']", tag)
        scheme = re.search(r"prefers-color-scheme:\s*(light|dark)", tag)
        if not content:
            fail(f"{relpath}: a theme-color meta has no hex content")
        elif not scheme:
            fail(
                f"{relpath}: theme-color {content.group(1)} has no "
                "prefers-color-scheme media attribute -- an unconditional tag "
                "paints the browser chrome wrongly for half of all visitors"
            )
        else:
            found[scheme.group(1)] = content.group(1).lower()
    for scheme in ("light", "dark"):
        if scheme not in found:
            fail(f"{relpath}: no theme-color meta for prefers-color-scheme: {scheme}")
    return found


def main():
    style = read("css/style.css")
    notfound = read("404.html")
    design = read("DESIGN.md")
    index = read("index.html")
    if style is None or notfound is None or design is None or index is None:
        return report()

    # 1. The source of truth.
    light_css, dark_css = split_schemes(
        style, "css/style.css", "@media (prefers-color-scheme: dark)"
    )
    if light_css is None:
        return report()
    light = colour_vars(light_css)
    dark = colour_vars(dark_css)

    for token in SEMANTIC:
        if token not in light:
            fail(f"css/style.css :root: {token} is missing")
        if token not in dark:
            fail(
                f"css/style.css dark block: {token} is not overridden -- every "
                "colour token needs a Mocha value"
            )

    # 2. 404.html's inline subset.
    light_404, dark_404 = split_schemes(
        notfound, "404.html", "@media (prefers-color-scheme: dark)"
    )
    if light_404 is not None:
        compare("404.html light", light, colour_vars(light_404), SUBSET_404)
        compare("404.html dark", dark, colour_vars(dark_404), SUBSET_404)

    # 3. DESIGN.md's front matter, under its semantic names.
    tokens = front_matter_colours(design)
    for css_name, semantic in SEMANTIC.items():
        for prefix, source, flavour in (("", light, "Latte"), ("mocha-", dark, "Mocha")):
            name = prefix + semantic
            want = source.get(css_name)
            got = tokens.get(name)
            if want is None:
                continue  # already reported above
            if got is None:
                fail(f"DESIGN.md: colors.{name} is missing (expected {want})")
            elif got != want:
                fail(
                    f"DESIGN.md: colors.{name} is {got}, but css/style.css "
                    f"{flavour} {css_name} is {want}"
                )

    # 4. The theme-color meta pairs, which must equal --bg-chrome per flavour.
    for relpath, text in (("index.html", index), ("404.html", notfound)):
        metas = theme_colours(text, relpath)
        for scheme, source in (("light", light), ("dark", dark)):
            want = source.get("--bg-chrome")
            got = metas.get(scheme)
            if want is not None and got is not None and got != want:
                fail(
                    f"{relpath}: theme-color for {scheme} is {got}, but "
                    f"--bg-chrome is {want}"
                )

    return report()


def report():
    if failures:
        print("Palette copies are out of step:\n")
        for message in failures:
            print(f"  {message}")
        print(
            f"\n{len(failures)} mismatch(es). css/style.css is the source of "
            "truth; bring the others into line with it."
        )
        return 1
    print("Palette is in step across css/style.css, 404.html, DESIGN.md and the "
          "theme-color metas.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
