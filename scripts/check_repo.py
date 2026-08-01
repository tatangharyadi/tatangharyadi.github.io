#!/usr/bin/env python3
"""Check the two repo invariants that a build step would normally catch.

  1. `.nojekyll` exists. Without it, Pages runs a legacy Jekyll build that
     silently drops any path starting with `_` or `.` -- with no build error.
     See ARCHITECTURE.md#why-nojekyll-matters.
  2. `sitemap.xml` lists every indexable page. AGENTS.md asks for this to be kept
     in step by hand when a page is added or removed; nothing checked it.

Pages carrying `<meta name="robots" content="noindex">` are exempt from (2) --
404.html is noindex and correctly absent from the sitemap. Listing it would ask
crawlers to index the not-found page.

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


def main():
    check_nojekyll()
    check_sitemap()
    if failures:
        print("Repo invariants broken:\n")
        for message in failures:
            print(f"  {message}")
        return 1
    print(".nojekyll present; sitemap.xml is in step with the indexable pages.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
