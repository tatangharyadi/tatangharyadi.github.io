#!/usr/bin/env python3
"""Regenerate fragments/work/ from the project articles in portfolio.html.

The work index on index.html shows a category and a title per project and
fetches the detail on demand. That detail is prose that already exists, once, in
portfolio.html — which is also the source corpus.json is generated from. So this
script exists to make sure the detail served to the index and the detail a reader
sees at its own URL cannot say different things: both come from the same article.

portfolio.html is the source of truth. Edit a project there, run this, commit the
result. Never edit a fragment directly.

    python3 scripts/propagate_work.py          # rewrite the fragments
    python3 scripts/propagate_work.py --check  # exit 1 if any are stale

Which projects appear in the index is not a constant here either: it is whichever
slugs index.html asks for by hx-get, in the order it asks for them. Adding a
project means writing the article in portfolio.html, adding the entry to
index.html, and running this. scripts/check_htmx.py fails CI if the two ever
disagree about the set.
"""

import argparse
import pathlib
import re
import sys
import textwrap

ROOT = pathlib.Path(__file__).resolve().parent.parent
INDEX = ROOT / "index.html"
PORTFOLIO = ROOT / "portfolio.html"
OUT = ROOT / "fragments" / "work"

HX_GET = re.compile(r'hx-get="fragments/work/([A-Za-z0-9._-]+)\.html"')


def slugs(text):
    """The project slugs index.html asks for, in document order, deduplicated.

    Reading the list off the markup rather than hard-coding it keeps index.html
    the single declaration of what the index contains. A slug that has no article
    in portfolio.html is an error here rather than a 404 at runtime.
    """
    seen, out = set(), []
    for match in HX_GET.finditer(text):
        slug = match.group(1)
        if slug not in seen:
            seen.add(slug)
            out.append(slug)
    return out


def article(text, slug):
    """The <article> block for `slug`, matched by depth rather than by regex.

    Same reasoning as the div helper in check_htmx.py: a greedy `.*</article>`
    would run past the end of the block the moment these ever nest. Depth
    counting is correct regardless.
    """
    open_tag = f'<article class="project" id="{slug}">'
    start = text.find(open_tag)
    if start == -1:
        return None
    depth = 0
    for match in re.finditer(r"<article\b|</article>", text[start:]):
        end = start + match.end()
        depth += 1 if match.group(0) != "</article>" else -1
        if depth == 0:
            return text[start:end]
    return None


HEADING = re.compile(r"[ \t]*<h2>(.*?)</h2>\n", re.DOTALL)


def body(block):
    """The article's contents, minus its wrapper and its <h2>, re-indented to 4.

    The heading goes because the index already renders the project title as the
    control you activated to get here. Repeating it would put the same string
    twice in a row on screen and add a second heading at the same level in the
    outline.
    """
    inner = block.partition(">")[2].rpartition("</article>")[0]
    match = HEADING.search(inner)
    title = match.group(1).strip() if match else None
    if match:
        inner = inner[: match.start()] + inner[match.end() :]
    return title, textwrap.indent(textwrap.dedent(inner).strip("\n"), " " * 4)


HEADER = """\
<!-- Detail for "{title}", served to the work index on index.html.

     GENERATED FILE — do not edit. The prose here exists once, in
     portfolio.html#{slug}, because that page is also what corpus.json is
     generated from and the search must not be able to disagree with the index.
     scripts/propagate_work.py rewrites this from it and
     scripts/check_htmx.py fails CI if it drifts. To change this project, edit
     portfolio.html and re-run the propagate script.

     The link at the end is focusable and carries no id, which is safe only
     because it arrives in a panel that was empty: htmx re-focuses the
     previously focused element by id after a swap, and nothing inside an empty
     panel can have been that element. The control that triggered this swap sits
     outside the panel and never moves. See AGENTS.md. -->"""


def render(title, slug, inner):
    header = HEADER.format(title=title or slug, slug=slug)
    more = (
        '    <p class="work--detail-more">'
        f'<a href="portfolio.html#{slug}">Read this in context</a>'
        "</p>"
    )
    return f'{header}\n<div class="work--detail">\n{inner}\n\n{more}\n</div>\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="do not write; exit 1 if any fragment differs from what would be written",
    )
    args = parser.parse_args()

    wanted = slugs(INDEX.read_text())
    if not wanted:
        sys.exit("index.html asks for no fragments under fragments/work/")

    portfolio = PORTFOLIO.read_text()
    OUT.mkdir(parents=True, exist_ok=True)

    stale, written = [], []
    for slug in wanted:
        block = article(portfolio, slug)
        if block is None:
            sys.exit(
                f'index.html asks for fragments/work/{slug}.html but portfolio.html '
                f'has no <article class="project" id="{slug}">'
            )
        title, inner = body(block)
        path = OUT / f"{slug}.html"
        want = render(title, slug, inner)
        if args.check:
            if not path.is_file() or path.read_text() != want:
                stale.append(path.relative_to(ROOT).as_posix())
        elif not path.is_file() or path.read_text() != want:
            path.write_text(want)
            written.append(path.relative_to(ROOT).as_posix())

    # A fragment for a project the index no longer lists would keep passing every
    # other check while being served to nobody, so it is a failure rather than
    # litter to ignore.
    orphans = sorted(
        p.relative_to(ROOT).as_posix()
        for p in OUT.glob("*.html")
        if p.stem not in wanted
    )

    if args.check:
        if stale or orphans:
            if stale:
                print("Stale work fragments:", file=sys.stderr)
                for s in stale:
                    print(f"  {s}", file=sys.stderr)
            if orphans:
                print("Work fragments no index entry asks for:", file=sys.stderr)
                for o in orphans:
                    print(f"  {o}", file=sys.stderr)
            print(
                "\nRun\n  python3 scripts/propagate_work.py\nand commit the result.",
                file=sys.stderr,
            )
            return 1
        print(f"{len(wanted)} work fragments are in step with portfolio.html.")
        return 0

    for o in orphans:
        (ROOT / o).unlink()
        print(f"removed {o}")
    for w in written:
        print(f"wrote {w}")
    print(f"{len(wanted)} work fragments ({len(written)} changed).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
