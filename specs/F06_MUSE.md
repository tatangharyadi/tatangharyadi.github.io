# F06: Muse, an in-browser avatar illustrator

**Status:** proposed, not implemented. This document is the design decision
itself — no `js/muse.js`, `muse.html`, or vendored/fetched model exists yet.
Everything below is the plan a first implementation would follow, written up
before any code so the CDN exception it requires (see "The model has to leave
same-origin" below) is argued and reviewed on its own terms first.

### Pending

**F06-AC01 is now verified.** A standalone browser prototype (not committed
to this repo — a scratch harness, described below) confirmed img2img is
achievable against the real `schmuell/sd-turbo-ort-web` files served live
from Hugging Face, with three concrete findings that change this spec's
risk picture:

- **The reference implementation behind this HF repo is
  [`guschmue/ort-webgpu`](https://github.com/guschmue/ort-webgpu)'s
  `sd-turbo/index.html`** (the repo's author, `schmuell`, is a Microsoft ORT
  engineer; the model card names no demo, but the file is easy to find once
  you know the model was exported for it). It is text-to-image only — it
  never loads `vae_encoder` and starts every generation from
  `randn_latents()`. The prototype extended that exact file rather than
  starting from scratch: same session options, same `freeDimensionOverrides`,
  same single-step "poor man's EulerA" scheduler math, with `vae_encoder`
  added and a `strength` parameter controlling how much noise is mixed into
  the encoded photo latent before the existing UNet call.
- **`vae_encoder`'s input must be an explicit `float16` tensor, not
  `float32`.** Feeding it `float32` (as every other input in the reference
  demo is fed) fails immediately with `Unexpected input data type. Actual:
  (tensor(float)), expected: (tensor(float16))`. `text_encoder`, `unet` and
  `vae_decoder` all accept `float32` directly — this constraint is specific
  to `vae_encoder`, and undocumented anywhere in the model card.
  `vae_encoder`'s `latent_sample` output is also `float16` and needs
  converting back before the noise-mixing math.
- **`vae_encoder` cannot run on the WebGPU execution provider at all on the
  `guschmue/ort-webgpu` reference demo's pinned
  `onnxruntime-web@1.18.0-dev.20240118-28a16c223c` build, but this is a
  version problem, not an architectural one, and it is already fixed
  upstream.** That build fails with `[WebGPU] Kernel "[Clip] /Clip" failed.
  Error: Invalid data type` — there is no `fp16` WebGPU kernel for `Clip` in
  it. [PR #21584](https://github.com/microsoft/onnxruntime/pull/21584)
  ("[js/webgpu] support float16 for Clip") added exactly that kernel and
  merged 2024-08-28; the fix first shipped in `onnxruntime-web@1.19.2` and
  is present in every release since, including current stable `1.27.0`.
  Re-running the same prototype pinned to `onnxruntime-web@1.20.1` instead
  confirmed this directly: `vae_encoder` now runs on WebGPU with no
  execution-provider override, no error, in **595ms** — not 52 seconds on
  `wasm`. One accompanying API change came with the newer build: it expects
  a native `Float16Array`-backed tensor rather than a `Uint16Array` of
  manually bit-packed IEEE-754 halves (the old build predates
  `Float16Array` support in its JS API); Chrome already ships native
  `Float16Array`, so this is a straightforward code change, not a new risk.
  With this fix, a full pipeline pass — `text_encoder` (346ms) +
  `vae_encoder` (595ms) + `unet` (706ms) + `vae_decoder` (819ms) — completed
  in about **2.5 seconds total**, all four sessions on WebGPU, no CPU
  fallback anywhere. **This retires the 52-second bottleneck as a concern:
  the fix is to pin a current `onnxruntime-web` release (`1.20.1` or later,
  not the reference demo's stale `1.18.0-dev` pin), not to accept a
  degraded CPU path.**
- **The noised-latent mechanism works and responds to `strength` in the
  expected direction.** At `strength=0.5` the output was a coherent,
  recognizably-derived stylized portrait (same head position and framing as
  the input, restyled per the prompt). At `strength=0.15` the output stayed
  close to unedited input pixels with almost no stylization — consistent
  with feeding the UNet a much lower effective noise level than SD-Turbo's
  single-step distillation was tuned against. This is qualitative, not a
  tuned production default, but it demonstrates the mechanism (encode →
  partial noise → single UNet pass → decode) is sound, not just that it
  runs without erroring.

**F06-AC02 is resolved: 2.58GB is acceptable.** The real fetched size,
measured live from `https://huggingface.co/schmuell/sd-turbo-ort-web/resolve/main/`
in the prototype (`text_encoder/model.onnx` 681.4MB, `unet/model.onnx`
1733.4MB, `vae_decoder/model.onnx` 99.1MB, `vae_encoder/model.onnx`
68.4MB), matches the repo's file listing exactly — these are the actual
bytes a visitor's browser transfers, not an overestimate from unused files.
Two other mirrors checked (`onnxruntime/sd-turbo`, `tlwu/sd-turbo-onnxruntime`)
total the same 2.58GB to within rounding, i.e. the same fp32-scale export
re-hosted, not a smaller alternative. One genuinely smaller quantized
alternative was found — `MiCkSoftware/sd-turbo-onnx-q8-static-arm64` at
**1.45GB total** (int8, `unet/model.onnx.data` external-data split) — but it
was not the repo this spec or the prototype targeted, and its
accuracy/output-quality trade-off at int8 is untested here. 2.58GB, as a
one-time cached page load rather than a per-visit cost, is accepted as the
size budget for this spec.

One premise remains open:

- **Real-hardware inference latency is still unmeasured on ordinary
  consumer hardware.** All four prototype numbers above (~2.5s total, all
  WebGPU) came from this sandbox's environment, not a visitor's laptop
  integrated GPU in a real browser tab. With the `onnxruntime-web` version
  bump, there is no longer a known CPU-bound bottleneck to specifically
  budget for — but the whole pipeline still needs a real-hardware run
  before claiming ~2.5s (or whatever multiple of it a weaker GPU produces)
  is a "tolerable time on ordinary hardware."

The prototype itself is a throwaway HTML/JS harness (extending
`guschmue/ort-webgpu`'s demo file, served locally, driven with a browser
automation tool) — it was not committed to this repository and does not
need to be; it exists only to answer the question above. It reused the
already-vendored MediaPipe assets' sibling pattern of "fetch a real model,
run it for real" rather than mocking anything.

Everything past this point assumes the two remaining premises resolve
favorably. If the size or latency numbers come back too large, the fallback
is a much smaller, fixed-style neural style-transfer network (AnimeGAN-class,
low tens of MB, single canned look, no prompt control) instead of a
diffusion model — a strictly worse match for Draft's quality bar, but the
only vendorable option. That fallback is not designed here; it is only named
as the next thing to consider if this spec's premises fail.

---

## Overview

A visitor uploads a photo. Muse detects a face in it, generates a stylized
illustrated portrait from it, then generates a small set of additional
posed variants of that same stylized portrait — looking left, right, up,
down — and crossfades between them as the cursor moves, the same
gaze-following effect `RotatingAvatar` gives Draft's team-directory
avatars. Unlike Draft, none of this touches a server: the photo, the face
detector, the stylization model and the generated images all stay in the
visitor's own tab.

This is a deliberately smaller-scoped rebuild of Draft's two-step Gemini
pipeline (stylize once, then regenerate reposed variants from that
stylized result), not a byte-for-byte port: Draft calls a hosted model
that costs money per call and returns results in seconds; Muse has to run
the whole thing on whatever GPU the visitor's browser can reach, for free,
which is why the pose count and prompt complexity below are both smaller
than Draft's nine.

### Why this needs a generative model instead of MediaPipe alone

MediaPipe Face Landmarker — already vendored for Iris — gives geometry: 478
points and a head-pose transform. It cannot produce a stylized image; it can
only describe where a face is and which way it's turned. Muse needs both
halves: MediaPipe's landmarks to find the face and read its pose (reused,
not re-vendored — see "Key files" below), and a separate generative model
to actually paint a portrait. Landmarks and pixels are different jobs, the
same division Iris already draws between "the model that finds things" and
"the code that draws the game."

### Why this needs a diffusion model instead of a small style-transfer net

A dedicated style-transfer network (AnimeGAN-class, or the "fast neural
style" architecture from PyTorch's own examples) is real, small — often
under 20MB — and does run entirely in-browser via ONNX Runtime Web. It was
seriously considered here and set aside for one reason: those networks bake
in one fixed painterly filter at training time. There is no prompt, no
style parameter, no way to ask for "professional portrait" versus anything
else, and — more importantly for this feature — no way to ask the same
network for "the same face, turned to look left" instead of "the same
input pixels, filtered." Reposing is Draft's whole second pipeline step;
a fixed-filter net has no equivalent to it at all, only a single global
transform of whatever pixels go in. A diffusion model conditioned on a text
prompt is the only realistic way to get both a controllable style and a
controllable pose out of one model family, which is why this spec reaches
for SD-Turbo despite its much larger size and unresolved feasibility
questions above.

### Why fewer poses than Draft's nine

Draft's Gemini calls run on Google's infrastructure, in parallel, at a
per-call cost the product absorbs. Every additional pose Muse generates is
one more full diffusion pass on the visitor's own GPU, blocking their own
tab. Center, left, right, up and down — five poses, not nine — is this
spec's starting point, cutting the four diagonals Draft includes. If the
unresolved latency question above comes back favorable, restoring the
diagonals is a small, additive change; if it comes back worse than hoped,
five may still need to shrink further. This is stated as a starting point,
not a measured one.

---

## Key files (planned)

| File | Role |
| --- | --- |
| `muse.html` | The page: upload gate, canvas/preview, model-load progress, the `connect-src` CSP tag (see below) |
| `css/muse.css` | Upload/preview layout. No colour of its own beyond the shared palette. |
| `js/muse.js` | Face detection via the already-vendored MediaPipe Face Landmarker, model fetch + load, stylization + repose inference calls, crossfade rendering |

`vendor/mediapipe/tasks-vision` and
`assets/models/face-landmarker/face_landmarker.task` are **reused as-is,
not re-vendored** — the same committed model Iris already loads (see
F05's "Why this needs MediaPipe Tasks Vision" section). Muse is the second
feature to load that runtime independently; Iris and Muse do not share a
JS module, only the same vendored assets on disk.

The stylization model itself is **fetched at runtime from Hugging Face,
not vendored** — see "The model has to leave same-origin" below for why
this is the one exception to the site's committed-asset precedent.

---

## Architecture (planned)

```
visitor presses #muse--upload, picks a photo
        │
        ├─ FilesetResolver.forVisionTasks('vendor/mediapipe/tasks-vision/wasm')
        │      └─ FaceLandmarker.createFromOptions({ modelAssetPath: FACE_MODEL_URL })
        │             └─ detect() on the uploaded image (not a video stream —
        │                one still frame, no camera, no loop)
        │
        └─ fetch stylization model from https://huggingface.co/schmuell/sd-turbo-ort-web
               (text_encoder, tokenizer, unet, vae_encoder, vae_decoder, scheduler)
               via ONNX Runtime Web + WebGPU — see Pending above for the
               unresolved size/latency questions this step depends on

face crop + landmarks (from MediaPipe) + stylization prompt
        │
   vae_encoder(photo crop) → latent
        │
   partially noise the latent (img2img strength parameter — the
   unverified step from Pending above) → UNet denoise loop, conditioned on
   a fixed "professional portrait illustration" prompt (mirroring Draft's
   DEFAULT_STYLE, not a visitor-editable field) → vae_decoder → center pose

center pose latent
        │
   for each of left / right / up / down:
       repeat the UNet loop from the center latent, conditioned on a
       fixed per-direction prompt (mirroring Draft's REPOSE_STYLE — "same
       face, same style, only the head angle changes") → vae_decoder →
       that direction's pose

five generated images held as in-memory canvas/blob data (never uploaded
anywhere)
        │
   crossfade renderer: same cursor-relative nearest-direction logic as
   Draft's RotatingAvatar (`js/muse.js` reimplements it in plain JS — no
   React here — rather than importing a Next.js component)
```

---

## Scope cuts

- **Upload only, no camera.** A camera boundary is a real decision on this
  site (see `js/mocap.js`'s own argument for one) and this feature doesn't
  need it: a single already-taken photo is a strictly simpler input than a
  live feed, and the "visitor's own face never leaves the browser" claim is
  easier to state for a file that's read once than a stream that's read
  continuously.
- **No editable style prompt.** The stylization and repose prompts are
  fixed, authored strings — mirroring Draft's `DEFAULT_STYLE`/`REPOSE_STYLE`
  constants — not a free-text field. A free-text diffusion prompt is a much
  larger surface (arbitrary content generation) than this feature needs to
  take on.
- **Five poses, not nine.** See "Why fewer poses than Draft's nine" above.
- **No recording, no server-side anything.** Same structural privacy claim
  as Echo's and Iris's: the photo, the crop, the landmarks and every
  generated image stay in the tab.
- **No retry/regenerate-one-pose control in v1.** If a generated pose looks
  wrong, the only path is re-uploading and starting over. A per-pose retry
  is a reasonable follow-up, not assumed here.

---

## The model has to leave same-origin, and Muse has to be honest about what that costs

Every other ML asset on this site — MediaPipe's runtime and models, the
MiniLM embedding model, LiteRT — is committed into the repository and
served from the same origin as everything else. That precedent exists for
a specific reason, argued throughout this repo: same-origin means a
visitor's browser never has to trust a host this site doesn't control, and
it's what makes "nothing here phones home" a claim about the network layer,
not just about the code.

Muse cannot meet that bar. The stylization model, at 2.58GB unquantized
(see Pending — the real fetched size may be smaller but is not yet
measured), is roughly 30 times the size of every ML asset this repo has
ever vendored *combined* (`vendor/` + `assets/models/` together measure
about 87MB). It is also far past GitHub's 100MB single-file push limit —
the model ships as multiple component files, but several of those
components (the UNet weights especially) are individually large enough to
be a live question — and it would consume a meaningful fraction of GitHub
Pages' ~1GB recommended total repository size on its own. There is no
quantized, vendorable-sized version of this specific model confirmed to
exist (see Pending). Vendoring is not a smaller, more-careful version of
this feature; it is not an option this feature has.

So Muse fetches the model from Hugging Face's CDN at runtime instead —
the site's first ML dependency that isn't same-origin. What that costs,
and what it doesn't:

- **Hugging Face's CDN sees the visitor's IP address and the fact that
  they loaded this specific model.** That is a real, new disclosure this
  site has never made before for any other feature. It is disclosed here
  rather than hidden.
- **The photo itself never crosses that boundary.** Only the frozen model
  weights come from Hugging Face; the uploaded photo, the MediaPipe
  landmarks, and every image the model generates are read, computed and
  held entirely in the browser tab, the same as every other feature here.
  The exception is scoped to "fetching a program," not "sending data out."
- **The fetch must be pinned to an exact revision, not a moving branch.**
  Hugging Face has no SRI-style subresource-integrity mechanism the way
  the vendored `htmx` CDN reference does, so `js/muse.js` must reference
  a specific commit hash of `schmuell/sd-turbo-ort-web`, not `main` —
  otherwise "what this site fetches" can change without a commit to this
  repository at all, which is exactly the risk vendoring exists to remove
  and htmx's version+digest pin exists to bound a different way.
- **`muse.html` needs a `Content-Security-Policy` naming Hugging Face's
  CDN host explicitly** — `connect-src 'self' huggingface.co
  *.hf.co` — rather than the CSP Iris uses to *block* an unwanted host,
  this one has to *allow* a specific one deliberately. It should still
  name nothing else: the same discipline as Iris's CSP, applied to let one
  host through instead of keeping every host out.

This directly narrows the non-functional requirement in
[specs/PRD.md](PRD.md) that reads "No third-party runtime the site does not
pin." Muse's answer is to pin as hard as the host allows — an exact
revision hash, an explicit CSP allowlist entry — rather than to claim the
requirement is unaffected. If that isn't a strong enough guarantee to
accept, the fallback in "Pending" above (a small, vendorable, fixed-style
network) is the alternative that keeps the requirement intact at the cost
of Draft-level quality.

---

## The DOM contract (planned)

| Id | Element | Contract |
| --- | --- | --- |
| `muse` | `section` or page root | |
| `muse--gate` | `div` | States what the upload is used for and that the photo never leaves the tab, before the press |
| `muse--upload` | `input type="file" accept="image/*"` | Selecting a file starts face detection and model load |
| `muse--preview` | `canvas` or `img` | The uploaded photo, shown while processing |
| `muse--stage` | `div` | Generated-avatar crossfade rendering, same structure as Draft's `RotatingAvatar` |
| `muse--status` | `p` | `role="status"`, `aria-live="polite"` |
| `muse--restart` | `button type="button"` | Clears the current photo/generation and returns to the upload gate |

None of these use the `disabled` property, for the same reason
`#iris--load` and `#ask--load` don't — model load and generation both take
multiple seconds, and disabling the control that started it strands a
keyboard user on `<body>` for that window. `aria-disabled` plus a
re-entry guard flag, same as the other three features.

---

## Acceptance criteria (planned)

| ID | Criterion | Evidence |
| --- | --- | --- |
| F06-AC01 | ONNX Runtime Web can run the `vae_encoder` graph from `schmuell/sd-turbo-ort-web` against an uploaded photo and produce a latent the UNet loop can partially noise and continue from. | **Unverified — see Pending.** Structural: a local prototype demonstrating one successful img2img pass, before any other AC is attempted |
| F06-AC02 | The stylization model's actual fetched size (not the 2.58GB unquantized repo total) is measured and judged acceptable for a one-time page load. | **Unverified — see Pending.** Measured: network panel transfer size for the exact files `js/muse.js` fetches |
| F06-AC03 | End-to-end generation (detect face → stylize → 5 poses) completes in a tolerable time on ordinary consumer hardware, not just a high-end discrete GPU. | **Unverified — see Pending.** Human, real hardware, real browser, timed |
| F06-AC04 | `js/muse.js` reuses `vendor/mediapipe/tasks-vision` and `assets/models/face-landmarker/face_landmarker.task` for face detection rather than fetching or vendoring a second face model. | Structural: diff against `git status` after implementation |
| F06-AC05 | The stylization model is fetched from an exact, pinned Hugging Face revision hash, not a branch reference. | Structural: `js/muse.js` source |
| F06-AC06 | `muse.html` ships a `Content-Security-Policy` naming Hugging Face's CDN host and nothing else new, and the network panel shows no request to any other third-party host. | Human, network panel inspection |
| F06-AC07 | The uploaded photo, the face landmarks, and every generated image are never transmitted anywhere; only the model weight files are fetched. | Human, network panel inspection during a full generation |
| F06-AC08 | None of `#muse--upload`, `#muse--restart` use the `disabled` property; focus survives file selection and model load. | Human, keyboard traversal |
| F06-AC09 | `muse.html` is listed in `sitemap.xml`. | `scripts/check_repo.py`, CI |
| F06-AC10 | Contrast holds at 4.5:1 for text in both flavours, checked against Latte. | Human, per colour scheme |

---

## Deferred

| Item | Note |
| --- | --- |
| Editable style prompt | Fixed prompts keep the generation surface bounded; a free-text field is a materially larger scope (arbitrary image content) not undertaken here. |
| Per-pose retry/regenerate | If one of the five generated poses looks wrong, v1 requires a full re-upload. A narrower per-pose retry is a plausible follow-up, not assumed. |
| Restoring Draft's four diagonal poses | Only worth doing if F06-AC03's real-hardware timing comes back comfortably under budget with five poses. |
| The small-vendorable-model fallback (AnimeGAN-class style transfer) | Only pursued if F06-AC01 or F06-AC02 come back unfavorable — see "Pending" above. Not designed here. |
