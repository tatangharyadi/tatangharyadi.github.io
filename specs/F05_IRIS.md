# F05: Iris, a gaze-controlled Breakout

**Status:** implemented, pending human verification. `iris.html`,
`css/iris.css` and `js/iris.js` are written and pass every check this repo
can run without camera hardware (CI-mirroring scripts, a headless browser
load with a clean console and the expected accessibility tree). F05-AC02
(real-camera iris landmark stability), AC04 (the CSP-violation line firing
against a live camera session), AC06 (full keyboard traversal through a
real camera grant) and AC08 (canvas text contrast, which Lighthouse cannot
read off canvas pixels) all need a human with a webcam and have not been
checked. `MIRROR_GAZE_X` and `IRIS_X_EMA_ALPHA` in `js/iris.js` are
first-guess constants pending that pass and should be expected to change.

The premise that the iris landmarks exist at all is no longer assumed: the
committed `assets/models/face-landmarker/face_landmarker.task`'s
`face_landmarks_detector.tflite` was extracted and its output tensor
inspected directly (`Interpreter.get_output_details()`), returning shape
`[1, 1, 1, 1434]` — 1434 = 478 × 3 coordinates, confirming 478 landmarks
(468 face-mesh points plus the 10 iris points at indices 468–477) come out
of this exact committed model file, independent of any runtime flag. See
F05-AC01.

**Before any of this is built**, confirm F05-AC02 below: that those iris
landmarks are stable enough, frame to frame, to drive a paddle without the
dwell/blink-style discretization other gaze-controlled games reach for.
Everything past that point assumes plain per-frame iris position, smoothed,
is enough.

---

## Overview

Iris is a Breakout-style game: bricks, a ball, a paddle. The paddle's
horizontal position tracks the visitor's gaze, read from iris landmark
position rather than a mouse or arrow keys. A short, rule-based line of
spoken encouragement — not a model, not a chat — plays on level-clear,
game-over, and long rallies, using the browser's own `speechSynthesis`.

This is **not** a puppeted character or a digital twin. It reuses a
tracking *signal* — iris position — the way Echo reuses BlazePose's body
landmarks to drive a rig, but there is no rig, no mesh deform, no capture,
and no download here. The visitor never sees their own face; they see a
ball and bricks respond to where they're looking.

### Why this reuses Twin's runtime instead of Echo's, and adds no third

`js/mocap.js` calls LiteRT.js directly for BlazePose's 33 *body* landmarks
— no face model is loaded at all, so there is nothing in Echo's pipeline
resembling gaze. `js/twin.js` already vendors `@mediapipe/tasks-vision` and
calls `FaceLandmarker.createFromOptions()`; the Tasks Vision face model
unconditionally returns 478 landmarks per detection — 468 face-mesh points
plus 10 iris points at indices 468–477 — regardless of the
`outputFaceBlendshapes` / `outputFacialTransformationMatrixes` flags, which
only gate *additional* outputs, not the base landmark set. Twin's own code
never reads indices 468–477; it only uses the 468 mesh points to deform the
canonical face.

Iris needs exactly the ten landmarks Twin already receives and discards. So
this spec reuses Twin's vendored runtime and boundary shape
(`FilesetResolver.forVisionTasks()` → `FaceLandmarker.createFromOptions()`
→ `detectForVideo()` in a loop) rather than vendoring a second face model or
falling back to Echo's body-only LiteRT pipeline, which cannot produce this
signal at all. `js/twin.js` and a prospective `js/iris.js` would therefore
both load the same `face_landmarker.task` already committed for Twin — no
new model file, no new vendored runtime — but remain two separate files:
Twin deforms a mesh from 468 points and exports a `.glb`; Iris reads 10
points and moves a paddle. Folding gaze-controlled gameplay into
`js/twin.js` would mean a face-copying feature and a game sharing one file
for no reason beyond both starting from the same detector call, which is
the same "argued from scratch, not from precedent" standard `AGENTS.md`
already holds Twin to against Echo.

### TTS needs no new vendoring at all

Unlike the face model, speech needs nothing committed to the repo. The
browser's own `speechSynthesis.speak()` and `speechSynthesis.getVoices()`
use whatever voices the visitor's OS/browser ships — no model download, no
network fetch, no vendored runtime. This is a structurally different
situation from Ask's `onnxruntime` weights or Twin's `face_landmarker.task`:
TTS is available or it silently isn't (a device with no installed voices
simply produces no audio), and either way nothing here can grow the
repository's asset footprint.

### Where a visitor reaches it

**Resolved 2026-08-09.** Iris takes a masthead door outright rather than a
sub-link from another page. The job title in `index.html`'s masthead —
"Chief Technology Officer" — was already two doors, one word each:
"Technology" opened `game.html` and "Officer" opened `mocap.html`. That job
title now reads "Technology" → `iris.html` and "Officer" → `game.html`;
`mocap.html` gives up its masthead door to Iris. (An earlier same-day draft
put an "Also try Iris" footnote on `game.html` instead; reusing an existing
masthead door reads as more consistent with the site's established
two-doors pattern than adding a footnote to a page with no door of its
own.)

`mocap.html` does not lose reachability, it loses its *masthead* door.
Losing the masthead door leaves it exactly as reachable as `twin.html`
already was — sitemap, hover, tab order — plus a same-page cross-link:
`mocap.html` and `iris.html` each carry an "Also try" paragraph pointing at
the other, the same framing `mocap.html` already used for `twin.html`.
`game.html` and `iris.html` deliberately do **not** cross-link each other:
each now has its own masthead door, so a footnote pointing from one to the
other would just be a second route to a page reachable in one hop already.

---

## Key files

| File | Role |
| --- | --- |
| `iris.html` | The page: camera gate, canvas, paddle/ball/brick rendering, score/status region, and the `connect-src 'self' blob:` CSP meta tag (see "The vendored bundle phones home" below) |
| `css/iris.css` | Stage/gate layout. No colour of its own beyond the shared palette. |
| `js/iris.js` | Camera acquisition, Tasks Vision load (reusing Twin's `FaceLandmarker` boundary shape), iris-position smoothing, game loop, `speechSynthesis` calls |

No new file under `assets/models/` or `vendor/` — both are already committed
for Twin and reused as-is.

---

## Architecture

```
visitor presses #iris--load
        │
        ├─ getUserMedia({video: true})
        └─ FilesetResolver.forVisionTasks('vendor/mediapipe/tasks-vision/wasm')
                 └─ FaceLandmarker.createFromOptions({ modelAssetPath: FACE_MODEL_URL,
                                                         numFaces: 1,
                                                         outputFaceBlendshapes: false,
                                                         outputFacialTransformationMatrixes: false })

every rendered frame
        │
   detectForVideo() → 478 landmarks
        │
   landmarks[468..477] (iris ring, both eyes) → mean x → EMA-smoothed →
   mapped to paddle x-position across the canvas width
        │
   game loop: ball/brick/paddle collision, scoring, deterministic difficulty
   (same "rule-based, not model-based" shape as Echo's retargeting math)
        │
   on level-clear / game-over / N-hit streak: a short, fixed phrase (or one
   picked from a small fixed set, not generated) is spoken via
   speechSynthesis.speak(), voice chosen by a scoring heuristic over
   speechSynthesis.getVoices() — the phrases themselves are authored, not
   produced at runtime, and nothing here is LLM-generated
```

No dwell timer and no blink detector: Iris only needs a continuous position,
the same property that made Breakout the right classic-game fit for this
input in the first place — it does not need a second, discrete gaze action
for anything.

---

## Scope cuts

- **No blink-to-act or dwell-to-act input.** Paddle position is the only
  signal read from gaze; there is no secondary action a visitor needs to
  time. If this spec's design later needs one (a "launch ball" gesture,
  say), it is a new acceptance criterion, not an assumed extension of this
  one.
- **No LLM, no adaptive coaching, no generated dialogue.** All spoken lines
  are authored text, picked from a small fixed set by rule, not produced at
  runtime.
- **No calibration flow.** Twin's front-only capture calibrates a face mesh
  against a canonical topology; Iris only needs a relative left-right
  signal for a paddle, not an absolute position, so no calibration step is
  planned. If early testing shows raw iris x needs a per-visitor offset or
  gain, that is a new acceptance criterion, not assumed here.
- **One difficulty curve, rule-based.** Whatever deterministic
  speed-up/brick-pattern progression Breakout traditionally uses; no
  jitter detection, no per-visitor tremor filtering. Iris is a game reusing
  a tracking signal, not a rehabilitation tool, and does not carry that
  scope.
- **No recording, no server-side anything.** Same structural privacy claim
  as Echo and Twin: the capture frame and the landmarks never cross a
  network boundary.

---

## The vendored bundle phones home, and Iris inherits the mitigation

Reusing Twin's runtime means reusing its risk, not just its capability.
`vendor/mediapipe/tasks-vision/vision_bundle.mjs` contains a usage-telemetry
client instantiated unconditionally inside `createFromOptions()` — the same
call both `js/twin.js` and a prospective `js/iris.js` make. That client
starts a 60-second flush interval on creation and, roughly every 30 seconds
of wall-clock time since the last flush, the next `detectForVideo()` call
queues a POST to `https://odml.pa.googleapis.com/v1/log`. There is no
consumer-facing option to disable it (see `F04_TWIN.md`'s "The vendored
bundle phones home, and the mitigation" for the full trace, kept there as
the canonical write-up while `js/twin.js` still ships).

Twin only reaches the 30-second threshold during its calibration window.
Iris calls `detectForVideo()` every rendered frame for as long as a game
runs — a strictly longer, harder-to-avoid exposure than Twin's. So
`iris.html` needs the same mitigation Twin needed, more certainly than Twin
did: a page-level `<meta http-equiv="Content-Security-Policy"
content="connect-src 'self' blob:">` tag, which makes the browser refuse the
`fetch()` before it leaves the tab. `'self'` covers the same-origin
model/WASM fetches `FilesetResolver` and `FaceLandmarker.createFromOptions()`
need; the omission of `odml.pa.googleapis.com` is what turns F05-AC04 into a
claim the browser enforces, not one that depends on the vendored dependency
behaving. As with Twin, the blocked request logs a CSP violation to the
console on purpose — AGENTS.md's clean-console verification step treats this
one case as the expected, visible proof the block fired, not as a defect.

If Twin's files and `F04_TWIN.md` are deleted as part of Twin's retirement
before this section is next revised, the paragraph above — not the
cross-reference — is the part of the record that must survive that deletion.

---

## The DOM contract

Same shape as `#echo--*` and `#twin--*`: every id looked up once, at module
scope, in `js/iris.js`.

| Id | Element | Contract |
| --- | --- | --- |
| `iris` | `section` or page root | |
| `iris--gate` | `div` | States what camera access is used for and that nothing leaves the tab, before the press |
| `iris--load` | `button type="button"` | Requests the camera and starts loading the runtime and model |
| `iris--video` | `video` | `muted`, `playsinline` |
| `iris--stage` | `canvas` | Paddle/ball/brick rendering |
| `iris--status` | `p` | `role="status"`, `aria-live="polite"` |
| `iris--stop` | `button type="button"` | Releases the camera stream's tracks and stops the game loop |

None of these use the `disabled` property, for the same reason
`#echo--load`, `#twin--load`, and `#ask--load` don't: each sits on the page
across a multi-second permission prompt or model load, and disabling on
press strands a keyboard user on `<body>` for that window. `aria-disabled`
plus a re-entry guard flag, same as the other three features.

---

## Acceptance criteria

| ID | Criterion | Evidence |
| --- | --- | --- |
| F05-AC01 | The committed `face_landmarker.task`'s detector emits 478 landmarks (468 face mesh + 10 iris, indices 468–477), not 468. | **Verified 2026-08-09**: `face_landmarks_detector.tflite` extracted from the `.task` bundle and its output tensor inspected — shape `[1, 1, 1, 1434]`, 1434 = 478 × 3 |
| F05-AC02 | Iris landmarks (468–477) from Twin's already-vendored `FaceLandmarker` are stable enough frame-to-frame, after EMA smoothing, to drive continuous paddle movement without a dwell timer. | Human, real camera, real browser — **run this before anything else in this spec is built** |
| F05-AC03 | No new file is vendored under `vendor/` or `assets/models/`; `js/iris.js` loads the same `face_landmarker.task` already committed for Twin. | Structural: diff against `git status` after implementation |
| F05-AC04 | The video frame and landmarks are never transmitted anywhere, including the same `odml.pa.googleapis.com` telemetry call the vendored `@mediapipe/tasks-vision` bundle makes unconditionally after ~30s of `detectForVideo()` (see "The vendored bundle phones home" above). `iris.html` ships the same `<meta http-equiv="Content-Security-Policy" content="connect-src 'self' blob:">` tag `twin.html` uses — Iris calls `detectForVideo()` every frame for the whole game, strictly longer exposure than Twin's calibration window, so this is required, not conditional. | Verify a CSP-violation console line for the blocked request and no successful third-party request in the network panel, same method as F04-AC03 |
| F05-AC05 | Spoken lines are read from a small authored, fixed set, never generated at runtime. | Structural: `js/iris.js` |
| F05-AC06 | None of `#iris--load`, `#iris--stop` use the `disabled` property; focus survives camera grant and load. | Human, keyboard traversal |
| F05-AC07 | `iris.html` is listed in `sitemap.xml`. | `scripts/check_repo.py`, CI |
| F05-AC08 | Contrast holds at 4.5:1 for text in both flavours, checked against Latte. | Human, per colour scheme |

---

## Deferred

| Item | Note |
| --- | --- |
| A launch/action gesture beyond continuous paddle position | Only needed if playtesting shows Breakout's classic serve-the-ball moment needs a discrete trigger; not assumed here. |
| Per-visitor calibration or gain adjustment for iris x-position | Only added if F05-AC02's testing shows a fixed mapping is not usable across visitors. |
| Reusing this same iris signal for a different classic game (Pong, considered and set aside earlier in this feature's design discussion) | Breakout was chosen because it needs only a continuous 1D position and no second discrete input; Pong fits the same constraints but offers less content/progression. Not pursued unless Breakout's scope turns out too small. |
