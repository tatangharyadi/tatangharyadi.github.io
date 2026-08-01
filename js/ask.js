// Semantic search over this site's own case studies, running entirely in the
// browser: a sentence-transformer embeds the question, and the answer is the
// passage whose meaning is closest to it.
//
// This is the one first-party script in the repository, and AGENTS.md asks for a
// reason rather than a shortcut. The reason is that there is nothing to fetch.
// htmx carries every other interaction here because every other interaction is a
// request for a piece of HTML that a server already has. This one is a matrix
// multiply against 384-dimensional vectors that exist only in this tab: no
// endpoint could answer it without being sent the question, which is exactly the
// property the page is built to avoid. Markup cannot express inference.
//
// Everything it needs is served from this origin — the library, the WebAssembly
// runtime, the tokenizer and the model weights. See the note on wasmPaths below
// for why that is load-bearing and not merely tidy.

const MODEL_ID = 'all-MiniLM-L6-v2';

// Where the corpus comes from. Deliberately not a generated JSON file committed
// beside the page: the portfolio page IS the corpus, parsed at runtime. A
// build step that emitted embeddings would be a second copy of the prose that
// could drift from the first, and this repository already carries four copies of
// the palette and eight of the case study deck precisely because that bargain is
// sometimes unavoidable. Here it is avoidable, so the drift is designed out
// rather than policed by another checker.
const SOURCES = [
  { url: 'portfolio.html', label: 'Portfolio' },
];

const TOP_K = 5;

// Retrieval quality falls off a cliff below roughly this similarity, and showing
// the closest passage regardless of how far away it is produces confident
// nonsense. Better to say nothing matched.
const MIN_SCORE = 0.25;

const els = {
  gate: document.getElementById('ask--gate'),
  load: document.getElementById('ask--load'),
  progressWrap: document.getElementById('ask--progress-wrap'),
  progress: document.getElementById('ask--progress'),
  progressLabel: document.getElementById('ask--progress-label'),
  form: document.getElementById('ask--form'),
  input: document.getElementById('ask--input'),
  submit: document.getElementById('ask--submit'),
  results: document.getElementById('ask--results'),
  status: document.getElementById('ask--status'),
  examples: document.getElementById('ask--examples'),
};

let extractor = null;
let corpus = [];

/* -------------------------------------------------------------------------- */
/* Corpus                                                                     */
/* -------------------------------------------------------------------------- */

// Split the portfolio page into passages. Each passage keeps the heading it
// sat under, because a paragraph about "the rescue turned on scope" means very
// little on its own and a great deal under "Multinational Retail Company". The
// heading is prepended for embedding and shown separately in the result, so the
// user sees where an answer came from.
function chunk(doc, sourceUrl, sourceLabel) {
  const out = [];
  for (const section of doc.querySelectorAll('.project')) {
    const heading = section.querySelector('h2')?.textContent.trim() ?? sourceLabel;
    const anchor = section.id ? `${sourceUrl}#${section.id}` : sourceUrl;
    const category = section.querySelector('.project--category')?.textContent.trim() ?? '';

    for (const p of section.querySelectorAll('p')) {
      if (p.classList.contains('project--meta')) continue;
      const text = p.textContent.replace(/\s+/g, ' ').trim();
      if (text.length < 40) continue;
      out.push({ heading, category, anchor, text });
    }

    // The impact bullets and the skills list are the highest-signal, lowest-word
    // content on the page. Embedded one bullet at a time they are too short to
    // carry meaning, so each list becomes a single passage.
    for (const list of section.querySelectorAll('.project--impact, .skills--list, .history--list')) {
      const items = [...list.children]
        .map((n) => n.textContent.replace(/\s+/g, ' ').trim())
        .filter(Boolean);
      if (!items.length) continue;
      out.push({ heading, category, anchor, text: items.join('. ') });
    }
  }
  return out;
}

async function loadCorpus() {
  const all = [];
  for (const src of SOURCES) {
    const res = await fetch(src.url);
    if (!res.ok) throw new Error(`could not read ${src.url} (${res.status})`);
    const doc = new DOMParser().parseFromString(await res.text(), 'text/html');
    all.push(...chunk(doc, src.url, src.label));
  }
  if (!all.length) throw new Error('the corpus came back empty');
  return all;
}

/* -------------------------------------------------------------------------- */
/* Model                                                                      */
/* -------------------------------------------------------------------------- */

function setProgress(fraction, label) {
  if (fraction === null) {
    els.progress.removeAttribute('value');
  } else {
    els.progress.value = Math.round(fraction * 100);
  }
  els.progressLabel.textContent = label;
}

async function loadModel() {
  const { pipeline, env } = await import('../vendor/transformers/transformers.min.js');

  // Nothing may be fetched from anywhere but this origin.
  //
  // transformers.js defaults env.backends.onnx.wasm.wasmPaths to a
  // cdn.jsdelivr.net URL and will happily pull the model from huggingface.co, so
  // leaving these at their defaults would mean a third party serving executable
  // WebAssembly and 22 MB of weights into this page on every visit. That is the
  // same objection AGENTS.md raises against an unpinned CDN script, an order of
  // magnitude larger, and it would make the page's central claim — that the
  // question never leaves the browser — depend on someone else's server. Both
  // paths are therefore pinned at vendored, same-origin files.
  env.allowRemoteModels = false;
  env.allowLocalModels = true;
  env.localModelPath = 'assets/models/';
  env.backends.onnx.wasm.wasmPaths = {
    mjs: new URL('vendor/transformers/ort-wasm-simd-threaded.mjs', document.baseURI).href,
    wasm: new URL('vendor/transformers/ort-wasm-simd-threaded.wasm', document.baseURI).href,
  };

  // Single-threaded, and not by preference. Threaded ONNX Runtime needs
  // SharedArrayBuffer, which needs cross-origin isolation, which needs COOP and
  // COEP response headers — and GitHub Pages cannot set headers. Measured here:
  // self.crossOriginIsolated is false and SharedArrayBuffer is undefined. Asking
  // for threads would fail at runtime rather than fall back, so the number is
  // stated explicitly to keep the reason next to the value.
  env.backends.onnx.wasm.numThreads = 1;
  env.backends.onnx.wasm.proxy = false;

  // Determinate progress, not a spinner. The download is tens of megabytes on a
  // connection we know nothing about, and a spinner in front of that is a lie
  // about whether anything is happening. transformers.js reports per-file byte
  // counts; totals are summed across files so the bar reflects the whole job.
  const seen = new Map();
  const onProgress = (p) => {
    if (p.status === 'progress' && p.total) {
      seen.set(p.file, { loaded: p.loaded, total: p.total });
      let loaded = 0;
      let total = 0;
      for (const v of seen.values()) {
        loaded += v.loaded;
        total += v.total;
      }
      const mb = (n) => (n / 1048576).toFixed(1);
      setProgress(loaded / total, `Downloading model — ${mb(loaded)} of ${mb(total)} MB`);
    } else if (p.status === 'ready') {
      setProgress(1, 'Starting the runtime…');
    }
  };

  return pipeline('feature-extraction', MODEL_ID, {
    dtype: 'q8',
    device: 'wasm',
    progress_callback: onProgress,
  });
}

// Embed in batches. One call with every passage would pad them all to the
// longest sequence in the set and waste most of the multiply; batching keeps the
// padding local to similar-length neighbours.
async function embed(texts, batchSize = 16, onBatch = null) {
  const vectors = [];
  for (let i = 0; i < texts.length; i += batchSize) {
    const batch = texts.slice(i, i + batchSize);
    const out = await extractor(batch, { pooling: 'mean', normalize: true });
    vectors.push(...out.tolist());
    if (onBatch) onBatch(Math.min(i + batchSize, texts.length), texts.length);
  }
  return vectors;
}

/* -------------------------------------------------------------------------- */
/* Search                                                                     */
/* -------------------------------------------------------------------------- */

// The vectors come out of the model L2-normalised, so cosine similarity is just
// a dot product and there is nothing to divide by.
//
// This is a linear scan, and that is the right answer at this size rather than a
// concession. The corpus is a few dozen passages of 384 floats — 22 at the time
// of writing, so under 10k multiply-accumulates per query, which is microseconds
// and is dwarfed by the ~25ms spent embedding the question. An approximate
// index — HNSW, IVF, a vector database — exists to avoid a scan that has become
// expensive, and buys that with build time, memory, tuning and recall you can no
// longer reason about exactly. None of those costs are worth paying to speed up
// something already faster than a single frame. The crossover is around 10^5
// vectors; if this corpus ever grows three orders of magnitude, revisit it.
function search(queryVec, k) {
  const scored = corpus.map((c, i) => {
    let dot = 0;
    const v = c.vector;
    for (let d = 0; d < queryVec.length; d++) dot += queryVec[d] * v[d];
    return { ...c, score: dot, i };
  });
  scored.sort((a, b) => b.score - a.score);
  return scored.filter((s) => s.score >= MIN_SCORE).slice(0, k);
}

function render(hits, query) {
  els.results.replaceChildren();

  if (!hits.length) {
    const p = document.createElement('p');
    p.className = 'ask--empty';
    p.textContent =
      'Nothing in the case studies is close enough to that to be worth showing. ' +
      'This searches what is written on this site, so questions about anything ' +
      'else will come back empty.';
    els.results.append(p);
    els.status.textContent = `No passage matched "${query}".`;
    return;
  }

  const ol = document.createElement('ol');
  ol.className = 'ask--hits';

  for (const hit of hits) {
    const li = document.createElement('li');
    li.className = 'ask--hit';

    const head = document.createElement('p');
    head.className = 'ask--hit-head';
    const a = document.createElement('a');
    a.href = hit.anchor;
    a.textContent = hit.heading;
    head.append(a);
    if (hit.category) {
      const cat = document.createElement('span');
      cat.className = 'ask--hit-category';
      cat.textContent = hit.category;
      head.append(cat);
    }

    const score = document.createElement('span');
    score.className = 'ask--hit-score';
    score.textContent = hit.score.toFixed(2);
    // The bar is decoration for a number that is already in the text beside it.
    score.setAttribute('title', `Cosine similarity ${hit.score.toFixed(4)}`);
    head.append(score);

    const body = document.createElement('p');
    body.className = 'ask--hit-text';
    body.textContent = hit.text;

    li.append(head, body);
    ol.append(li);
  }

  els.results.append(ol);
  els.status.textContent =
    `${hits.length} passage${hits.length === 1 ? '' : 's'} matched, ` +
    `closest first. Best similarity ${hits[0].score.toFixed(2)}.`;
}

/* -------------------------------------------------------------------------- */
/* Wiring                                                                     */
/* -------------------------------------------------------------------------- */

// The model is 22 MB and the runtime 13 MB, so it loads when someone asks for it
// and never on page load. Charging every visitor tens of megabytes to read a
// page they might not interact with would be the wrong default no matter how
// good the feature is.
//
// Neither this nor ask() uses the `disabled` property, for the same reason the
// carousel arrows do not — see AGENTS.md#accessibility-invariants. Disabling the
// element the user just activated moves focus to <body>, and re-enabling it later
// does not bring focus back. Here that would strand a keyboard user for the whole
// multi-second model download with no way back into the component. `aria-disabled`
// announces the state without touching focus, and a plain guard flag is what
// actually prevents a second press doing the work twice — the same division of
// labour hx-sync provides for the arrows.
let loading = false;

async function start() {
  if (loading) return;
  loading = true;
  els.load.setAttribute('aria-disabled', 'true');
  els.progressWrap.hidden = false;
  setProgress(0, 'Fetching the runtime…');

  try {
    const [model, chunks] = await Promise.all([loadModel(), loadCorpus()]);
    extractor = model;
    corpus = chunks;

    setProgress(null, `Embedding ${corpus.length} passages…`);
    const vectors = await embed(
      corpus.map((c) => `${c.heading}. ${c.text}`),
      16,
      (done, total) => setProgress(done / total, `Embedding passages — ${done} of ${total}`)
    );
    corpus.forEach((c, i) => { c.vector = vectors[i]; });

    // Focus is moved to the input below *before* the gate is hidden, so the
    // button the user pressed never disappears from under a live focus.
    els.form.hidden = false;
    els.examples.hidden = false;
    els.status.textContent =
      `Ready. ${corpus.length} passages indexed in this tab. Ask a question.`;
    els.input.focus();
    els.gate.hidden = true;
  } catch (err) {
    loading = false;
    els.progressWrap.hidden = true;
    els.load.removeAttribute('aria-disabled');
    els.status.textContent = `The model could not be loaded: ${err.message}`;
    // Left in place deliberately: if this ever fires in the wild, the console is
    // the only place with the stack.
    console.error(err);
  }
}

let searching = false;

async function ask(query) {
  const q = query.trim();
  if (!q || !extractor || searching) return;

  searching = true;
  els.submit.setAttribute('aria-disabled', 'true');
  els.status.textContent = 'Searching…';
  try {
    const t0 = performance.now();
    const [queryVec] = await embed([q], 1);
    const hits = search(queryVec, TOP_K);
    const ms = Math.round(performance.now() - t0);
    render(hits, q);
    // Appended rather than replacing the status: the count is the useful part,
    // the timing is the interesting part.
    els.status.textContent += ` (${ms} ms, in this browser.)`;
  } catch (err) {
    els.status.textContent = `Search failed: ${err.message}`;
    console.error(err);
  } finally {
    searching = false;
    els.submit.removeAttribute('aria-disabled');
  }
}

els.load.addEventListener('click', start);

els.form.addEventListener('submit', (e) => {
  e.preventDefault();
  ask(els.input.value);
});

els.examples.addEventListener('click', (e) => {
  const btn = e.target.closest('button[data-q]');
  if (!btn) return;
  els.input.value = btn.dataset.q;
  ask(btn.dataset.q);
});

// The gate is the only thing a no-JS visitor would see stuck, so it starts
// hidden in the markup and is revealed here. If this script never runs, the
// static fallback below it is what remains — the full case studies, linked.
els.gate.hidden = false;
