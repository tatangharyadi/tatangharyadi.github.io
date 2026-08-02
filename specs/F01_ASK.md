# F01: Ask, semantic search that never leaves the tab

**Status:** done

## Overview

Ask is a search box on the home page. A visitor types a question in their own
words, and the site returns the passages of `portfolio.html` that answer it,
ranked by meaning rather than by keyword. "How did you cut cloud costs?" finds
the passage about cost reduction whether or not it uses the word "cut".

The interesting property is where the work happens. The question is turned into a
384-dimensional vector by a quantised `all-MiniLM-L6-v2` running in the visitor's
browser, and scored against vectors that were computed ahead of time and shipped
as a static file. There is no endpoint, no API key, no rate limit and no log,
because there is no server involved at any point. The privacy claim is not a
promise in a policy, it is a consequence of the architecture: the question cannot
be collected because it is never transmitted.

That is also the whole justification for `js/ask.js` existing. Every other
interaction on this site is htmx asking a server for HTML it already holds. This
one multiplies a question against vectors that exist only in the tab, and no
endpoint could answer it without being sent the question, which is precisely the
thing the feature avoids.

**Implementation status:** shipped and live.

---

## Key files

| File | Role |
| --- | --- |
| `index.html` (the `#ask` section) | The markup: gate, progress bar, form, examples, status region, results |
| `js/ask.js` | Model load, corpus load, embedding, cosine search, rendering |
| `corpus.json` | The index. One entry per passage, each with its embedding. Generated. |
| `portfolio.html` | Source of truth for every indexed passage |
| `scripts/build-corpus.html` | The only thing allowed to write `corpus.json` |
| `scripts/check_corpus.py` | Asserts the index still describes the page |
| `vendor/transformers/` | Vendored Transformers.js runtime, web build |
| `assets/models/all-MiniLM-L6-v2/` | Quantised weights, tokenizer, ONNX runtime |

---

## Architecture

```
visitor presses #ask--load
        │
        ├─ loadModel()   vendor/transformers  +  assets/models/…   (progress bar)
        └─ loadCorpus()  corpus.json
                │
visitor submits #ask--form
        │
   embed(query)  ──► 384-dim vector, unit length
        │
   search(vec, TOP_K)  ──► cosine against every passage vector
        │
   filter by MIN_SCORE
        │
   render(hits) ──► #ask--results, each hit linking portfolio.html#anchor
```

### The index

`corpus.json` carries its own provenance at the top level: the model id, the
quantisation, the dimension count, the exact `embedding_input` template used, the
source page and the generator that wrote it. Each passage carries a heading, a
category, an anchor into `portfolio.html`, the text and the vector.

Recording `embedding_input` matters more than it looks. The query at runtime and
the passage at build time must be embedded the same way or the geometry is
subtly wrong in a way nothing raises an error about, and the file states what
that way was.

### Why the generator is a web page

`scripts/build-corpus.html` is opened in a browser and pressed, which is an odd
shape for a build tool. It is the right one here for a specific reason: the
vectors must be the ones the shipped code would have produced. Running the
generator in the same browser, against the same vendored runtime and the same
weights, makes that true by construction rather than by care.

Node cannot do it, because the vendored build is the web build and its ONNX
backend registry is empty outside a browser. A Python generator would mean taking
on `onnxruntime`, which the no-toolchain stance rules out.

### Why it embeds one passage at a time

Batching pads every sequence in a batch to the length of its longest member, and
under 8-bit quantisation that padding changes the output. A two-passage prose edit
once moved the vectors of eleven untouched passages to a cosine of 0.997 against
themselves. Nothing catches that, because `check_corpus.py` compares text, not
geometry.

Left unbatched, re-running the generator with no prose change reproduces the
committed file byte for byte. That reproducibility is worth more than the seconds
batching would save.

### The download, and being honest about it

The model and runtime are large. `ARCHITECTURE.md` carries the measured table:
roughly 37 MB on disk compressing to roughly 20 MB transferred, of which the
quantised weights are the great majority. GitHub Pages serves gzip and not brotli,
which is why the ONNX runtime compresses so much better than the weights do.

The feature therefore does not load on page view. `#ask--gate` states the size
before spending it and waits for a press. The figure in the markup carries an
inline comment with the `curl` command that re-measures it, so the number in the
prose has a way to be checked rather than merely trusted.

### Interaction and accessibility

| Element | Note |
| --- | --- |
| `#ask--gate` | Hidden in markup, revealed by `js/ask.js`. A reader with no JavaScript is never offered a button that cannot work. |
| `#ask--load`, `#ask--submit` | Use `aria-disabled` plus a plain re-entry flag. Never the `disabled` property. |
| `#ask--progress`, `#ask--progress-label` | Progress during a multi-second download |
| `#ask--status` | `role="status"`, `aria-live="polite"`. Results arrive in a region nobody navigated to. |
| `#ask--examples` | Five worked queries, so the box is not a blank prompt |
| `.ask--straight` | A plain link to `portfolio.html`, outside the gate and never hidden |
| `<noscript>` | States the requirement and points at the same page |

The `disabled` rule is not stylistic. An earlier version disabled `#ask--load` on
press, reasoning that it leaves the page once the model is up. It does, but it
leaves when the load *finishes*, so disabling it on press stranded a keyboard user
on `<body>` for the entire download.

---

## Acceptance criteria

| ID | Criterion | Evidence |
| --- | --- | --- |
| F01-AC01 | The question is never transmitted. No fetch in the query path reaches any host. | Structural: `js/ask.js` fetches only the model, the runtime and `corpus.json`, all same-origin, all before the first query |
| F01-AC02 | Every passage in the index is still present, word for word, in `portfolio.html`. | `scripts/check_corpus.py`, CI |
| F01-AC03 | Every passage anchor resolves to a section id that exists on `portfolio.html`. | `scripts/check_corpus.py`, CI |
| F01-AC04 | The index declares the model and dimension count the runtime expects. | `scripts/check_corpus.py`, CI |
| F01-AC05 | Re-running the generator with no prose change reproduces `corpus.json` byte for byte. | Unbatched embedding, by construction. Verified by re-running and diffing. |
| F01-AC06 | Nothing downloads until the visitor presses the gate. | Human, network panel |
| F01-AC07 | Prose added to `portfolio.html` is actually indexed. | **Human only.** Load the page, press load, confirm the passage count went up. No static check can catch this, because an unindexed paragraph looks exactly like one the generator was never meant to see. |
| F01-AC08 | Neither button uses the `disabled` property, and focus survives both a load and a query. | Human, keyboard traversal |
| F01-AC09 | Results announce to a screen reader without stealing focus. | Human, `aria-live` on `#ask--status` |
| F01-AC10 | The page is usable with JavaScript off: the gate stays hidden and the straight link works. | Human, JavaScript disabled |
| F01-AC11 | Contrast holds at 4.5:1 for text in both flavours, checked against Latte. | Human, per colour scheme |

Criteria marked human are not weaker than the CI ones. They are the ones a static
check cannot reach, which is exactly why they are written down.

---

## Deferred

| Item | Note |
| --- | --- |
| A smaller download | Any change to the model changes retrieval quality. Needs measuring against real queries before it is chosen, not assumed. |
| Highlighting the matched span | Currently the whole passage is returned. Span-level scoring would need per-sentence vectors and a larger index. |
| Query logging of any kind | Rejected, not deferred. It would dissolve the property the feature exists to demonstrate. |
