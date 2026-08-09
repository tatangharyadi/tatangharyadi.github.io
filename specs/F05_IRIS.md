# F05: Iris, a gaze-controlled Breakout

**Status:** implemented, iterating against real-hardware telemetry. The
gaze signal, calibration shape and DOM contract described below are all
current as of commit `2b104c5`; the sections under "Calibration" that
follow the current description are kept as a narrative record of every
signal and fix this feature tried and rejected before landing here — see
"How this got here" for the reading order.

The signal actually shipping is eye-corner-ratio geometry (`landmark 468`
against its own eye's `33`/`133` corners, `473` against `362`/`263`), head-
yaw-corrected using the same model's `outputFacialTransformationMatrixes`
output, smoothed with a median-then-EMA cascade, and mapped through a
five-point weighted-polynomial calibration fit (not the original two-point
linear map). This replaced an intermediate attempt that read the model's
`face_blendshapes.tflite` classifier instead of hand-rolled geometry — that
attempt is also in the record below, and also failed on real hardware.

Two real-hardware telemetry rounds since the corner-ratio-plus-calibration
signal landed have each found and fixed a distinct problem:

- **"The paddle barely moved"** (session 2, 2026-08-09): telemetry showed
  the paddle could reach both edges, but a full sweep took ~3 seconds of
  real gaze movement — too slow for gameplay. Root-caused to over-heavy
  raw-side smoothing (`RAW_GAZE_MEDIAN_WINDOW` was 5, `IRIS_X_EMA_ALPHA` was
  0.15) now that `PADDLE_EMA_ALPHA` exists as a dedicated post-calibration
  jitter absorber; fixed in commit `bf37945` (median window 5→3, EMA alpha
  0.15→0.3).
- **Erratic paddle swings from small gaze movements** (session 3, same
  day): a fresh telemetry pair showed the same ~3-second high-to-low swing
  time as before, but with the intermediate signal oscillating wildly
  rather than moving cleanly — traced to a calibration capture where two
  adjacent points (targets 0.5 and 0.75) read almost identically
  (`capturedRaw` differing by 0.0007), which `CAL_MIN_SEPARATION`'s
  overall-spread check doesn't catch, producing a degree-2 fit whose vertex
  sat inside the visitor's natural gaze range and amplified ordinary signal
  noise into full-track swings. Fixed in commit `2b104c5`: after fitting,
  check whether the vertex falls inside the captured raw range and, if so,
  refit at degree 1 instead of keeping the degenerate quadratic.

Neither fix has yet been confirmed against a fourth real-hardware session
played, not just measured — see "Pending" below. AC02 (real-camera gaze
signal stability and responsiveness), AC04 (the CSP-violation line firing
against a live camera session), AC06 (full keyboard traversal through a
real camera grant, including the calibration step) and AC08 (canvas text
contrast, which Lighthouse cannot read off canvas pixels) all need a human
with a webcam. `MIRROR_GAZE_X`, `HEAD_YAW_CORRECTION_GAIN`,
`IRIS_X_EMA_ALPHA`, `RAW_GAZE_MEDIAN_WINDOW`, `PADDLE_EMA_ALPHA` and
`CAL_MIN_SEPARATION` in `js/iris.js` are still first-guess constants (or,
for the first two rounds' worth, retuned first guesses) and should be
expected to change again as more sessions come in.

### Pending

A fourth real-hardware telemetry pair, played rather than only measured,
is needed to confirm both `bf37945` and `2b104c5` actually fixed what they
targeted — the analysis behind each was telemetry-only, not a person
reporting the paddle feels right.

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
                                                         outputFacialTransformationMatrixes: true })

five-point calibration (before gameplay, see "Calibration" below)
        │
   look at each of 5 targets in turn, press "Capture this point" → last
   CAPTURE_AVERAGE_WINDOW smoothed readings averaged into one raw sample per
   point → weighted least-squares polynomial (degree 2, edges weighted 2x)
   fit through the 5 (raw, target) pairs → degree-1 refit if the fitted
   curve's vertex falls inside the captured raw range (see "A second,
   distinct problem" below) → calCoeffs, or null if "Skip calibration"

every rendered frame
        │
   detectForVideo() → faceLandmarks[0] (478 points) +
   facialTransformationMatrixes[0] (head pose)
        │
   iris 468/473 position normalized against that eye's own 33/133, 362/263
   corner span (gazeScoreFromLandmarks) → corrected by head yaw extracted
   from the transformation matrix (HEAD_YAW_CORRECTION_GAIN) → rolling
   median (RAW_GAZE_MEDIAN_WINDOW) → EMA (IRIS_X_EMA_ALPHA) → calCoeffs
   polynomial remap, clamped to [0,1] → second EMA (PADDLE_EMA_ALPHA) →
   paddle x-position across the canvas width. See "How this got here" below
   for the blendshape-classifier signal this replaced.
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
- ~~**No calibration flow.**~~ Superseded: real-camera testing under F05-AC02
  showed a fixed mapping does need a per-visitor range. See "Calibration"
  below.
- **One difficulty curve, rule-based.** Whatever deterministic
  speed-up/brick-pattern progression Breakout traditionally uses; no
  jitter detection, no per-visitor tremor filtering. Iris is a game reusing
  a tracking signal, not a rehabilitation tool, and does not carry that
  scope.
- **No recording, no server-side anything.** Same structural privacy claim
  as Echo and Twin: the capture frame and the landmarks never cross a
  network boundary.

---

## Calibration

The current implementation, as of commit `2b104c5`. The gaze signal is
eye-corner-ratio geometry: each eye's iris landmark (468 right, 473 left)
normalized against that same eye's own corner-to-corner span (33/133,
362/263), which cancels head *translation* because both corners move with
the head by the same amount the iris does — see `gazeScoreFromLandmarks()`.
Head *rotation* (yaw) is not cancelled by the corner-ratio alone, so a
separate correction term, read from the same model's
`facialTransformationMatrixes` output, is subtracted before the signal is
used at all: `HEAD_YAW_CORRECTION_GAIN`. This combined value is
`smoothedGazeX`'s input.

A comfortable eye-movement range rarely spans the full 0..1 the corner-
ratio can theoretically reach, and that range differs by visitor (eye
shape, camera angle, glasses) and isn't necessarily linear across it — a
fixed or two-point linear map leaves part of the track unreachable, or
uneven, for most people. `CAL_POINTS` defines five targets (0, 0.25, 0.5,
0.75, 1 across the track); the visitor looks at each in turn and presses
"Capture this point". `captureCalPoint()` averages the last
`CAPTURE_AVERAGE_WINDOW` smoothed readings as that point's raw sample
(rather than a single instantaneous read, which is exactly the kind of
value transient noise shows up in), then — once all five are in —
`fitPolynomial()` fits a degree-`CAL_POLY_DEGREE` (2) weighted least-
squares curve through the five (raw, target) pairs, with the two edge
points weighted `CAL_EDGE_WEIGHT` (2x) against the three interior points'
`CAL_INTERIOR_WEIGHT` (1x) — favoring getting the paddle's full left/right
reach right over interior smoothness, since running out of track before
the visitor's own comfortable range does is the more visible failure.
`calibratedX()` evaluates this fit and clamps to `[0,1]`; a null
`calCoeffs` (calibration skipped or rejected) falls back to passing the raw
signal through unchanged.

Two validity guards run after all five captures, both falling back rather
than shipping a fit expected to misbehave:

- `CAL_MIN_SEPARATION` (0.03) rejects the whole capture if the *overall*
  spread of the five raw values — `max - min` across all of them — is
  below it: a static/synthetic camera, or a visitor whose gaze genuinely
  isn't moving the signal, produces this by construction. `calCoeffs` is
  set to `null` and the session plays uncalibrated.
- The vertex check (added in `2b104c5`, see "A second, distinct problem"
  below) computes the fitted degree-2 curve's vertex (`-c1/(2c2)`) and, if
  it falls *inside* the five captured raw values' range, refits at degree 1
  instead of discarding the fit outright — a straight line can't produce a
  vertex at all, so it can't have this problem.

Neither guard requires a monotonic or evenly-spaced capture; both are
narrower checks aimed at specific failure shapes real telemetry has
actually produced, not a general well-conditioned-ness test. A capture that
fails neither guard but is still poorly conditioned in some other way is a
plausible future finding, not a case already covered.

A "Skip calibration" control (`#iris--cal-skip`) leaves `calCoeffs` at
`null` and plays with the raw signal directly, for a visitor who would
rather not do the capture step. A "Recalibrate" control (`#iris--recalibrate`)
in the game view re-enters the same five-point step without re-requesting
the camera or reloading the model — only the fit needs to change.

The camera/model lifecycle is shared across calibration and gameplay: the
capture rAF loop keeps calling `detectForVideo()` and updating
`smoothedGazeX` throughout, and a small `aria-hidden` marker gives sighted
visitors live visual feedback of the current (uncalibrated) position while
capturing — deliberately not a live text region, since an `aria-live`
announcement on every animation frame would be unusable noise for a screen
reader user. The status text before and after each capture is the
non-visual equivalent.

### How this got here

Everything from here to "The vendored bundle phones home" is a narrative
record, in order, of every gaze signal and calibration shape this feature
tried before landing on the one described above — kept because each dead
end ruled something out that the next attempt needed to know, not because
any of it is still shipping. The two-point calibration and corner-ratio
signal this section starts from were both later superseded (five-point
weighted polynomial; head-yaw-corrected corner ratio, after an intervening
blendshapes attempt also failed) — see "Calibration" above for what
actually ships.

### The corner-relative fix traded head-tracking for noise amplification

The same real-camera pass that confirmed the head-tracking fix above also
found a second, worse problem: with the eyes held still, the paddle
twitched left and right on its own. The corner-relative ratio divides by
the eye's own corner-to-corner span, which is a small number — it is the
width of one eye in frame-normalized coordinates, not the width of the
frame. Landmark detection carries some amount of per-frame noise
regardless of what it's measuring, and dividing that noise by a small span
scales it up by roughly the inverse of the span. Noise that was negligible
against the old, frame-absolute signal became visible against this one,
and the EMA alone (`IRIS_X_EMA_ALPHA` was 0.25) wasn't a low-enough pass
filter to remove it without adding unacceptable lag.

Three changes address this, all applied before the EMA rather than by
lowering the EMA alone:

- `MIN_EYE_SPAN` (0.02) discards a per-eye reading outright when that eye's
  corner span is below it, instead of trusting a ratio computed from a
  near-zero denominator. `eyeRatio` now returns `null` in that case, and
  `updateGazeFromLandmarks` drops it from the average; if both eyes are
  unreliable in a given frame, the frame is skipped and `smoothedGazeX`
  holds its last value rather than jumping to a guess.
- A rolling median over the last `RAW_GAZE_MEDIAN_WINDOW` (5) raw combined-
  eye readings runs before the EMA. A median rejects single-frame outlier
  spikes outright, which an EMA can only ever attenuate — this is why the
  two are combined instead of relying on either alone.
- `IRIS_X_EMA_ALPHA` is lowered from 0.25 to 0.15 as an additional, smaller
  measure on top of the two above.

**This mitigation did not fix it.** A second real-camera pass reported the
same class of failure as before: the paddle still moves on its own while
the eyes hold still. That rules out "the filter just needs to be a little
stronger" as the whole story — either the noise-amplification diagnosis was
right but undersized (a bigger median window or lower alpha would still be
guessing at a magnitude), or there's a mechanism the filter can't reach at
all. Two candidates neither commit above has ruled out:

- `MIN_EYE_SPAN`'s guard is itself a discontinuity source. If one eye's
  span hovers near the 0.02 threshold, the guard flickers between using
  both eyes' average and one eye alone every few frames. If the two eyes
  don't share the same baseline ratio — plausible for any camera that
  isn't perfectly centred on the face — every flicker is a *step*, not
  noise, and a median filter does not touch steps it can't distinguish
  from a real value.
- A blink. At the camera's frame rate a blink is several consecutive
  frames of degraded or extrapolated iris detection, comfortably longer
  than `RAW_GAZE_MEDIAN_WINDOW` (5) can absorb, and it happens with the
  eyes not "moving" in the sense a visitor means by that word.

### Diagnostic instrumentation added instead of a third blind guess

Two consecutive fixes, each a plausible theory tuned against no real data,
have both failed on the only hardware that can judge them. A third constant
change would be the same bet again. Instead, `js/iris.js` now exposes the
raw pipeline: appending `?debug=1` to `iris.html`'s URL unhides
`#iris--debug`, a `<pre>` below the canvas showing, every frame, both eyes'
ratio and corner span, the pre-median raw combined reading, the post-median
value, the post-EMA `smoothedGazeX`, and a running count of frames where
both eyes were dropped as unreliable. `renderDebug()` in `js/iris.js` is the
whole of it — no state that survives past the page, no colour (so it is not
a seventh place `scripts/check_palette.py` has to track), hidden and
`aria-hidden` by default so it changes nothing for anyone not testing this.

This is deliberately not another fix. The sandbox's fake camera cannot
exercise it meaningfully — a feed that never moves produces the same flat
numbers whether or not the pipeline is healthy — so what this needs next is
a real webcam session reading the on-screen numbers while staring at a
fixed point (does the smoothed value wander, and does `dropped frames`
climb?) and while looking hard left/right and holding (do the two eyes'
ratios move together, or does one diverge, and does the smoothed value's
range across that motion comfortably exceed the wander seen while
motionless?). Whatever those numbers show is what should drive the next
change to `MIN_EYE_SPAN`, `RAW_GAZE_MEDIAN_WINDOW`, `IRIS_X_EMA_ALPHA`, or a
different mechanism entirely — not another guess.

### The numbers point away from the raw pipeline and at the calibration map

A real session sent back two `?debug=1` readings, both captioned "eyes
still": `smoothedGazeX` read 0.480 then 0.483, a delta of 0.003 — about one
pixel of paddle travel on a 390px track. `dropped frames` stayed at 0 in
both, ruling out the guard-flicker and blink theories above outright: there
was no discontinuity to distinguish from noise, because there was no visible
discontinuity in `smoothedGazeX` at all. The raw per-eye ratios wandered
more (right ratio 0.479→0.490, left 0.572→0.548), but that is exactly what
the median and EMA stages exist to absorb, and the numbers show them doing
it.

So two fixes were aimed at a stage that, on this evidence, was never the
problem. The one stage `renderDebug()` didn't expose is `calibratedX()`:

```js
const t = (x - calMin) / (calMax - calMin);
```

`CAL_MIN_SEPARATION` only requires `calMax - calMin` to exceed 0.03 for a
capture to be accepted. A visitor whose two calibration presses landed at,
say, 0.04 apart has a gain of `1 / 0.04 = 25x` between `smoothedGazeX` and
the paddle fraction. The 0.003 wander measured above, at that gain, is 0.075
of the track — visible, constant, uncorrelated with anything the visitor's
eyes are doing, and reproduced by a signal that the same numbers show is
otherwise nearly still. This would explain why both prior fixes — each
aimed upstream of `calibratedX()` — changed nothing a visitor could see.

`renderDebug()` now also prints `calMin`, `calMax`, their difference, the
resulting gain, and the final `paddle fraction` (`calibratedX(smoothedGazeX)`
itself), so the next real session can confirm or kill this directly instead
of us reasoning about it from `CAL_MIN_SEPARATION`'s value alone. Two
numbers settle it: `calMax - calMin` from that session's own calibration,
and whether `paddle fraction` swings by roughly `gain × 0.003` while the
eyes are still. If confirmed, the fix is raising `CAL_MIN_SEPARATION` (to
bound gain, at the cost of forcing a visitor with a genuinely narrow range
to use "Skip calibration" instead) — not another change to the filtering
constants above, which the evidence so far says were never the amplifier.

**This theory did not get the chance to be confirmed or killed on its own
terms**, because the next real session found something upstream of it: the
calibration marker (driven by `smoothedGazeX`, the same signal
`calibratedX()` divides by its own range) did not move at all when the
visitor looked left or right, before any calibration capture had happened.
Asked to hold a hard left gaze and then a hard right gaze on the
`?debug=1` screen with no calibration involved, the report back was "it
does not moved" — no difference in the raw or smoothed reading between the
two extremes. A signal with no measurable range between the two gaze
positions it exists to distinguish cannot be fixed by any calibration
divisor, however it's tuned: `calibratedX()` amplifies whatever range
`smoothedGazeX` has, and on this hardware that range was, to observable
precision, zero. Every fix up to this point — corner-relative
normalization, the noise filters above, and the calibration-gain theory in
this section — was tuning or reasoning about a signal that never carried
the information those fixes assumed it did.

### The raw signal itself had no dynamic range

The eye-corner-ratio approach (iris x-position normalized against that same
eye's own corner span, described under "Calibration" above) turned out to
be the wrong signal to extract from the 478 face-mesh points on this
visitor's hardware, not merely a noisy or unnormalized one. No amount of
median filtering, EMA tuning, or calibration-range rescaling can recover
gaze information from a signal that does not carry it in the first place.

The fix is a different signal already present in the same committed asset.
`assets/models/face-landmarker/face_landmarker.task` bundles four
sub-models (confirmed with `unzip -l` on the committed file):
`face_detector.tflite`, `face_landmarks_detector.tflite`,
`geometry_pipeline_metadata_landmarks.binarypb`, and
`face_blendshapes.tflite`. The last of these is a classifier — trained to
score ARKit-standard expression categories, confirmed present via `strings`
on the extracted `.tflite` — and among those categories are
`eyeLookInLeft`, `eyeLookInRight`, `eyeLookOutLeft`, and `eyeLookOutRight`:
a purpose-built gaze-direction estimate, not geometry derived by hand from
landmark positions. It ships inside the model Twin already loads; Twin
simply never asks for it, so enabling it costs no new download, just
`outputFaceBlendshapes: true` on the existing `createFromOptions()` call.

`gazeScoreFromBlendshapes()` in `js/iris.js` reads it. Looking to one side
is conjugate gaze — each eye rotates a different way relative to its own
nasal/temporal axis, not the same way — so looking right registers as the
right eye's `eyeLookOutRight` (rotating toward the temple) together with
the left eye's `eyeLookInLeft` (rotating toward the nose), and looking left
is the mirror pair (`eyeLookInRight` + `eyeLookOutLeft`). Averaging each
pair rather than trusting one eye halves the effect of either eye's score
being noisier on a given frame. The result centres on 0.5 (straight ahead)
and moves toward 1 or 0 as one side's pair outscores the other's, then
passes through the same median-then-EMA smoothing the corner-ratio signal
used (`RAW_GAZE_MEDIAN_WINDOW`, `IRIS_X_EMA_ALPHA` — unchanged), and the
same `calibratedX()` remap.

The one thing this diagnosis does not yet have is a name for whether the
old corner-ratio signal failed for this visitor specifically (an eye shape,
camera angle, or calibration quirk the geometry couldn't handle) or fails
in general — the evidence is one visitor's hardware, not a survey. The
blendshapes classifier being purpose-built for exactly this estimate,
rather than a byproduct of landmark geometry not designed for it, is the
reason to expect it generalizes better, not proof that it does.

`MIRROR_GAZE_X` also flips from `true` to `false` with this change. The old
signal used raw `landmark.x`, which is camera-frame-relative and needed the
flip the corner-ratio code applied to read as visitor-relative. The
blendshape category names are already visitor-relative — "Right" means the
visitor's own right eye regardless of camera orientation — so
`gazeScoreFromBlendshapes()` resolves directly to "higher score = visitor
looked to their own right" with no flip needed. This is reasoned from the
category semantics, not measured on hardware; if a real session finds the
paddle now moves backwards, `MIRROR_GAZE_X` is the one constant to flip
before re-deriving anything else.

**This rewrite is unverified on real hardware.** It has been checked
structurally — `node --check`, the CI scripts, and a chrome-devtools MCP
session against the sandbox's fake camera confirming the model loads with
`outputFaceBlendshapes: true` and `#iris--debug` populates with real
category scores and no console errors — but the sandbox's camera feed
never moves, so nothing here has exercised whether the new signal actually
carries gaze information on a real visitor's hardware. The next real
session should repeat exactly the test that killed the old signal, before
touching calibration at all: open `?debug=1`, hold a hard left gaze, hold a
hard right gaze, and check whether `raw`/`smoothed` move between the two.
`renderDebug()` now prints the four blendshape scores and the two averaged
per-side values alongside `raw`/`median`/`smoothed`, so that comparison
doesn't need guessing at.

### The blendshapes classifier failed on real hardware too

A real-hardware telemetry file (`iris-calibration-*.json`, 2026-08-09)
answered the question the previous section left open, and answered it
against the blendshapes signal, not for it: `eyeLookOutRight` and
`eyeLookInLeft` stayed flat across all five calibration points, and
`eyeLookInRight`/`eyeLookOutLeft` moved, but in the wrong direction
relative to the target. A classifier purpose-built for this estimate was
still, on this hardware, not tracking gaze direction.

`gazeScoreFromLandmarks()` returns to the corner-ratio geometry this
section started from, this time with `HEAD_YAW_CORRECTION_GAIN` applied —
the ingredient the three earlier corner-ratio rounds never had, and a
plausible reason the very first one showed no usable range at all: a head
turn shifts an eye's corners and its iris by different amounts under
perspective, which corner-ratio math alone has no way to distinguish from
an actual gaze shift. `MIRROR_GAZE_X` flips back to `false`, since the
corner-ratio signal is camera-frame-relative again, not the blendshape
categories' visitor-relative naming.

At the same time, the original two-point linear calibration is replaced by
the five-point weighted polynomial fit described under "Calibration"
above, porting two techniques ("Head-pose correction and multi-point
calibration") from public MediaPipe-based gaze trackers
(`aciderix/React-Eye-Tracker-V1`, `ChiShengChen/gaze_track_webcam`) that
report working accuracy on the same `FaceLandmarker` API. Both were
first-guess ports pending a real-hardware session of their own — not
verified by anything about *why* they generalize, just by matching a shape
that reportedly works elsewhere.

### "The paddle barely moved" — a real latency problem, found by asking the wrong question first

The first real-hardware session against the corner-ratio-plus-calibration
signal reported "fitted" calibration with real, distinct coefficients and
still summarized as "barely moved". Telemetry alone looked like a
validated fix at first pass — the play session's `targetFraction` reached
both 0 and 1 with no visible jitter — and it took the visitor's direct,
live contradiction of that reading ("the paddle barely moved") to send the
analysis back to the same data asking a different question: not *does the
signal reach the full range*, but *how long does a sweep across it take*.
It took roughly 3 seconds of real gaze movement to go from one extreme to
the other — numerically full-range, but far slower than a dodge in actual
gameplay needs.

The cause was excessive latency stacked in the raw-side smoothing cascade,
sized for a problem (per-frame landmark noise) a later stage now also
absorbs: `PADDLE_EMA_ALPHA`, added to damp the calibration fit's own noise
amplification (see its own comment in `js/iris.js`), sits downstream of
`RAW_GAZE_MEDIAN_WINDOW` and `IRIS_X_EMA_ALPHA` and does the same kind of
smoothing job. With a dedicated stage already absorbing jitter after
calibration, the raw-side stages no longer needed to be tuned as if they
were the only line of defense against it. Commit `bf37945` cut
`RAW_GAZE_MEDIAN_WINDOW` from 5 to 3 and raised `IRIS_X_EMA_ALPHA` from
0.15 to 0.3, trading some of the raw signal's own noise rejection for
lower latency, on the reasoning that `PADDLE_EMA_ALPHA` can absorb what
that trade reintroduces.

### A second, distinct problem: erratic swings from a poorly-conditioned fit

A fresh telemetry pair requested to validate `bf37945` showed the same
~3-second high-to-low swing time as before — on its own, indistinguishable
from "the latency fix didn't work". But this swing was oscillatory rather
than a clean sweep: `smoothedGazeX` drifted gently and monotonically across
the whole window while `targetFraction` (the post-calibration value)
whipsawed between roughly 0.08 and 0.90 several times within it. A timing
metric that looks the same can mean two structurally different things, and
only inspecting the intermediate signal shape — not just start and end —
told them apart.

The calibration capture behind this session had two adjacent points
(targets 0.5 and 0.75) whose `capturedRaw` values differed by only 0.0007,
against jumps of 0.0125–0.0522 between every other pair — plausibly a
visitor undershooting a small angular target ("a quarter of the way from
the right") rather than a fluke, and a shape worth expecting to recur, not
a one-off bad capture. `CAL_MIN_SEPARATION` checks only the overall spread
across all five points, which this capture still cleared (0.0799, larger
than a prior session's passing capture). The resulting degree-2 fit had a
vertex at raw x ≈ 0.538, sitting inside this visitor's observed gameplay
range (0.452–0.550) — meaning the fitted curve was nearly flat (paddle
pinned near an edge) through much of that range and very steep just below
it, so ordinary frame-to-frame signal noise near the steep zone was
amplified into large `targetFraction` swings. This is a different failure
from `bf37945`'s target: reachable range and responsiveness were both
fine; the *mapping* was pathological in a way neither prior guard caught.

Commit `2b104c5` added the vertex check described under "Calibration"
above: if the fitted degree-2 curve's vertex falls inside the captured raw
range, refit at degree 1. Verified against both this session's raw values
(vertex 0.538, inside its range [0.464, 0.544] → refit fires, producing a
monotonic map) and a prior session's (vertex ≈0.542, outside its range
[0.480, 0.526] → unaffected), by replicating `fitPolynomial()` outside the
browser against both sessions' actual telemetry.

Neither `bf37945` nor `2b104c5` has yet been confirmed by a person playing
the game and reporting on it, only by re-deriving properties of past
telemetry — see "Pending" at the top of this document.

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
| `iris--calibrate` | `div` | Five-point calibration step, shown after load succeeds and before gameplay |
| `iris--cal-target` | `div` | `aria-hidden` marker for where to look, positioned per step by `showCalStep()` |
| `iris--cal-marker` | `div` | `aria-hidden` live visual readout of the current raw gaze position, decorative only |
| `iris--cal-capture` | `button type="button"` | Captures the current gaze position as the current step's sample and advances to the next; on the fifth capture, fits (or falls back — see "Calibration" above) and starts the game |
| `iris--cal-skip` | `button type="button"` | Starts the game with no calibration fit (raw signal used directly) |
| `iris--stage` | `canvas` | Paddle/ball/brick rendering |
| `iris--debug` | `pre` | `hidden` and `aria-hidden` unless the URL carries `?debug=1`; raw per-eye gaze diagnostics, see "Diagnostic instrumentation" above |
| `iris--status` | `p` | `role="status"`, `aria-live="polite"` |
| `iris--recalibrate` | `button type="button"` | Re-enters the calibration step without releasing the camera or reloading the model |
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
| F05-AC02 | The head-yaw-corrected eye-corner-relative gaze signal, run through calibration, is stable enough frame-to-frame to drive continuous paddle movement without a dwell timer, tracks eye movement independent of head position, and responds to a gaze shift within roughly a second rather than several (the original mean-landmark-x version and an intervening blendshapes-classifier version both failed this — see "How this got here"). | Human, real camera, real browser, actually played rather than only measured from telemetry — see "Pending" at the top of this document |
| F05-AC03 | No new file is vendored under `vendor/` or `assets/models/`; `js/iris.js` loads the same `face_landmarker.task` already committed for Twin. | Structural: diff against `git status` after implementation |
| F05-AC04 | The video frame and landmarks are never transmitted anywhere, including the same `odml.pa.googleapis.com` telemetry call the vendored `@mediapipe/tasks-vision` bundle makes unconditionally after ~30s of `detectForVideo()` (see "The vendored bundle phones home" above). `iris.html` ships the same `<meta http-equiv="Content-Security-Policy" content="connect-src 'self' blob:">` tag `twin.html` uses — Iris calls `detectForVideo()` every frame for the whole game, strictly longer exposure than Twin's calibration window, so this is required, not conditional. | Verify a CSP-violation console line for the blocked request and no successful third-party request in the network panel, same method as F04-AC03 |
| F05-AC05 | Spoken lines are read from a small authored, fixed set, never generated at runtime. | Structural: `js/iris.js` |
| F05-AC06 | None of `#iris--load`, `#iris--stop` use the `disabled` property; focus survives camera grant and load. | Human, keyboard traversal |
| F05-AC07 | `iris.html` is listed in `sitemap.xml`. | `scripts/check_repo.py`, CI |
| F05-AC08 | Contrast holds at 4.5:1 for text in both flavours, checked against Latte. | Human, per colour scheme |
| F05-AC09 | The five-point calibration step is reachable and completable by keyboard alone via repeated presses of `iris--cal-capture`, and "Skip calibration" and "Recalibrate" both leave the page in a state a keyboard user can continue from without a focus loss to `<body>`. | Human, keyboard traversal, real camera |

---

## Deferred

| Item | Note |
| --- | --- |
| A launch/action gesture beyond continuous paddle position | Only needed if playtesting shows Breakout's classic serve-the-ball moment needs a discrete trigger; not assumed here. |
| Reusing this same iris signal for a different classic game (Pong, considered and set aside earlier in this feature's design discussion) | Breakout was chosen because it needs only a continuous 1D position and no second discrete input; Pong fits the same constraints but offers less content/progression. Not pursued unless Breakout's scope turns out too small. |
