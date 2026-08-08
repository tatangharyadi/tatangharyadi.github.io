# F05: Iris, a gaze-controlled Breakout

**Status:** implemented, pending human verification. `iris.html`,
`css/iris.css` and `js/iris.js` are written and pass every check this repo
can run without camera hardware (CI-mirroring scripts, a headless browser
load with a clean console and the expected accessibility tree). First
real-camera testing (2026-08-09) found the gaze signal was tracking head
position rather than eye movement; the fix (eye-corner-relative
normalization, see "Calibration" below) and a two-point calibration step
are both implemented. A second real-camera pass the same day found that fix
introduced a new problem — the paddle twitching on its own with the eyes
still, from landmark noise amplified by the corner-span division. A second
fix (a degenerate-span guard, a median pre-filter and a lower EMA alpha —
see "The corner-relative fix traded head-tracking for noise amplification"
below) is implemented but **not yet verified against real hardware at
all** — the fake camera this sandbox has access to cannot exhibit either
the head-tracking failure or the twitching, so neither fix has been
confirmed by anything other than a human eye on a webcam. AC02 (real-camera
iris landmark stability: tracks eye movement independent of head position,
and does not drift or twitch with the eyes still), AC04 (the CSP-violation
line firing against a live camera session), AC06 (full keyboard traversal
through a real camera grant, now including the calibration step) and AC08
(canvas text contrast, which Lighthouse cannot read off canvas pixels) all
need a human with a webcam and have not been checked. `MIRROR_GAZE_X`,
`IRIS_X_EMA_ALPHA`, `MIN_EYE_SPAN`, `RAW_GAZE_MEDIAN_WINDOW` and
`CAL_MIN_SEPARATION` in `js/iris.js` are first-guess constants pending that
pass and should be expected to change.

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
   each eye's iris centre (468, 473) normalized against that eye's own
   corner-to-corner span (33/133, 362/263) → averaged across both eyes →
   EMA-smoothed → remapped through the session's calibration range (if any)
   → mapped to paddle x-position across the canvas width
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

First real-camera testing of F05-AC02 (2026-08-09) found the original
mapping was tracking head position, not eye movement: `updateGazeFromLandmarks`
averaged the raw, frame-absolute x of the ten iris landmarks, and that value
moves just as much when the head translates left-right as when the eyes
move within their sockets — the two are not distinguishable from landmark
position alone. The fix normalizes each eye's iris centre (landmark 468 for
the right eye, 473 for the left) against that same eye's own corner-to-corner
span (33/133 and 362/263), which cancels head translation because both
corners move with the head by the same amount the iris does. This is now
the actual gaze signal `smoothedGazeX` carries, in `js/iris.js`.

Even head-translation-corrected, a comfortable eye-movement range rarely
spans the full 0..1 the corner-relative ratio can theoretically reach, and
that range differs by visitor (eye shape, camera angle, glasses). A fixed
linear map from that ratio to the paddle track therefore leaves part of
the track unreachable for most people — the scope cut above assumed a
relative signal wouldn't need this, and real testing showed otherwise.

The fix is a two-point calibration step inserted between camera+model load
and gameplay: look at the left edge, press "Capture left"; look at the
right edge, press "Capture right". The two raw `smoothedGazeX` readings
become `calMin`/`calMax`, and `stepGame` maps through that range instead of
the raw 0..1 signal directly. A "Skip calibration" control keeps the
original fixed mapping available for a visitor who would rather not do the
capture step, or whose setup makes it unnecessary. A "Recalibrate" control
in the game view re-enters the same step without re-requesting the camera
or reloading the model — only the mapping range needs to change.

`CAL_MIN_SEPARATION` (0.03) rejects a capture pair too close together to be
a deliberate left/right pair rather than the same point measured twice;
below it, "Start playing" stays `aria-disabled` and the visitor is asked to
try again or skip. This is also the shape a static/synthetic camera feed
hits directly — anything that never moves produces two nearly identical
captures and never satisfies the guard, which is expected: it does not
mean calibration is broken, only that there is no real head or eye motion
to calibrate against.

The camera/model lifecycle is shared across calibration and gameplay: the
capture rAF loop keeps calling `detectForVideo()` and updating
`smoothedGazeX` throughout, and a small `aria-hidden` marker gives sighted
visitors live visual feedback of the current (uncalibrated) position while
capturing — deliberately not a live text region, since an `aria-live`
announcement on every animation frame would be unusable noise for a screen
reader user. The status text before and after each capture is the
non-visual equivalent.

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
| `iris--calibrate` | `div` | Two-point calibration step, shown after load succeeds and before gameplay |
| `iris--cal-marker` | `div` | `aria-hidden` live visual readout of the current raw gaze position, decorative only |
| `iris--cal-left` | `button type="button"` | Captures the current gaze position as the left end of the range |
| `iris--cal-right` | `button type="button"` | Captures the current gaze position as the right end of the range |
| `iris--cal-start` | `button type="button"` | Commits the calibration range and starts the game; `aria-disabled` until both ends are captured with enough separation |
| `iris--cal-skip` | `button type="button"` | Starts the game with no calibration range (raw signal used directly) |
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
| F05-AC02 | The eye-corner-relative gaze signal is stable enough frame-to-frame, after EMA smoothing, to drive continuous paddle movement without a dwell timer, and tracks eye movement independent of head position (the original mean-landmark-x version failed this — see "Calibration"). | Human, real camera, real browser — **run this before anything else in this spec is built** |
| F05-AC03 | No new file is vendored under `vendor/` or `assets/models/`; `js/iris.js` loads the same `face_landmarker.task` already committed for Twin. | Structural: diff against `git status` after implementation |
| F05-AC04 | The video frame and landmarks are never transmitted anywhere, including the same `odml.pa.googleapis.com` telemetry call the vendored `@mediapipe/tasks-vision` bundle makes unconditionally after ~30s of `detectForVideo()` (see "The vendored bundle phones home" above). `iris.html` ships the same `<meta http-equiv="Content-Security-Policy" content="connect-src 'self' blob:">` tag `twin.html` uses — Iris calls `detectForVideo()` every frame for the whole game, strictly longer exposure than Twin's calibration window, so this is required, not conditional. | Verify a CSP-violation console line for the blocked request and no successful third-party request in the network panel, same method as F04-AC03 |
| F05-AC05 | Spoken lines are read from a small authored, fixed set, never generated at runtime. | Structural: `js/iris.js` |
| F05-AC06 | None of `#iris--load`, `#iris--stop` use the `disabled` property; focus survives camera grant and load. | Human, keyboard traversal |
| F05-AC07 | `iris.html` is listed in `sitemap.xml`. | `scripts/check_repo.py`, CI |
| F05-AC08 | Contrast holds at 4.5:1 for text in both flavours, checked against Latte. | Human, per colour scheme |
| F05-AC09 | The two-point calibration step is reachable and completable by keyboard alone, `iris--cal-start` stays `aria-disabled` (never `disabled`) until both ends are captured with enough separation, and "Skip calibration" and "Recalibrate" both leave the page in a state a keyboard user can continue from without a focus loss to `<body>`. | Human, keyboard traversal, real camera |

---

## Deferred

| Item | Note |
| --- | --- |
| A launch/action gesture beyond continuous paddle position | Only needed if playtesting shows Breakout's classic serve-the-ball moment needs a discrete trigger; not assumed here. |
| Reusing this same iris signal for a different classic game (Pong, considered and set aside earlier in this feature's design discussion) | Breakout was chosen because it needs only a continuous 1D position and no second discrete input; Pong fits the same constraints but offers less content/progression. Not pursued unless Breakout's scope turns out too small. |
