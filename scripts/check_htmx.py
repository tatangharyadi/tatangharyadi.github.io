#!/usr/bin/env python3
"""Check the htmx wiring: fragments resolve, agree with their source, and keep the
invariants that make the interaction accessible.

The site's interaction is hypermedia. Each entry in the work index on index.html
asks for a static file under fragments/work/ and swaps in the detail it returns.
That is what makes the pattern work without a server, and it is also what makes
it fragile in ways a browser will not complain about:

  * A renamed or deleted fragment is a 404 on click. The page keeps working, the
    control silently does nothing.
  * The detail prose exists once, in portfolio.html, which is also what
    corpus.json is generated from. A fragment for a project that page no longer
    describes would still resolve, still render, and still contradict the search.
  * htmx restores focus after a swap by looking the previously focused element up
    again with document.getElementById. A control that loses its id during a swap
    drops keyboard focus onto <body>. The index avoids this structurally, by
    keeping every trigger outside the region it swaps, and that structure is
    checked here rather than left to whoever edits the markup next.
  * Each trigger is an <a> with a working href, so the index degrades to ordinary
    links with no JavaScript. An hx-get bolted onto a <button>, or an <a> whose
    href drifts from the fragment it fetches, breaks that quietly.

None of that is visible to node --check, a linter, or a green page load. It is
visible here.

Run from the repository root:  python3 scripts/check_htmx.py
"""

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
FRAGMENTS = ROOT / "fragments"
WORK = FRAGMENTS / "work"

HTMX_VERSION = "2.0.10"
HTMX_SRI = "sha384-H5SrcfygHmAuTDZphMHqBJLc3FhssKjG7w/CeCpFReSfwBWDTKpkzPP8c+cLsK+V"

# Which projects the index holds is not written down anywhere as a number or a
# list. It is whichever fragments index.html asks for by hx-get, and portfolio.html
# must have an article for each. A constant here would be a third place to
# remember to update, which is the class of bug this file exists to catch.
HX_GET_WORK = re.compile(r'hx-get="fragments/work/([A-Za-z0-9._-]+)\.html"')

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


def tags_with(attr_name, text):
    """Yield the full source of every start tag carrying the given attribute."""
    for match in re.finditer(r"<(\w+)\b[^>]*>", text, re.DOTALL):
        if re.search(rf"\b{attr_name}\s*=", match.group(0)):
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
    """The CDN dependency stays pinned by version and digest, wherever it is used.

    "On every page" was the original rule and it was the right one while every
    page was an htmx page. game.html is not: it fetches nothing over the wire
    after load, and a page that uses no hx-* attribute has nothing for the
    library to do. Loading it anyway to satisfy a checker would be the checker
    dictating the bytes a visitor downloads, which is backwards.

    So the rule is narrowed rather than dropped, and narrowed on evidence the
    page itself supplies: a page with no hx-* attribute is exempt, and a page
    with even one is held to the full pin. The failure mode this still catches
    is the one that matters, which is a page that uses htmx and gets it from an
    unpinned or undigested URL. The failure mode it gives up is a page that
    quietly stops using htmx, which is visible in the diff that removes the
    attributes.
    """
    pages = [(rel, text) for rel, text in files if "/" not in rel]
    for relpath, text in pages:
        tags = [t for t in re.findall(r"<script\b[^>]*>", text) if "htmx.org" in t]
        if not tags:
            if not re.search(r"\shx-[a-z-]+\s*=", text):
                continue
            fail(f"{relpath} uses hx- attributes but does not load htmx")
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


def work_slugs(index_text):
    """The project slugs index.html asks for, in document order, deduplicated."""
    seen, out = set(), []
    for match in HX_GET_WORK.finditer(index_text):
        slug = match.group(1)
        if slug not in seen:
            seen.add(slug)
            out.append(slug)
    return out


def check_work_set(index_text, portfolio_text):
    """The index, the fragments on disk and the portfolio articles are one set.

    Three places have to agree about which projects exist, and each disagreement
    fails differently and quietly: a fragment nobody asks for is dead weight that
    still resolves, and an index entry with no article is prose that vanished from
    the search while the page still links to it.
    """
    slugs = work_slugs(index_text)
    if not slugs:
        fail("index.html asks for no fragments under fragments/work/")
        return []

    for slug in slugs:
        if f'<article class="project" id="{slug}">' not in portfolio_text:
            fail(
                f'index.html asks for fragments/work/{slug}.html but portfolio.html '
                f'has no <article class="project" id="{slug}">. The detail is '
                "generated from that article, and corpus.json is generated from the "
                "same page, so the search cannot see this project either."
            )
        if not (WORK / f"{slug}.html").is_file():
            fail(f"fragments/work/{slug}.html is missing. Run scripts/propagate_work.py.")

    if WORK.is_dir():
        orphans = sorted(p.name for p in WORK.glob("*.html") if p.stem not in slugs)
        if orphans:
            fail(
                "fragments/work/ holds files no index entry asks for: "
                f"{', '.join(orphans)}. They resolve, so nothing 404s, but they are "
                "served to nobody. Run scripts/propagate_work.py."
            )
    return slugs


def check_work_controls(index_text, slugs):
    """Each entry is a link htmx upgrades, aimed at a panel it is not inside of."""
    for slug in slugs:
        tag = next(
            (t for t in tags_with("hx-get", index_text)
             if attr(t, "hx-get") == f"fragments/work/{slug}.html"),
            None,
        )
        if tag is None:
            continue

        # An <a> with a real href is what makes the index work with no JavaScript
        # and stay crawlable. A <button> here would be a dead control for both.
        if not tag.startswith("<a"):
            fail(
                f"index.html's control for {slug} is not an <a>. Without a real href "
                "the entry does nothing when JavaScript is unavailable."
            )
        want_href = f"portfolio.html#{slug}"
        if attr(tag, "href") != want_href:
            fail(
                f"index.html's control for {slug} has href=\"{attr(tag, 'href')}\", "
                f"expected \"{want_href}\". The fallback must land on the same prose "
                "the fragment carries."
            )

        want_target = f"#work--panel-{slug}"
        if attr(tag, "hx-target") != want_target:
            fail(
                f"index.html's control for {slug} does not target {want_target}, "
                "so the detail would replace something else"
            )
        if attr(tag, "hx-swap") != "innerHTML":
            fail(
                f"index.html's control for {slug} does not use hx-swap=\"innerHTML\", "
                "so the panel element itself would be replaced and its id lost"
            )
        # Disabling the control the user just pressed moves focus to <body>. The
        # same invariant the Ask buttons keep, for the same reason.
        if re.search(r"\bdisabled\b(?!\s*=\s*\"false\")", tag):
            fail(
                f"index.html's control for {slug} uses the disabled property, which "
                "strands keyboard focus on <body>. Use aria-disabled if a state "
                "genuinely needs announcing."
            )

        # Matched by attribute rather than as a literal tag: the panel carries
        # aria-live, and a check that pins attribute order would fail the next
        # time one is added while proving nothing about the wiring.
        panel = re.search(
            rf'<div\b[^>]*\bid="work--panel-{re.escape(slug)}"[^>]*>', index_text
        )
        if panel is None or "work--panel" not in attr(panel.group(0), "class").split():
            fail(f"index.html has no <div class=\"work--panel\" id=\"work--panel-{slug}\">")
            continue
        start = panel.start()
        block, _ = div_block(index_text, start)
        if block is None:
            fail(f"index.html's work--panel for {slug} is never closed")
            continue

        # The structural reason this pattern cannot strand focus: the trigger is
        # not inside the region it replaces, so the swap never removes it. If a
        # trigger is ever moved inside its own panel, it needs a stable id in
        # every fragment that replaces it, and this stops being safe by
        # construction.
        control_id = attr(tag, "id")
        if control_id and f'id="{control_id}"' in block:
            fail(
                f"index.html's control for {slug} sits inside the panel it swaps. "
                "htmx re-focuses by id after a swap; a control that replaces itself "
                "must keep a stable id in every incoming fragment or focus lands on "
                "<body>. Move it outside the target."
            )

        # A panel that ships with content would show it twice after the first
        # swap, and worse, that content would be a hand-written copy of prose the
        # generator owns.
        inner = block.partition(">")[2].rpartition("</div>")[0]
        if inner.strip():
            fail(
                f"index.html's work--panel for {slug} is not empty. It is filled by "
                "the fragment; anything written here is an unchecked second copy."
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
    print(
        "htmx fragments resolve, agree with portfolio.html and keep their focus "
        "invariants."
    )
    return 0


def main():
    if not FRAGMENTS.is_dir():
        print("htmx wiring check failed:\n\n  - fragments/ does not exist")
        return 1

    files = html_files()
    index_text = read(ROOT / "index.html")
    portfolio_text = read(ROOT / "portfolio.html")

    check_fragment_targets(files)
    check_ids_on_controls(files)
    check_htmx_script(files)
    slugs = check_work_set(index_text, portfolio_text)
    check_work_controls(index_text, slugs)
    check_robots()

    return report()


if __name__ == "__main__":
    sys.exit(main())
