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
| `vendor/transformers/` | Vendored Transformers.js, ORT `.mjs` and `.wasm` |
| `assets/models/all-MiniLM-L6-v2/` | Quantised weights, config, tokenizer |

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
   embed([query], 1)  ──► 384-dim vector, L2-normalised
        │
   search(vec, TOP_K)  ──► dot product against every passage vector
        │
   filter score >= MIN_SCORE, take TOP_K
        │
   render(hits) ──► #ask--results, each hit linking portfolio.html#anchor
```

### Constants

Every one of these is load-bearing. They appear at the top of `js/ask.js` and,
where they affect the vectors, are mirrored in `scripts/build-corpus.html`.

| Constant | Value | Where | Meaning |
| --- | --- | --- | --- |
| `MODEL_ID` | `all-MiniLM-L6-v2` | both | Checked against `corpus.json`'s `model` at load |
| `dtype` | `q8` | both | 8-bit quantised. Changes the vectors. |
| `DIMS` | `384` | generator | Asserted per passage at build time and at load time |
| `CORPUS_URL` | `corpus.json` | `ask.js` | Site root, not the portfolio parsed at runtime |
| `TOP_K` | `5` | `ask.js` | Maximum hits rendered |
| `MIN_SCORE` | `0.25` | `ask.js` | Below this, render the empty state rather than a bad hit |
| `PRECISION` | `6` | generator | Decimal places each vector component is rounded to |
| pooling | `mean` | both | Passed as `{ pooling: 'mean', normalize: true }` |

`normalize: true` is what makes cosine similarity a bare dot product. The search
function does not divide by anything, so a rebuild that drops normalisation
produces scores that are not similarities and a `MIN_SCORE` that means nothing.

`PRECISION` is six because a component of a unit vector rounded that far is off
by at most 5e-7, so a 384-term dot product moves by under 2e-4. That is four
orders of magnitude below the score floor and invisible at the two decimals the
page prints. Full float64 would roughly triple the file for no observable
difference.

### The corpus format

`corpus.json` is generated and must never be hand-edited. Top level:

| Key | Type | Value |
| --- | --- | --- |
| `model` | string | `all-MiniLM-L6-v2` |
| `dtype` | string | `q8` |
| `dims` | number | `384` |
| `embedding_input` | string | `{heading}. {text}` |
| `source` | string | `portfolio.html` |
| `generator` | string | `scripts/build-corpus.html` |
| `passages` | array | One object per passage |

Each passage:

| Key | Type | Meaning |
| --- | --- | --- |
| `heading` | string | The `h2` of the `.project` section it came from |
| `category` | string | The `.project--category` text, or empty |
| `anchor` | string | `portfolio.html#<section id>`, or bare `portfolio.html` |
| `text` | string | The embedded prose, whitespace collapsed |
| `parts` | array of string | **List passages only.** The items before joining. |
| `vector` | array of 384 numbers | The embedding, rounded to six places |

`embedding_input` is recorded because it is not recoverable from the vectors and
it must match what `js/ask.js` does at query time. The two sides are deliberately
asymmetric: a **passage** is embedded as `` `${heading}. ${text}` ``, a **query**
is embedded bare. A rebuild that prepends anything to the query moves it away
from the whole corpus at once, and nothing raises an error.

`parts` exists for the checker. A list passage's `text` is its items joined with
`". "`, and that separator is not on the page, so the joined string is not
something `check_corpus.py` could search for. The items are, so they are carried.

### What counts as a passage

This is the single most reconstruction-critical part of the feature, because it
decides whether a rebuilt index has the same entries at all. It is defined in
`chunk()` in `scripts/build-corpus.html` and nowhere else.

For each `.project` section of `portfolio.html`, in document order:

1. `heading` is the section's first `h2`, trimmed. If there is none, the literal
   `Portfolio`.
2. `anchor` is `portfolio.html#<id>` if the section has an `id`, else
   `portfolio.html`.
3. `category` is the section's `.project--category` text, trimmed, or empty.
4. **Every `p` in the section becomes a passage**, except: any `p` carrying
   `.project--meta`, and any whose text is shorter than 40 characters after
   whitespace collapsing. Text is `textContent` with `/\s+/g` replaced by a
   single space, then trimmed.
5. **Each `.project--impact`, `.skills--list` and `.history--list` becomes one
   passage**, not one per item. Its children's texts are collapsed and trimmed,
   empties dropped, and the result joined with `". "` for `text` and carried
   verbatim as `parts`. A list that ends up empty is skipped.

The bullets and skills are the highest-signal, lowest-word content on the page.
One bullet at a time is too short to carry meaning, which is why each list is one
passage.

Note what this makes load-bearing: five class names, `.project`,
`.project--category`, `.project--impact`, `.skills--list` and `.history--list`,
are an interface between the stylesheet and the search. Renaming one in
`portfolio.html` silently drops a section from retrieval. That was worse when
this ran on every page load; now it runs once and `check_corpus.py` notices.

### The output format

The generator does not use `JSON.stringify` on the whole object. It writes the
scalar keys one per line and then **one passage per line**, so a diff shows which
passage changed rather than one 100 KB line that changed. A rebuild that pretty
prints or minifies differently will produce a file that is semantically identical
and diffs uselessly, which defeats the property the next section exists to buy.

### Model configuration

Both `js/ask.js` and `scripts/build-corpus.html` set exactly this, and the two
must not drift:

```js
env.allowRemoteModels = false;
env.allowLocalModels = true;
env.localModelPath = 'assets/models/';
env.backends.onnx.wasm.wasmPaths = {
  mjs:  new URL('vendor/transformers/ort-wasm-simd-threaded.mjs',  document.baseURI).href,
  wasm: new URL('vendor/transformers/ort-wasm-simd-threaded.wasm', document.baseURI).href,
};
env.backends.onnx.wasm.numThreads = 1;
env.backends.onnx.wasm.proxy = false;
pipeline('feature-extraction', MODEL_ID, { dtype: 'q8', device: 'wasm' });
```

Each line is defending against a specific default.

- **`allowRemoteModels` and `wasmPaths`.** Left alone, Transformers.js pulls the
  ONNX runtime from `cdn.jsdelivr.net` and the weights from `huggingface.co`.
  That would mean a third party serving executable WebAssembly and 16 MB of
  weights into the page on every visit, and it would make the claim that the
  question never leaves the browser depend on someone else's server. Both are
  pinned at vendored same-origin files.
- **`numThreads = 1`.** Not a preference. Threaded ONNX Runtime needs
  `SharedArrayBuffer`, which needs cross-origin isolation, which needs COOP and
  COEP response headers, and GitHub Pages cannot set headers. Measured on the
  deployed page: `self.crossOriginIsolated` is `false` and `SharedArrayBuffer` is
  `undefined`. Asking for threads fails at runtime rather than falling back, so
  the value is written down next to its reason.
- **The library build.** `vendor/transformers/transformers.min.js`, the
  self-contained build, **not** `transformers.web.min.js`. The web build is
  lighter but ships bare import specifiers such as `from "onnxruntime-web/webgpu"`
  that only a bundler can resolve, and this repository has no bundler.

### Why the generator is a web page

`scripts/build-corpus.html` is opened in a browser and pressed, which is an odd
shape for a build tool and is the right one here. The vectors must be the ones
the shipped code would have produced; a vector that differs does not raise an
error, it just retrieves worse. Running the generator in the same engine, against
the same vendored runtime and the same weights, makes that true by construction.

Node cannot do it. The vendored build is the web build: its ONNX backend registry
is empty outside a browser and its loader resolves model paths through `fetch()`,
which throws on a filesystem path. A Python generator would mean taking on
`onnxruntime`, which the no-toolchain stance rules out.

The page carries `<base href="../" />`, and that is load-bearing rather than
tidy. It makes every path in the file resolve from the site root exactly as it
does on `index.html`, so the model configuration above can be a verbatim copy
rather than the same thing rewritten with `../` in front of it. An earlier
version did the rewriting, and an absolute `localModelPath` made Transformers.js
skip loading the tokenizer altogether: no request, no error, and a pipeline that
threw `this.tokenizer is not a function` only at inference.

The page also carries `<meta name="robots" content="noindex, nofollow">`, is
absent from `sitemap.xml` and from every nav, and sits under `scripts/`, which is
outside the `*.html` globs `check_repo.py` and `check_htmx.py` walk.

### Why it embeds one passage at a time

The generator loops one text per `extractor()` call. This is the slow way round
and it is deliberate.

Batching pads every sequence in a batch to the length of its longest member, and
under 8-bit quantisation that padding changes the output. Editing the prose of
one passage once moved the committed vectors of the eleven unedited passages
sharing its batch, to a cosine of 0.997 against themselves. Nothing catches that,
because `check_corpus.py` compares text: a vector that shifted because a
neighbour got longer looks exactly like one that did not shift at all, and a
two-passage edit lands as a twelve-passage diff.

Unbatched, a vector is a function of its own text and nothing else, so
regenerating after an unrelated edit produces a diff you can read. It is also
what the live path does: `ask.js` embeds a single query string, never a batch, so
a sequence sitting alone in its call is the case the shipped code exercises.

### Retrieval

`search()` is an exhaustive scan. `corpus.map` computes a dot product against
every passage, sorts descending, filters to `score >= MIN_SCORE` and slices to
`k`. There is no index and no divide, because the vectors arrive L2-normalised.

This is the right answer at this size rather than a concession. A few dozen
passages of 384 floats is under 10,000 multiply-accumulates per query, which is
microseconds, and is dwarfed by the roughly 25 ms spent embedding the question.
An approximate index (HNSW, IVF, a vector database) exists to avoid a scan that
has become expensive, and buys that with build time, memory, tuning and recall
you can no longer reason about exactly. The crossover is around 10^5 vectors. If
this corpus ever grows three orders of magnitude, revisit it.

### Load-time validation

`loadCorpus()` throws, with a message, on any of:

- the fetch not being `ok`;
- `data.model !== MODEL_ID`;
- `data.passages` empty or absent;
- any passage whose `vector.length !== data.dims`.

The model check is two lines and the alternative is a failure with no symptom: a
corpus built against a different model has vectors of the right shape that do not
live in the same space as the query, so it loads without complaint and retrieves
nonsense.

### The DOM contract

Rebuild the markup with these ids. `js/ask.js` looks all of them up by
`getElementById` at module scope, so a missing one is a null dereference on the
first interaction.

| Id | Element | Contract |
| --- | --- | --- |
| `ask` | `section` | `aria-labelledby="ask--heading"` |
| `ask--heading` | `h2` | |
| `ask--gate` | `div` | **`hidden` in the markup.** Revealed by the last line of `ask.js`. |
| `ask--load` | `button type="button"` | Press starts the load |
| `ask--progress-wrap` | `div` | `hidden` until the load starts |
| `ask--progress` | `progress max="100" value="0"` | Determinate. Has a `.visually-hidden` `label`. |
| `ask--progress-label` | `p` | Byte counts during download |
| `ask--form` | `form` | `hidden` until ready. Submit is prevented and routed to `ask()`. |
| `ask--input` | `input type="search"` | `required`, `autocomplete="off"` |
| `ask--submit` | `button type="submit"` | |
| `ask--examples` | `div` | `hidden` until ready. Delegated click on `button[data-q]`. |
| `ask--examples-label` | `p` | `aria-labelledby` target for the `ul` |
| `ask--status` | `p` | `role="status"`, `aria-live="polite"` |
| `ask--results` | `div` | Replaced wholesale on each search |

Two things outside the gate and never hidden: a `.ask--straight` link to
`portfolio.html`, for a visitor who can run the search and would rather read; and
a `<noscript>` block, for a visitor who cannot. They are different readers and
both are served.

Results are built as an `ol.ask--hits` carrying an explicit `role="list"`,
because the stylesheet sets `list-style: none` and that makes VoiceOver drop the
list semantics. Each `li.ask--hit` holds a head paragraph with a link to
`hit.anchor` labelled `hit.heading`, an optional `.ask--hit-category` span, a
`.ask--hit-score` span showing the score to two decimals with the four-decimal
value in its `title`, and a `.ask--hit-text` paragraph with the passage.

### The state machine

1. Markup loads with `#ask--gate`, `#ask--form`, `#ask--examples` and
   `#ask--progress-wrap` all `hidden`.
2. `js/ask.js` runs and sets `els.gate.hidden = false`. This is the last
   statement in the file. If the script never runs, nothing is offered.
3. Press `#ask--load`. The `loading` flag guards re-entry, `aria-disabled="true"`
   goes on the button, `#ask--progress-wrap` is revealed, and `loadModel()` and
   `loadCorpus()` run concurrently under `Promise.all`.
4. Progress is summed across files. Transformers.js reports per-file `loaded` and
   `total`; both are accumulated into a `Map` keyed by file so the bar reflects
   the whole job rather than the current file.
5. On success: reveal the form and the examples, set the status to the passage
   count, **focus `#ask--input`, and only then hide `#ask--gate`.** That order is
   the contract. Hiding the pressed button while focus is still on it drops focus
   to `<body>`.
6. On failure: clear `loading`, re-hide the progress, remove `aria-disabled`, put
   the message in the status, and `console.error` the stack.
7. Submit or press an example. The `searching` flag guards re-entry,
   `#ask--submit` gets `aria-disabled`, the query is embedded, searched and
   rendered, and the elapsed milliseconds are appended to the status.

Neither button ever uses the `disabled` property, and the rule is not stylistic.
An earlier version disabled `#ask--load` on press, reasoning that it leaves the
page once the model is up. It does, but it leaves when the load *finishes*, so
disabling it on press stranded a keyboard user on `<body>` for the entire
multi-second download. `aria-disabled` announces the state without touching
focus; a plain flag is what actually prevents the work happening twice.

### The download, and being honest about it

The model and runtime are large enough that the feature does not load on page
view. `#ask--gate` states the cost before spending it and waits for a press.

The measured table, the compression figures and the `curl` that re-measures them
live in [ARCHITECTURE.md](../ARCHITECTURE.md#the-search) and are not copied here,
because the number is measured and will drift. The gate must exist, state its
cost, and quote the transferred figure rather than the on-disk one.

---

## Acceptance criteria

| ID | Criterion | Evidence |
| --- | --- | --- |
| F01-AC01 | The question is never transmitted. Every fetch in the query path is same-origin and happens before the first query. | Structural: `js/ask.js` fetches only the runtime, the weights and `corpus.json` |
| F01-AC02 | `allowRemoteModels` is false and `wasmPaths` is overridden, in both `ask.js` and the generator. | Structural, and a network panel with no third-party host |
| F01-AC03 | Every passage in the index is still present, word for word, in `portfolio.html`. | `scripts/check_corpus.py`, CI |
| F01-AC04 | Every passage anchor resolves to a section id that exists on `portfolio.html`. | `scripts/check_corpus.py`, CI |
| F01-AC05 | The index declares the model and dimension count the runtime expects. | `scripts/check_corpus.py`, CI, and `loadCorpus()` at runtime |
| F01-AC06 | Every passage vector has exactly `dims` components. | Asserted at build time and again at load time |
| F01-AC07 | Re-running the generator with no prose change reproduces `corpus.json` byte for byte. | Unbatched embedding, by construction. Verified by re-running and diffing. |
| F01-AC08 | Passages are embedded with the heading prepended, queries bare, and the file records which. | `embedding_input` in `corpus.json`, `embed([q], 1)` in `ask.js` |
| F01-AC09 | Nothing downloads until the visitor presses the gate. | Human, network panel |
| F01-AC10 | Prose added to `portfolio.html` is actually indexed. | **Human only.** Load the page, press load, confirm the passage count went up. No static check can catch this, because an unindexed paragraph looks exactly like one the generator was never meant to see. |
| F01-AC11 | Neither button uses the `disabled` property, and focus survives both a load and a query. | Human, keyboard traversal |
| F01-AC12 | Focus reaches `#ask--input` before `#ask--gate` is hidden. | Structural: statement order in `start()`. Human to confirm. |
| F01-AC13 | Results announce to a screen reader without stealing focus. | Human, `aria-live` on `#ask--status` |
| F01-AC14 | The page is usable with JavaScript off: the gate stays hidden and the straight link works. | Human, JavaScript disabled |
| F01-AC15 | Contrast holds at 4.5:1 for text in both flavours, checked against Latte. | Human, per colour scheme |

Criteria marked human are not weaker than the CI ones. They are the ones a static
check cannot reach, which is exactly why they are written down.

---

## Deferred

| Item | Note |
| --- | --- |
| A smaller download | Any change to the model changes retrieval quality. Needs measuring against real queries before it is chosen, not assumed. |
| Highlighting the matched span | Currently the whole passage is returned. Span-level scoring would need per-sentence vectors and a larger index. |
| Query logging of any kind | Rejected, not deferred. It would dissolve the property the feature exists to demonstrate. |
