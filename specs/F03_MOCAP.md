# F03: Echo, a webcam puppeting a 3D character on device

**Status:** wired end to end and running — camera access, pose inference,
retargeting and rendering all confirmed executing without error, including a
stop/restart cycle, against `godette_rigged.glb`. **Not the same as
verified.** A real camera surfaced a confirmed, unresolved head-jitter
regression on this rig relative to the one it replaced — see the
`RobotExpressive.glb`/`godette_rigged.glb` caveats below — and that
regression is open, not fixed. Four real gaps were found and fixed before
this ever touched a real camera, each verified against the live code path
with synthetic landmarks: a mirrored L/R mapping (reverted to direct,
same-side landmarks), excess Neck pitch from monocular depth noise (fixed via
`flattenDepth`, see below), a static ~4°/~9° pitch/yaw offset baked into the
`Head` bone's own rest rotation in the GLB (fixed via `levelHead()`), and
head yaw being entirely unsupported because it is a twist and `retarget()`'s
swing-only math cannot produce one (added via `applyHeadYaw()`). Those four
fixes were verified against `RobotExpressive.glb`; the character swap to
`godette_rigged.glb` carried the same fixes forward on the assumption they
would still hold, and the real-camera jitter regression says that assumption
does not fully hold for at least the head.

A real camera then took five more passes to get that last one usable, each
exposing the next problem underneath the previous fix rather than a fresh
one, because all five live in the same signal: the difference between the
two ears' monocular depth estimates is small, and every stage of turning it
into a yaw angle had a different way of amplifying its noise instead of the
turn it was supposed to represent. In order: it snapped between left and
right with nothing in between and never returned to front (losing the far
ear's tracking mid-turn froze the last rotation instead of easing back, and
every frame copied its raw target with no damping — fixed with
fallback-to-front on lost tracking and a per-frame `slerp`); it then
defaulted to a left turn while facing the camera dead on (the raw signal
never actually read zero at rest for this camera/face — fixed by calibrating
an early reading as "forward" and measuring later frames as a delta from it,
`scratch.headYawBaseline`); it then overcorrected the *other* way (that
calibration had locked onto a single frame exactly as exposed to noise as
the signal it was meant to correct — fixed by averaging
`CALIBRATION_FRAMES`, 30, tracked frames before locking the baseline in);
it then oscillated left/right without ever settling (the raw signal's
sign was flipping frame to frame near zero, so the *target* handed to the
slerp flipped too, and a slerp chasing a reversing target cannot converge no
matter how slow it moves — fixed with an exponential filter,
`DEPTH_DIFF_FILTER_ALPHA`, smoothing the signal itself before the deadzone
and sign check ever see it); and it then read as a strong asymmetry between
directions — one turn reaching the ±45° clamp, a comparably deliberate turn
the other way barely clearing the deadzone. That was not a per-ear bias: the
locked baseline had drifted away from where `depthDiff` actually rested
partway into the session (auto-exposure, the subject shifting in frame,
BlazePose's own temporal smoothing settling further), and re-centering both
readings on the depthDiff observed at rest during that session, rather than
on the value locked minutes earlier at startup, brought the two turns to
within a few percent of each other. Fixed by letting the baseline keep
drifting slowly toward whatever `depthDiff` reads once tracked, via
`BASELINE_DRIFT_ALPHA`, rather than locking it once and holding it for the
rest of the session — the one-shot lock still runs first, so yaw is not held
at a stale zero for the first `CALIBRATION_FRAMES` frames. The known cost:
hold a turn for tens of seconds and the character eases back toward front
under you, because the baseline has drifted onto the held position; not
something a deadzone-gated adapter could avoid, since the whole point is
correcting drift that has already carried the rest point outside the
deadzone. Each fix is re-verified with a synthetic sequence built to
reproduce its specific reported symptom — see the inline comments in
`js/mocap-retarget.js` for the exact sequences. For the fifth fix, that
synthetic check replayed the actual depthDiff readings from the real-camera
session that reported the asymmetry (rest ≈ -15, strong turn -43.66, weak
turn +12.47, each held for a 60-frame/2-second window) through the corrected
logic, and it produced -30.9° and +36.7° — comparable magnitudes, not the
reported ±45°/+9°. That is strong evidence the fix addresses the reported
numbers, but it is still a replay, not a fresh real-camera session: the
discriminating check proposed for this — a single continuous session
holding straight → left → straight → right → straight, 2s each, capturing
`depthDiff` throughout — has not yet been run. None of the five fixes has
been reconfirmed on a live real camera after the fix, only against
synthetic reproductions (real or replayed) of the bugs they targeted.
`applyHeadYaw()` as a whole still needs that real-camera pass before its
deadzone, clamp, damping, calibration, filtering and baseline drift can all
be trusted together rather than each having only cleared the specific
failure that motivated it. The remaining AGENTS.md human checks
(keyboard-only traversal, both colour schemes, breakpoints, reduced motion,
Lighthouse) have not yet been run either.

## Overview

Echo is a page reached as an easter egg from one word of the masthead role,
the way `game.html` is reached from another. A visitor grants camera access,
and their own body, tracked entirely in the browser, drives a rigged 3D
character in real time: raise an arm and the character raises its arm.

The interesting property is the same one Ask and Helm each already carry, on a
third and harder surface. A pose model — MediaPipe's BlazePose, running
through Google's LiteRT.js — turns each video frame into 33 body-landmark
coordinates without the frame ever leaving the tab. Nothing is uploaded,
because there is nowhere for it to go: the model, the character and the
retargeting math are all vendored files loaded same-origin, and the video
stream never touches a `<video>` element's `srcObject` boundary in a way that
would let it be read back out except by the page's own canvas.

This is also the whole justification for two more first-party JavaScript
files. `js/mocap.js` is a boundary and a renderer, in the same shape as
`js/game.js`: it acquires the camera, instantiates the pose model, drives the
inference loop, and drives a Three.js scene. `js/mocap-retarget.js` cannot
claim the same "no rule lives in it" framing `js/game.js` earns, because
turning a landmark coordinate into a bone rotation is exactly a rule, so it is
named and reasoned about on its own terms below, not smuggled in under the
boundary argument.

**Implementation status:** scaffolded. Vendor files, model, and character are
committed; the page, stylesheet and both scripts are not yet wired end to end.
Nothing here is reachable by a visitor until that wiring lands.

---

## Key files

| File | Role |
| --- | --- |
| `mocap.html` | The page: camera gate, video preview, Three.js stage, status region |
| `css/mocap.css` | Stage layout, gate styling. No colour of its own beyond the shared palette. |
| `js/mocap.js` | Camera acquisition, LiteRT load, inference loop, Three.js scene and render loop |
| `js/mocap-retarget.js` | Maps 33 BlazePose landmarks to `godette_rigged.glb` bone rotations |
| `vendor/litert/litert-core.mjs` | Vendored `@litertjs/core`, `CompiledModel`/`Tensor`/`loadLiteRt` |
| `vendor/litert/wasm-utils.mjs` | Vendored `@litertjs/wasm-utils`, `createWasmLib` |
| `vendor/litert/litert_wasm_compat_internal.{js,wasm}` | Non-threaded, no relaxed-SIMD WASM build. The safe default. |
| `vendor/litert/litert_wasm_internal.{js,wasm}` | Non-threaded, relaxed-SIMD WASM build. Auto-selected when the browser supports it. |
| `vendor/three/three.module.min.js` | Vendored Three.js r169 |
| `vendor/three/examples/jsm/loaders/GLTFLoader.js` | Vendored GLTF loader, imports bare `"three"` |
| `assets/models/pose-landmark-full/pose_landmark_full.tflite` | MediaPipe BlazePose full, from `storage.googleapis.com/mediapipe-assets/` |
| `assets/character/godette_rigged.glb` | CC-BY-4.0 rigged character, "Godette (Rigged)" by zahlenmaler, from Sketchfab |

---

## Architecture

```
visitor presses #echo--load
        │
        ├─ getUserMedia({video: true})         camera permission
        ├─ loadLiteRt('vendor/litert/')         picks compat_internal or internal
        │        └─ loadAndCompile(POSE_MODEL_URL)
        └─ GLTFLoader().loadAsync(CHARACTER_URL) → Three.js scene, skinned mesh

requestAnimationFrame loop
        │
   draw video frame to an offscreen canvas, read pixels
        │
   Tensor.fromTypedArray(float32 RGB, inputShape)  ──►  model.run(tensor)
        │
   landmarks: Float32Array, shape read from getOutputDetails(), not hardcoded
        │
   js/mocap-retarget.js: landmarks ──► { boneName: Quaternion }
        │
   apply quaternions to the loaded skeleton's bones ──► renderer.render(scene, camera)
```

### Why GitHub Pages caps which LiteRT build runs

GitHub Pages cannot set response headers, so there is no COOP/COEP, no
cross-origin isolation, and `self.crossOriginIsolated` is `false` — the same
constraint `js/ask.js` already lives under (`ARCHITECTURE.md`). LiteRT.js
ships four WASM variants; `threaded_internal` needs `SharedArrayBuffer` and
`jspi_internal` needs a `threads`+`jspi` combination `loadLiteRt` explicitly
rejects together. Calling `loadLiteRt(path)` with no `options` argument avoids
both: it self-selects `internal` when the browser has relaxed SIMD and falls
back to `compat_internal` otherwise, and neither needs a thread.

### Tensor construction

`vendor/litert/litert-core.mjs`'s `Tensor` has no `fromVideo`/`fromTexture`
convenience for pixel data; the documented path for host memory is
`Tensor.fromTypedArray(data, shape, environment)`, backed by a constructor
that accepts a `TypedArray` plus a shape and infers the element type from the
array's own constructor (a `Float32Array` becomes a `float32` tensor). Feeding
a video frame is therefore:

1. Draw the current `<video>` frame to an offscreen `<canvas>` sized to the
   model's expected input, via `getInputDetails()[0].shape` (never hardcoded,
   so a future model swap cannot silently mismatch).
2. Read it back with `getImageData()`, drop the alpha channel, and normalise
   `Uint8ClampedArray` `[0, 255]` to `Float32Array` `[0, 1]`.
3. `Tensor.fromTypedArray(floatData, inputShape)`.
4. `await model.run(tensor)`, read the landmark output the same
   `getOutputDetails()`-driven way, then `tensor.delete()` / dispose the
   output tensor once copied to a plain array, since `Tensor`s hold WASM
   memory that is not garbage collected.

### The retargeting problem

MediaPipe's 33 pose landmarks (`0` nose … `11`/`12` shoulders, `13`/`14`
elbows, `15`/`16` wrists, `23`/`24` hips, `25`/`26` knees, `27`/`28` ankles, the
rest face and foot detail) are normalized image-space coordinates, not bone
rotations. `js/mocap-retarget.js` is the file that turns one into the other:
for each mapped bone, it takes the landmark pair spanning that bone segment,
builds a direction vector, and computes the quaternion that rotates the rig's
rest-pose bone direction onto it.

Real bone names, read directly out of the loaded scene's own `THREE.Bone`
nodes rather than assumed: `Head_129`, `Neck_1_132`, `Arm_Upper_1L_157`/
`Arm_Upper_1R_187`, `Arm_Lower_1L_155`/`Arm_Lower_1R_185`,
`Leg_UpperL_205`/`Leg_UpperR_211`, `Leg_LowerL_202`/`Leg_LowerR_208` — no dot
separator, despite the GLB's own exported node names carrying one as an L/R
side marker (`Arm_Upper_1.L_157`). `THREE.GLTFLoader` runs every node name
through `PropertyBinding`'s track-path sanitizer on load, which strips that
character before the bone ever reaches `buildBoneMap`. An earlier version of
this file and of `js/mocap-retarget.js` used the dotted, pre-sanitization
form; every one of those names silently missed the real bone,
`buildRestDirections()` built an empty map for them, and the character never
moved with no error ever thrown — see F03-AC09. This is the same failure
mode the site's earlier character (`RobotExpressive.glb`) hit for a
different reason: that rig's names had no dot at all, and an even earlier
version of this mapping assumed one (`UpperArm.L`) anyway. Two different
rigs, two different wrong guesses at the same seam — read `boneMap.keys()`
from the loaded scene, do not assume the naming convention from either the
GLB's JSON or a prior rig. Finger and pole-target bones exist on the rig and
are out of scope — see below.

Head/neck orientation is in scope: BlazePose has no landmark at the base of
the neck, so `js/mocap-retarget.js` drives the rig's `Neck` bone from the
midpoint of the shoulder landmarks to the midpoint of the ear landmarks, via a
`resolveLandmark()` helper that averages an array of landmark indices instead
of reading one directly. `Head` itself is turned separately, and only for
yaw — see `applyHeadYaw()` below; the character's face otherwise stays fixed
relative to its neck rather than swivelling past it independently. The target
was originally the nose landmark rather than the
ear midpoint; the nose sits well forward of the head's actual vertical axis
even when a subject looks straight ahead, which baked a permanent forward
slump into every frame regardless of real head pose. The ears sit close to
that axis at roughly head height, so their midpoint removes most of that
bias, but not all of it: BlazePose's monocular `z` is noisy, and even a
small, realistic forward-lean depth offset between the shoulder and ear
midpoints (0.05 of the normalized range) measured out to roughly 11° of
extra Neck pitch beyond the rig's own ~6° rest baseline, 26° at 0.10. The
`Neck` entry in `BONE_DIRECTIONS` carries a `flattenDepth` flag that zeroes
this z term entirely rather than computing it from landmark depth, which
removes that noise at a real cost: see the comment above
`BONE_DIRECTIONS` in `js/mocap-retarget.js` and the scope cut below.

**The specific degree figures above were measured against
`RobotExpressive.glb`, the character this rig replaced, and have not been
re-measured against `godette_rigged.glb`.** The reasoning (a monocular depth
estimate has no business contributing to a rotation this visible) is not
character-specific and still holds; the magnitudes are not re-verified. See
the same caveat above `BONE_DIRECTIONS` in `js/mocap-retarget.js`.

**KNOWN REGRESSION, confirmed against a real camera (2026-08-05): head
motion is visibly jitterier on `godette_rigged.glb` than it was on
`RobotExpressive.glb`.** This is not a "not yet re-verified" gap, it is a
reported, observed regression, and nothing in this feature fixes it — every
constant and the `flattenDepth` reasoning above are carried over unchanged
from the old rig on the assumption they would still hold, and a real camera
says that assumption is wrong. Three unconfirmed, unisolated candidate
causes: (a) `Neck_1` is a shorter segment than the old rig's single `Neck`
bone, so the same amount of angular noise swings `Head` through a wider arc
at the end of a longer downstream chain (`Neck_2`, `Neck_3`, `Head`); (b) the
unmeasured rest baseline above being far enough off that ordinary landmark
noise now crosses a threshold it previously sat comfortably inside; (c)
`Head_129`'s own baked rest rotation (see below) differing enough from what
was measured on the old rig that `levelHead()`'s correction compounds noise
per frame instead of cancelling it once. A follow-up check — three
screenshots taken seconds apart, subject facing the camera head-on and not
moving — showed the debug overlay's `yaw` pinned at `0.0000` in every frame,
which rules out `applyHeadYaw()` as the source of what those three frames
show, while the character's whole rendered pose (legs and arms, not only
head) visibly changed shape between them despite no change in the input.
That widens the suspect list beyond `Neck`/`Head` to swing retargeting more
generally — see `js/mocap-retarget.js` for the same note. See
[specs/PRD.md](PRD.md) for how this affects the feature's overall status.

Separately, `Head` itself is never a bone `retarget()` writes to — there is
no landmark for it, only for `Neck`. `RobotExpressive.glb` gave `Head` a
small but non-identity rest rotation of its own (measured directly:
`{x: -0.035, y: -0.082, z: -0.0019, w: 0.996}`), and because nothing ever
corrects it, that offset composed on top of whatever `Neck` was doing on
every single frame. A neutral pose that left `Neck`'s own world rotation
level still left `Head`'s world rotation off by roughly 4° of pitch and 9°
of yaw, confirmed with the real `THREE.Bone` state in a loaded scene and
again by a rendered screenshot. **Those figures are `RobotExpressive.glb`'s
own; `godette_rigged.glb`'s `Head_129` rest rotation has not been
re-measured**, though the mechanism `levelHead()` corrects for — a rig's
authored rest rotation on the head bone composing on top of `Neck`'s target
every frame — is generic to any rig, not specific to the old one. This is one
of the three named suspects in the confirmed head-jitter regression above:
`levelHead()` itself is not where a fix would land, since forcing identity
cancels whatever `Head_129`'s baked rotation actually is regardless of its
value, but if `Neck_1`'s own swing rotation is noisier on this rig, `Head`
sitting directly on top of it with nothing absorbing that noise is what
carries it through to what a viewer sees on `Head`.
`levelHead(boneMap)`, exported from `js/mocap-retarget.js` and called once
from `js/mocap.js`'s `setupScene()` alongside `buildRestDirections`, resets
`Head`'s local quaternion to identity at load so its world rotation tracks
`Neck`'s exactly instead of carrying that baked-in tilt forever after.

Turning the head left or right is a twist around its own vertical axis, not a
swing between two directions, and `setFromUnitVectors` — the only rotation
`retarget()` knows how to compute — returns the minimal-arc rotation between
two vectors, which by construction has zero component about the axis those
vectors share. No landmark pair fed into that call, however the target vector
is corrected, can ever produce a twist. Confirmed directly: an asymmetric
ear-depth offset (one ear pushed toward the camera, the other away, x and y
held fixed) produced no change at all in `Neck`'s rotation once `flattenDepth`
zeroed the z term, and even without that flag `resolveLandmark()`'s midpoint
averaging would have cancelled the same antisymmetric signal — only a
*symmetric* depth change, both ears moving together, survives an average of
two points, and that is the forward-lean slump signal `flattenDepth` already
suppresses, not the turn signal. The difference between the two ears' `z` is
where a turn actually shows up, and it never reached anything Neck's target
vector could use.

`applyHeadYaw(landmarks, boneMap, THREE, scratch)`, exported from
`js/mocap-retarget.js` and called from `js/mocap.js`'s `renderFrame()` after
`retarget()`, computes that ear-depth difference and writes it straight to
`Head` as a `setFromAxisAngle` twist about world-up (rotated into `Head`'s
parent-local frame the same way `retarget()` rotates its swing target).
Writing `Head` rather than composing the twist onto `Neck` is deliberate:
`Head` has no mapped children, so a wrong or noisy yaw here cannot leak into
the swing rotation on `Neck` that already earned its own verification. A
0.02-unit deadzone and a clamp to ±45° stand between the raw ear-depth
difference and the applied angle, because this reads the same monocular `z`
channel that produced the Neck pitch bug above: a realistic ~45° turn moves
the ear-depth difference by roughly the same 0.05–0.10 magnitude that a
forward lean once turned into 11°–26° of spurious pitch. That earlier failure
was a constant bias, wrong the same way every frame; a yaw fix on the same
channel, undamped, would instead be zero-mean jitter around a correct center
— a different failure shape, and one a deadzone is the standard fix for, but
still unverified against real noise. Confirmed with synthetic landmarks: a
depth difference below the deadzone produces exactly zero rotation, a
moderate difference (0.07) produces a clean ±22.5° yaw with pitch and roll
held under a millionth of a degree, an extreme difference clamps at ±45°, and
`Neck`'s own world rotation is unchanged across every case, confirming the
isolation from `Neck` holds in practice as well as by construction. What this
cannot confirm is the sign: whether a rightward turn actually reads as a
rightward turn depends on both this file's `y`-depth-sign convention (already
load-bearing for `retarget()`'s arm/leg z-flip) and the real noise floor of
BlazePose's ear-depth estimate, neither of which a synthetic landmark can
exercise. That needs a real camera.

MediaPipe's left/right landmark labels are anatomical, the same convention a
photograph uses: a subject's own raised right hand is still `LEFT_WRIST`'s
mirror counterpart in screen space, not `RIGHT_WRIST`'s. A character viewed
face-on has to move the way a reflection does, so each `BONE_DIRECTIONS` pair
crosses sides on purpose — the visitor's right arm drives the character's own
left arm bone, and the same crossing applies to legs. This is not a naming
mistake to "simplify" back to matching names.

An earlier version of the retargeting math also had a Z-axis sign error:
BlazePose's landmark `z` is depth relative to the hips, where a *smaller*
value means *closer to the camera*, while the scene's camera sits on the
character's `+z` side — so, in this scene, closer to the camera is the
*larger* z. Left unflipped (only the image-down-to-world-up `y` flip was
applied), reaching a limb toward the camera pointed the corresponding bone
away from the camera instead, so every forward motion read as backward. The
fix negates `z` the same way `y` is already negated.

---

## Scope cuts

- **One character.** `godette_rigged.glb` only. A second rig needs its own
  bone-name mapping in `js/mocap-retarget.js`, documented in
  `assets/character/README.md`.
- **No recording, no export, no photo/video capture of any kind.** The stage
  is live-only; closing or navigating away discards everything, which is also
  what keeps the privacy claim structural rather than promised.
- **Body pose and head/neck orientation, not hands or face.** BlazePose's 33
  landmarks cover torso, limbs and the nose. Head orientation mostly follows
  via the `Neck` bone (shoulder midpoint to ear midpoint), with yaw applied
  separately to `Head` by `applyHeadYaw()` (above); finger bones and facial
  morph targets exist on the rig and are not driven.
- **Neck does not convey a forward/backward nod or an in-place head tilt.**
  `flattenDepth` (above) removes the z term that a nod would have shown up
  in, and `resolveLandmark()`'s midpoint averaging removes the rest: one ear
  higher than the other, with the midpoint unmoved, produced the identical
  Neck rotation as a level pose in direct testing, and moving the ear
  midpoint's y alone (holding x fixed) produced no rotation change at all,
  because a normalized target vector doesn't change direction when only its
  magnitude changes. The one motion that does still reach Neck is the ear
  midpoint shifting sideways in x relative to the shoulder midpoint — leaning
  the whole head to one side — which reads mostly as roll. This is a real
  limit of a midpoint-to-midpoint mapping with no per-eye or per-ear signal,
  not a bug still to be found; see the `BONE_DIRECTIONS` comment in
  `js/mocap-retarget.js`.
- **WebGPU is never required.** `loadLiteRt` is called with no `options`, so
  it runs on WASM alone; a `getWebGpuDevice()` path is not used, keeping the
  feature working on any browser Ask already assumes.
- **No mobile-specific layout pass beyond the standard breakpoints.** A
  handheld camera and a stage this size are an awkward pairing; revisit only
  if usage shows it matters.

---

## The DOM contract

Mirrors the `#ask--*` id contract in shape: every id here is looked up once,
at module scope, in `js/mocap.js`, so a missing one is a null dereference on
first interaction, not a silent no-op.

| Id | Element | Contract |
| --- | --- | --- |
| `echo` | `section` or page root | |
| `echo--gate` | `div` | States what camera access is used for and that nothing leaves the tab, before the press |
| `echo--load` | `button type="button"` | Press requests the camera and starts loading the model and character |
| `echo--video` | `video` | `muted`, `playsinline`, visually small or hidden behind the rendered stage — the pixels feed inference, the visitor watches the character, not themselves |
| `echo--stage` | `canvas` | Three.js render target |
| `echo--status` | `p` | `role="status"`, `aria-live="polite"` |
| `echo--stop` | `button type="button"` | Releases the camera stream's tracks and stops the render loop |

Neither button uses the `disabled` property, for the same reason `#ask--load`
does not: `#echo--load` is on the page for the whole multi-second camera
permission prompt and model download, and disabling it on press would strand
a keyboard user on `<body>` for that entire window. Both use `aria-disabled`
plus a re-entry guard flag, exactly like `js/ask.js`.

---

## Acceptance criteria

| ID | Criterion | Evidence |
| --- | --- | --- |
| F03-AC01 | The video frame is never transmitted anywhere; every fetch is same-origin and happens before or independent of camera access. | Structural, and a network panel with no third-party host |
| F03-AC02 | `loadLiteRt` is called with no threading options, so it only ever selects `compat_internal` or `internal`. | Structural: `js/mocap.js` |
| F03-AC03 | Camera tracks are stopped when `#echo--stop` is pressed or the page unloads. | Human, browser camera indicator |
| F03-AC04 | Neither button uses the `disabled` property; focus survives both camera grant and load. | Human, keyboard traversal |
| F03-AC05 | The masthead's two linked words remain real `<a href>`s, reachable and functional with JavaScript off. | Human, JavaScript disabled |
| F03-AC06 | `mocap.html` has no `nav-links--container`, so `check_repo.py`'s nav check does not need updating for it, matching `ask.html`'s precedent. | Structural, `scripts/check_repo.py` |
| F03-AC07 | `mocap.html` is listed in `sitemap.xml`. | `scripts/check_repo.py`, CI |
| F03-AC08 | Contrast holds at 4.5:1 for text in both flavours, checked against Latte. | Human, per colour scheme |
| F03-AC09 | Bone names referenced in `js/mocap-retarget.js` match the names actually present in a loaded scene's `boneMap`. | Human: logged `[...boneMap.keys()]` in a browser (`GLTFLoader` loading `godette_rigged.glb`) and diffed against `BONE_DIRECTIONS`. The GLB's own exported node names carry a `.` (`Arm_Upper_1.L_157`); `THREE.GLTFLoader` strips it via `PropertyBinding`'s sanitizer before the bone reaches `buildBoneMap`, so the confirmed, matching form is `Arm_Upper_1L_157` — an earlier version of this mapping used the unsanitized, dotted form and every `boneMap.get()` call for it returned `undefined` |

---

## Deferred

| Item | Note |
| --- | --- |
| Hand and face tracking | BlazePose's 33 landmarks don't cover them; would need an additional model and a larger inference budget per frame. |
| A second character | Needs its own bone-name mapping; deliberately not built until a first one is proven out. |
| WebGPU acceleration | `loadLiteRt` currently requests no backend preference; revisit if inference latency on WASM alone proves too slow to feel responsive. |
| Recording or sharing a session | Rejected, not deferred — it would dissolve the property the feature exists to demonstrate. |
