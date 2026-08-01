#!/usr/bin/env python3
"""Check the repo invariants that a build step would normally catch.

  1. `.nojekyll` exists. Without it, Pages runs a legacy Jekyll build that
     silently drops any path starting with `_` or `.` -- with no build error.
     See ARCHITECTURE.md#why-nojekyll-matters.
  2. `sitemap.xml` lists every indexable page. AGENTS.md asks for this to be kept
     in step by hand when a page is added or removed; nothing checked it.
  3. Every copy of the nav offers the same destinations. There are four copies,
     one per root page plus fragments/404-links.html, and editing three of them
     leaves the fourth pointing somewhere the rest of the site has dropped.

Pages carrying `<meta name="robots" content="noindex">` are exempt from (2) --
404.html is noindex and correctly absent from the sitemap. Listing it would ask
crawlers to index the not-found page.

Note that a page being absent from the nav is not the same as being absent from
the sitemap. portfolio.html is deliberately out of the nav and deliberately in
the sitemap: it is a destination visitors arrive at rather than choose. See
AGENTS.md.

Stdlib only, by design. See ARCHITECTURE.md#continuous-integration.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SITE = "https://tatangharyadi.github.io"

failures = []


def fail(message):
    failures.append(message)


def check_nojekyll():
    if not (ROOT / ".nojekyll").is_file():
        fail(
            ".nojekyll is missing. Pages will run a Jekyll build and silently "
            "drop any path starting with _ or . -- see "
            "ARCHITECTURE.md#why-nojekyll-matters"
        )


def check_sitemap():
    sitemap_path = ROOT / "sitemap.xml"
    if not sitemap_path.is_file():
        fail("sitemap.xml is missing")
        return
    sitemap = sitemap_path.read_text(encoding="utf-8")
    listed = {loc.strip() for loc in re.findall(r"<loc>(.*?)</loc>", sitemap, re.S)}

    for page in sorted(ROOT.glob("*.html")):
        text = page.read_text(encoding="utf-8")
        noindex = re.search(
            r"<meta[^>]*name=[\"']robots[\"'][^>]*content=[\"'][^\"']*noindex",
            text,
            re.I,
        )
        # The site root is served for index.html, so that is the URL to expect.
        if page.name == "index.html":
            expected = f"{SITE}/"
        else:
            expected = f"{SITE}/{page.name}"

        if noindex:
            if expected in listed:
                fail(
                    f"sitemap.xml lists {expected}, but {page.name} is noindex "
                    "-- remove it rather than asking crawlers to index it"
                )
        elif expected not in listed:
            fail(
                f"sitemap.xml does not list {expected} for {page.name}. Add it, "
                "or mark the page noindex if it should not be crawled."
            )

    known = {f"{SITE}/"} | {
        f"{SITE}/{p.name}" for p in ROOT.glob("*.html")
    }
    for url in sorted(listed - known):
        fail(f"sitemap.xml lists {url}, which has no matching page in the repo")


def nav_labels(text, path):
    """The visible link labels of a nav list, in order.

    Labels rather than hrefs, because the hrefs legitimately differ per page and
    the labels legitimately cannot: index.html links Home to `#section-home`,
    ask.html links it to `index.html`, and the 404 fragment uses root-absolute
    paths because Pages serves it from whatever URL was missed. Comparing hrefs
    would flag all three as drift. What every copy does have to agree on is which
    destinations the nav offers at all.
    """
    match = re.search(r"<ul[^>]*>(.*?)</ul>", text, re.S)
    if not match:
        fail(f"{path} has no nav list, so the nav can no longer be checked")
        return None
    return [
        re.sub(r"\s+", " ", label).strip()
        for label in re.findall(r"<a\b[^>]*>(.*?)</a>", match.group(1), re.S)
    ]


def check_nav():
    """Every copy of the nav offers the same destinations.

    The nav is written out four times: once in each root page, because those links
    are content a crawler should see and the site has to navigate with no
    JavaScript, and once in fragments/404-links.html. Nothing checked that the four
    agreed. Removing an entry means editing four files, and missing one leaves a
    page offering a destination the rest of the site has dropped -- which looks
    perfectly fine on the page you happen to be reading.
    """
    sources = {}

    for page in sorted(ROOT.glob("*.html")):
        text = page.read_text(encoding="utf-8")
        block = re.search(
            r'<ul class="nav-links--container">.*?</ul>', text, re.S
        )
        if block:
            sources[page.name] = nav_labels(block.group(0), page.name)

    fragment = ROOT / "fragments" / "404-links.html"
    if fragment.is_file():
        sources["fragments/404-links.html"] = nav_labels(
            fragment.read_text(encoding="utf-8"), "fragments/404-links.html"
        )

    if not sources:
        fail("no nav list was found anywhere, which cannot be right")
        return

    # index.html is the reference: it is the one page whose nav a crawler reads
    # first and the only one that is also the site root.
    reference = sources.get("index.html")
    if reference is None:
        fail("index.html has no nav list to check the other copies against")
        return

    for name, labels in sorted(sources.items()):
        if labels is not None and labels != reference:
            fail(
                f"{name} nav offers {labels}, but index.html offers {reference}. "
                "The nav is written out in every root page and in "
                "fragments/404-links.html; change all of them or none."
            )


def main():
    check_nojekyll()
    check_sitemap()
    check_nav()
    if failures:
        print("Repo invariants broken:\n")
        for message in failures:
            print(f"  {message}")
        return 1
    print(
        ".nojekyll present; sitemap.xml is in step with the indexable pages; "
        "every copy of the nav agrees."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
