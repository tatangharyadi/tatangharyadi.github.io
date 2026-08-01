#!/usr/bin/env python3
"""Regenerate fragments/casestudy/ from the deck in index.html.

The case studies exist in one file per rotation per direction, plus index.html
itself. That duplication is deliberate — it is what lets the carousel be pure
hypermedia with no client-side state — but propagating an edit across it by hand
is how the copies drift apart. scripts/check_htmx.py fails CI when they do; this
script is the other half of that bargain, the thing that makes them agree again.

index.html is the source of truth. Edit a case study there, run this, commit the
result. Never edit a fragment directly.

    python3 scripts/propagate_casestudy.py          # rewrite the fragments
    python3 scripts/propagate_casestudy.py --check  # exit 1 if any are stale

Adding or removing a case study means changing ROTATIONS in check_htmx.py to
match, and adding the matching --casestudy-itemN-* tokens, :nth-child(N) rule and
@keyframes fromItemN in css/style.css. Nothing here can infer those for you.
"""

import argparse
import pathlib
import re
import sys
import textwrap

ROOT = pathlib.Path(__file__).resolve().parent.parent
INDEX = ROOT / "index.html"
OUT = ROOT / "fragments" / "casestudy"

ITEM_OPEN = '<div class="casestudy--item">'
ARROW_SYNC = "closest .casestudy--container:replace"


def div_block(text, start):
    """The <div> beginning at `start`, matched by depth rather than by regex.

    Same reasoning as the identical helper in check_htmx.py: these divs nest, so
    a greedy `.*</div>` runs past the end of the block.
    """
    depth, end = 0, start
    for match in re.finditer(r"<div\b|</div>", text[start:]):
        end = start + match.end()
        depth += 1 if match.group(0) != "</div>" else -1
        if depth == 0:
            return text[start:end], end
    return None, len(text)


def deck(text):
    """Every .casestudy--item block, in document order, re-indented to 4 spaces."""
    items, index = [], 0
    while True:
        start = text.find(ITEM_OPEN, index)
        if start == -1:
            return items
        block, index = div_block(text, start)
        if block is None:
            sys.exit("a .casestudy--item block in index.html is never closed")
        # The block starts at the opening tag, so its first line arrives with no
        # indentation at all while the rest still carries index.html's deeper
        # nesting. Dedent the body — which strips the item's own level along with
        # everything above it — then re-indent to the 4 the fragments sit at.
        first, _, rest = block.partition("\n")
        body = textwrap.indent(textwrap.dedent(rest), " " * 4) if rest else ""
        items.append("    " + first + ("\n" + body if body else ""))


HEADER = """\
<!-- Rotation r{r}, entered by moving {motion}.

     Served to the #casestudy container as innerHTML. The deck below is the {n}
     case studies of index.html rotated by {r}, and the .{direction} class on the list
     is what triggers the entry animation in css/style.css. The arrows carry the
     URLs for the two rotations reachable from here, so no client-side state
     exists.

     GENERATED FILE — do not edit. The case studies are duplicated across these
     {files} fragments the same way the palette is duplicated across four files:
     index.html is the source of truth, scripts/propagate_casestudy.py rewrites
     these from it, and scripts/check_htmx.py fails CI if they drift. To change a
     case study, edit index.html and re-run the propagate script. -->"""

ARROWS = """\
<div class="casestudy--arrows" role="group" aria-label="Case study navigation">
    <button type="button" id="prev" aria-label="Previous case study"
        hx-get="fragments/casestudy/r{prev}-prev.html"
        hx-trigger="click, keydown[key=='ArrowLeft'] from:#casestudy"
        hx-sync="{sync}">&lt;</button>
    <button type="button" id="next" aria-label="Next case study"
        hx-get="fragments/casestudy/r{next}-next.html"
        hx-trigger="click, keydown[key=='ArrowRight'] from:#casestudy"
        hx-sync="{sync}">&gt;</button>
</div>"""


def render(items, rotation, direction):
    n = len(items)
    rotated = items[rotation:] + items[:rotation]
    header = HEADER.format(
        r=rotation,
        n=n,
        files=2 * n,
        direction=direction,
        motion="forward" if direction == "next" else "back",
    )
    arrows = ARROWS.format(
        prev=(rotation - 1) % n,
        next=(rotation + 1) % n,
        sync=ARROW_SYNC,
    )
    body = "\n\n".join(rotated)
    return (
        f'{header}\n<div class="casestudy--list {direction}">\n'
        f"{body}\n</div>\n{arrows}\n"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="do not write; exit 1 if any fragment differs from what would be written",
    )
    args = parser.parse_args()

    items = deck(INDEX.read_text())
    if not items:
        sys.exit("found no .casestudy--item blocks in index.html")

    stale, written = [], []
    for rotation in range(len(items)):
        for direction in ("next", "prev"):
            path = OUT / f"r{rotation}-{direction}.html"
            want = render(items, rotation, direction)
            if args.check:
                if not path.is_file() or path.read_text() != want:
                    stale.append(path.relative_to(ROOT).as_posix())
            else:
                if not path.is_file() or path.read_text() != want:
                    path.write_text(want)
                    written.append(path.relative_to(ROOT).as_posix())

    if args.check:
        if stale:
            print("Stale case study fragments:", file=sys.stderr)
            for s in stale:
                print(f"  {s}", file=sys.stderr)
            print(
                "\nindex.html has changed since these were generated. Run\n"
                "  python3 scripts/propagate_casestudy.py\n"
                "and commit the result.",
                file=sys.stderr,
            )
            return 1
        print(f"{2 * len(items)} case study fragments are in step with index.html.")
        return 0

    if written:
        for w in written:
            print(f"wrote {w}")
    print(
        f"{2 * len(items)} fragments for {len(items)} case studies "
        f"({len(written)} changed)."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
