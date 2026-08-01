#!/usr/bin/env python3
"""Check that corpus.json is still an index of portfolio.html.

corpus.json holds the search index for the Ask page: one entry per passage, each
with a 384-dimensional embedding computed by scripts/build-corpus.html. The
embeddings cannot be recomputed here — that needs the model, the WebAssembly
runtime and a browser — so this does the next best thing and checks that the text
those embeddings describe is still the text on the page.

What it catches: prose edited or deleted on portfolio.html while the index kept the
old wording, an anchor pointing at a section id that no longer exists, and a corpus
built for a different model or with the wrong number of dimensions. That is the
failure that has no symptom otherwise — a stale vector does not raise, it just
retrieves worse.

What it does not catch: prose *added* to portfolio.html and never indexed. Nothing
static can, because an unindexed paragraph looks exactly like a paragraph the
generator was never meant to see. Step 4 of "Verifying a change" in AGENTS.md is
what covers that: load ask.html and confirm the passage count went up.

Run it after any edit to portfolio.html:

    python3 scripts/check_corpus.py

When it fails, re-run the generator — serve the repository and open
/scripts/build-corpus.html. Do not hand-patch corpus.json; the text and the vector
beside it have to agree, and only the generator can make that true.
"""

import html.parser
import json
import math
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
CORPUS = ROOT / "corpus.json"
SOURCE = ROOT / "portfolio.html"

# The vectors come out of the model L2-normalised. Six decimal places of stored
# precision moves a component by at most 5e-7, so the norm cannot drift further
# than a few parts in a million; anything looser than this means the file was not
# written by the generator.
NORM_TOLERANCE = 1e-4

failures: list[str] = []


def fail(message: str) -> None:
    failures.append(message)


class TextExtractor(html.parser.HTMLParser):
    """Approximate Node.textContent for the whole document.

    Deliberately inserts nothing between elements, because textContent does not
    either: a passage read out of one <p> containing <strong> and <a> children is a
    contiguous run of this output once whitespace is collapsed. <script> and
    <style> contents are skipped for the same reason the DOM leaves them out of a
    rendered read.
    """

    SKIP = {"script", "style"}

    def __init__(self) -> None:
        super().__init__(convert_charrefs=True)
        self.parts: list[str] = []
        self._depth = 0

    def handle_starttag(self, tag, attrs):
        if tag in self.SKIP:
            self._depth += 1

    def handle_endtag(self, tag):
        if tag in self.SKIP and self._depth:
            self._depth -= 1

    def handle_data(self, data):
        if not self._depth:
            self.parts.append(data)


def normalise(text: str) -> str:
    return re.sub(r"\s+", " ", text).strip()


def main() -> int:
    if not CORPUS.is_file():
        print(f"error: {CORPUS.name} is missing. Run scripts/build-corpus.html.")
        return 1
    if not SOURCE.is_file():
        print(f"error: {SOURCE.name} is missing.")
        return 1

    try:
        corpus = json.loads(CORPUS.read_text(encoding="utf-8"))
    except json.JSONDecodeError as err:
        print(f"error: {CORPUS.name} is not valid JSON: {err}")
        return 1

    source_html = SOURCE.read_text(encoding="utf-8")
    parser = TextExtractor()
    parser.feed(source_html)
    page_text = normalise("".join(parser.parts))
    page_ids = set(re.findall(r'\bid="([^"]+)"', source_html))

    # The model name lives in js/ask.js, which refuses to load a corpus built for
    # anything else. Checking it here as well means the mismatch is caught before it
    # is published rather than by a visitor.
    ask_js = (ROOT / "js" / "ask.js").read_text(encoding="utf-8")
    match = re.search(r"const MODEL_ID = '([^']+)'", ask_js)
    if not match:
        fail("js/ask.js no longer declares MODEL_ID, so the corpus cannot be checked against it")
    elif corpus.get("model") != match.group(1):
        fail(
            f"corpus.json was built for {corpus.get('model')!r} but js/ask.js "
            f"expects {match.group(1)!r}"
        )

    if corpus.get("source") != SOURCE.name:
        fail(f"corpus.json names {corpus.get('source')!r} as its source, expected {SOURCE.name!r}")

    dims = corpus.get("dims")
    if not isinstance(dims, int) or dims <= 0:
        fail(f"corpus.json has no usable 'dims': {dims!r}")
        dims = None

    passages = corpus.get("passages")
    if not passages:
        fail("corpus.json holds no passages")
        passages = []

    for index, passage in enumerate(passages):
        where = f"passage {index} ({passage.get('heading', '?')!r})"

        vector = passage.get("vector")
        if dims is not None:
            if not isinstance(vector, list) or len(vector) != dims:
                fail(f"{where} is not {dims}-dimensional")
                vector = None
        if vector:
            norm = math.sqrt(sum(x * x for x in vector))
            if abs(norm - 1.0) > NORM_TOLERANCE:
                fail(f"{where} has norm {norm:.6f}, so it is not a unit vector")

        anchor = passage.get("anchor", "")
        page, _, fragment = anchor.partition("#")
        if page != SOURCE.name:
            fail(f"{where} links to {page!r}, which is not {SOURCE.name}")
        elif fragment and fragment not in page_ids:
            fail(f"{where} links to #{fragment}, which no longer exists in {SOURCE.name}")

        # A list passage joins its items with ". ", and that separator is not on the
        # page — so the joined string is checked item by item and prose is checked
        # whole.
        for needle in passage.get("parts") or [passage.get("text", "")]:
            if not needle:
                fail(f"{where} holds an empty passage")
            elif normalise(needle) not in page_text:
                fail(
                    f"{where} indexes text that is no longer in {SOURCE.name}: "
                    f"{normalise(needle)[:70]!r}"
                )

    if failures:
        print(f"corpus.json has drifted from {SOURCE.name}:\n")
        for message in failures:
            print(f"  - {message}")
        print(
            "\nRe-run the generator: serve the repository and open "
            "/scripts/build-corpus.html, then save the result over corpus.json."
        )
        return 1

    print(f"corpus.json: {len(passages)} passages, all still in {SOURCE.name}.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
