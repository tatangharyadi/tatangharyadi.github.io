#!/usr/bin/env python3
"""Check the htmx wiring: fragments resolve, agree with index.html, and keep the
invariants that make the interaction accessible.

The site's interaction is hypermedia. Every state of the case study carousel is a
static file under fragments/, and the markup a fragment returns is the whole
state — which buttons exist, which one is selected, where each button points
next. That is what makes the pattern work without a server, and it is also what
makes it fragile in ways a browser will not complain about:

  * A renamed or deleted fragment is a 404 on click. The page keeps working, the
    control silently does nothing.
  * The case studies are written out once per rotation per direction, plus
    index.html. Editing one copy and not the rest is the same class of mistake
    the palette's four copies invite, and check_palette.py exists for exactly
    that reason.
  * htmx restores focus after a swap by looking the previously focused element up
    again with document.getElementById. A control that loses its id during a swap
    drops keyboard focus onto <body>, which is the regression the carousel's
    whole shape is designed to avoid.

None of that is visible to node --check, a linter, or a green page load. It is
visible here.

Run from the repository root:  python3 scripts/check_htmx.py
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
FRAGMENTS = ROOT / "fragments"

HTMX_VERSION = "2.0.10"
HTMX_SRI = "sha384-H5SrcfygHmAuTDZphMHqBJLc3FhssKjG7w/CeCpFReSfwBWDTKpkzPP8c+cLsK+V"

ITEM_OPEN = '<div class="casestudy--item">'

# How many case studies there are is not written down anywhere as a number. It is
# however many .casestudy--item blocks index.html holds, and everything else — the
# count of fragments, the modulus the arrows wrap by, the slot rules in the
# stylesheet — is checked against that. A constant here would be a fifth place to
# remember to update, which is the class of bug this file exists to catch.

# The re-entry guard. `:replace` is what makes a second press abandon the
# in-flight request rather than queue behind it; `closest` resolves to
# #casestudy both in index.html and in a swapped-in fragment, since the
# fragment's arrows land inside that same container.
ARROW_SYNC = "closest .casestudy--container:replace"

problems = []


def fail(message):
    problems.append(message)


def read(path):
    return path.read_text(encoding="utf-8")


def html_files():
    """Every page and every fragment, as (relative path, text)."""
    paths = sorted(ROOT.glob("*.html")) + sorted(FRAGMENTS.rglob("*.html"))
    return [(p.relative_to(ROOT).as_posix(), read(p)) for p in paths]


def squash(text):
    """Collapse whitespace so indentation differences are not treated as drift."""
    return re.sub(r"\s+", " ", text).strip()


def tags_with(attr, text):
    """Yield the full source of every start tag carrying the given attribute."""
    for match in re.finditer(r"<(\w+)\b[^>]*>", text, re.DOTALL):
        if re.search(rf"\b{attr}\s*=", match.group(0)):
            yield match.group(0)


def attr(tag, name):
    match = re.search(rf'\b{name}\s*=\s*"([^"]*)"', tag)
    return match.group(1) if match else None


def div_block(text, start):
    """The <div> beginning at `start`, matched by depth rather than by regex.

    A greedy pattern cannot do this: these divs nest, so `.*</div>` runs off the
    end of the block and swallows whatever follows it in the document.
    """
    depth, end = 0, start
    for match in re.finditer(r"<div\b|</div>", text[start:]):
        end = start + match.end()
        depth += 1 if match.group(0) != "</div>" else -1
        if depth == 0:
            return text[start:end], end
    return None, len(text)


def casestudy_items(text):
    """The .casestudy--item blocks, in document order, whitespace-normalised."""
    items, index = [], 0
    while True:
        start = text.find(ITEM_OPEN, index)
        if start == -1:
            return items
        block, index = div_block(text, start)
        if block is None:
            fail("a .casestudy--item block is never closed")
            return items
        items.append(squash(block))


def check_fragment_targets(files):
    """Every hx-get resolves to a file that exists, and no fragment is a page."""
    for relpath, text in files:
        # A relative hx-get inside a fragment resolves against the URL of the page
        # the fragment was swapped into, not against the fragment's own location.
        # index.html is served at /, so both pages and fragments resolve from the
        # repository root.
        for tag in tags_with("hx-get", text):
            url = attr(tag, "hx-get")
            target = ROOT / url.lstrip("/")
            if not target.is_file():
                fail(f"{relpath} has hx-get=\"{url}\", which matches no file in the repo")

        # 404.html is served at whatever URL was missed, so a relative fragment
        # path there would resolve against a directory that does not exist.
        if relpath == "404.html":
            for tag in tags_with("hx-get", text):
                url = attr(tag, "hx-get")
                if not url.startswith("/"):
                    fail(
                        f"404.html has hx-get=\"{url}\": paths here must be "
                        "root-absolute, because this page renders at the missing URL"
                    )

        for match in re.finditer(r'href\s*=\s*"([^"]*fragments/[^"]*)"', text):
            fail(
                f"{relpath} links to {match.group(1)} with href. Fragments are not "
                "pages — they have no <head>, title or styles."
            )


def check_ids_on_controls(files):
    """htmx re-focuses by id after a swap, so every control it drives needs one."""
    for relpath, text in files:
        for tag in tags_with("hx-get", text):
            if not attr(tag, "id"):
                fail(
                    f"{relpath} has an element with hx-get and no id: "
                    f"{squash(tag)[:80]}. Focus would land on <body> after the swap."
                )


def check_htmx_script(files):
    """The CDN dependency stays pinned by version and digest, on every page."""
    pages = [(rel, text) for rel, text in files if "/" not in rel]
    for relpath, text in pages:
        tags = [t for t in re.findall(r"<script\b[^>]*>", text) if "htmx.org" in t]
        if not tags:
            fail(f"{relpath} does not load htmx")
            continue
        for tag in tags:
            src = attr(tag, "src") or ""
            if f"htmx.org@{HTMX_VERSION}/" not in src:
                fail(f"{relpath} loads htmx from {src}, not a pinned @{HTMX_VERSION}")
            if attr(tag, "integrity") != HTMX_SRI:
                fail(
                    f"{relpath} loads htmx without the expected integrity digest. "
                    "An unpinned CDN can change what executes on the page."
                )
            if attr(tag, "crossorigin") is None:
                fail(f"{relpath} loads htmx without crossorigin, so integrity is ignored")


def check_casestudy_fragments(index_text):
    """One rotation per case study, each the index.html deck rotated, each
    pointing onward. Two files per rotation, one per direction of travel."""
    deck = casestudy_items(index_text)
    if not deck:
        fail("index.html holds no .casestudy--item blocks")
        return
    rotations = len(deck)

    stale = sorted(
        p.name for p in (FRAGMENTS / "casestudy").glob("r*.html")
        if not re.fullmatch(r"r(\d+)-(next|prev)\.html", p.name)
        or int(re.match(r"r(\d+)", p.name).group(1)) >= rotations
    )
    if stale:
        # A case study removed from index.html leaves its fragments behind, and
        # nothing else would notice: they resolve, so no link 404s, but they serve
        # a deck that no longer exists and arrows that wrap past the end.
        fail(
            f"index.html has {rotations} case studies, so rotations r0..r{rotations - 1} "
            f"are the only valid ones; these are left over: {', '.join(stale)}"
        )

    for rotation in range(rotations):
        expected_items = deck[rotation:] + deck[:rotation]
        for direction in ("next", "prev"):
            name = f"r{rotation}-{direction}.html"
            path = FRAGMENTS / "casestudy" / name
            if not path.is_file():
                fail(f"fragments/casestudy/{name} is missing")
                continue
            relpath = path.relative_to(ROOT).as_posix()
            text = read(path)

            if casestudy_items(text) != expected_items:
                fail(
                    f"{relpath} does not hold the index.html case studies rotated by "
                    f"{rotation}. index.html is the source of truth; edit it there and "
                    "regenerate, do not edit one copy."
                )

            list_tag = next((t for t in tags_with("class", text)
                             if "casestudy--list" in (attr(t, "class") or "")), None)
            if list_tag is None:
                fail(f"{relpath} has no .casestudy--list")
            elif direction not in (attr(list_tag, "class") or "").split():
                fail(
                    f"{relpath} does not carry .{direction} on its list, so the slide "
                    "animation for this direction never runs"
                )

            check_arrow_targets(relpath, text, rotation, rotations)

    check_casestudy_slots(rotations)


def check_casestudy_slots(rotations):
    """Every case study needs a slot in the stylesheet to be shown in.

    The deck is positioned by :nth-child, so adding a case study to index.html
    without adding its slot leaves the new item stacked at the default position —
    visible, wrong, and reported by no other check. The @keyframes is what the
    .next / .prev rules animate from; a missing one silently disables the slide.
    """
    css = read(ROOT / "css" / "style.css")
    for slot in range(1, rotations + 1):
        for token in ("transform", "zindex"):
            name = f"--casestudy-item{slot}-{token}"
            if name not in css:
                fail(
                    f"css/style.css has no {name}, but index.html has {rotations} "
                    f"case studies so slot {slot} needs one"
                )
        if f"@keyframes fromItem{slot}" not in css:
            fail(
                f"css/style.css has no @keyframes fromItem{slot}, so the case study "
                f"entering slot {slot} would jump rather than slide"
            )


def check_arrow_targets(relpath, text, rotation, rotations):
    """From rotation r, prev goes to r-1 and next to r+1, both wrapping."""
    expected = {
        "prev": f"fragments/casestudy/r{(rotation - 1) % rotations}-prev.html",
        "next": f"fragments/casestudy/r{(rotation + 1) % rotations}-next.html",
    }
    for which, want in expected.items():
        tag = next((t for t in re.findall(r"<button\b[^>]*>", text, re.DOTALL)
                    if attr(t, "id") == which), None)
        if tag is None:
            fail(f"{relpath} has no #{which} button")
            continue
        got = attr(tag, "hx-get")
        if got != want:
            fail(f"{relpath}'s #{which} points at {got}, expected {want}")
        # Disabling the control the user just pressed moves focus to <body>.
        if re.search(r"\bdisabled\b(?!\s*=\s*\"false\")", tag) and "aria-disabled" not in tag:
            fail(
                f"{relpath}'s #{which} uses the disabled property. Use hx-sync to "
                "guard re-entry — disabling it strands keyboard focus on <body>."
            )
        # hx-sync is the re-entry guard that replaced the old isSliding flag.
        # Losing it is invisible on a single click and only shows up as two
        # slides racing, so nothing but this check would catch it.
        if attr(tag, "hx-sync") != ARROW_SYNC:
            fail(
                f"{relpath}'s #{which} does not carry hx-sync=\"{ARROW_SYNC}\", "
                "so a second press queues behind the first instead of replacing it"
            )
        # Keyboard operation is declared here, not in a script. A fragment that
        # drops the keydown binding still works with a mouse, so a green page
        # load hides it — the arrows simply stop responding to arrow keys after
        # the first swap.
        key = "ArrowLeft" if which == "prev" else "ArrowRight"
        want_trigger = f"click, keydown[key=='{key}'] from:#casestudy"
        if attr(tag, "hx-trigger") != want_trigger:
            fail(
                f"{relpath}'s #{which} does not carry "
                f"hx-trigger=\"{want_trigger}\", so {key} stops working after a swap"
            )


def check_robots():
    text = read(ROOT / "robots.txt")
    if "Disallow: /fragments/" not in text:
        fail("robots.txt does not disallow /fragments/, so fragments can be indexed as pages")


def report():
    if problems:
        print("htmx wiring check failed:\n")
        for problem in problems:
            print(f"  - {problem}")
        return 1
    print("htmx fragments resolve, agree with index.html and keep their focus invariants.")
    return 0


def main():
    if not FRAGMENTS.is_dir():
        print("htmx wiring check failed:\n\n  - fragments/ does not exist")
        return 1

    files = html_files()
    index_text = read(ROOT / "index.html")

    check_fragment_targets(files)
    check_ids_on_controls(files)
    check_htmx_script(files)
    check_casestudy_fragments(index_text)
    check_robots()

    return report()


if __name__ == "__main__":
    sys.exit(main())
