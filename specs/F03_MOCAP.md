# F03: Echo, a webcam puppeting a 3D character on device

**Status:** wired end to end and verified in a browser — camera access, pose
inference, retargeting and rendering all confirmed working, including a
stop/restart cycle. Three real bugs found and fixed since, each verified
against the live `retarget()`/`buildBoneMap`/`levelHead` code path with
synthetic landmarks rather than by inspection: a mirrored L/R mapping
(reverted to direct, same-side landmarks), excess Neck pitch from monocular
depth noise (fixed via `flattenDepth`, see below), and a static ~4°/~9°
pitch/yaw offset baked into the `Head` bone's own rest rotation in the GLB
(fixed via `levelHead()`). None of these three fixes has been checked against
a real webcam — only against synthetic landmarks fed through the real code in
a loaded browser tab, which is strong evidence but not the same claim. The
remaining AGENTS.md human checks (keyboard-only traversal, both colour
schemes, breakpoints, reduced motion, Lighthouse, and a real-camera pass) have
not yet been run.

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
| `js/mocap-retarget.js` | Maps 33 BlazePose landmarks to `RobotExpressive.glb` bone rotations |
| `vendor/litert/litert-core.mjs` | Vendored `@litertjs/core`, `CompiledModel`/`Tensor`/`loadLiteRt` |
| `vendor/litert/wasm-utils.mjs` | Vendored `@litertjs/wasm-utils`, `createWasmLib` |
| `vendor/litert/litert_wasm_compat_internal.{js,wasm}` | Non-threaded, no relaxed-SIMD WASM build. The safe default. |
| `vendor/litert/litert_wasm_internal.{js,wasm}` | Non-threaded, relaxed-SIMD WASM build. Auto-selected when the browser supports it. |
| `vendor/three/three.module.min.js` | Vendored Three.js r169 |
| `vendor/three/examples/jsm/loaders/GLTFLoader.js` | Vendored GLTF loader, imports bare `"three"` |
| `assets/models/pose-landmark-full/pose_landmark_full.tflite` | MediaPipe BlazePose full, from `storage.googleapis.com/mediapipe-assets/` |
| `assets/character/RobotExpressive.glb` | CC0 rigged character, from `mrdoob/three.js` examples |

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
nodes rather than assumed: `Hips`, `Neck`, `Head`, `ShoulderL`/`ShoulderR`,
`UpperArmL`/`UpperArmR`, `LowerArmL`/`LowerArmR`, `UpperLegL`/`UpperLegR`,
`LowerLegL`/`LowerLegR` — no dot separator, despite a Blender-export naming
convention suggesting one. An earlier version of this file and of
`js/mocap-retarget.js` assumed the dotted form (`UpperArm.L`); every one of
those names silently missed the real bone, `buildRestDirections()` built an
empty map, and the character never moved with no error ever thrown — see
F03-AC09. The rig carries two nodes both named `Torso`;
`js/mocap-retarget.js` documents which one it targets and why, inline, the
first time this ambiguity matters, rather than leaving a future reader to
rediscover it from the GLB by hand. Finger and pole-target bones exist on the
rig and are out of scope — see below.

Head/neck orientation is in scope: BlazePose has no landmark at the base of
the neck, so `js/mocap-retarget.js` drives the rig's `Neck` bone from the
midpoint of the shoulder landmarks to the midpoint of the ear landmarks, via a
`resolveLandmark()` helper that averages an array of landmark indices instead
of reading one directly. Nothing turns the `Head` bone itself, so the
character's face stays fixed relative to its neck rather than swivelling past
it independently. The target was originally the nose landmark rather than the
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

Separately, `Head` itself is never a bone `retarget()` writes to — there is
no landmark for it, only for `Neck`. `RobotExpressive.glb` gives `Head` a
small but non-identity rest rotation of its own (measured directly:
`{x: -0.035, y: -0.082, z: -0.0019, w: 0.996}`), and because nothing ever
corrects it, that offset composes on top of whatever `Neck` is doing on
every single frame. A neutral pose that left `Neck`'s own world rotation
level still left `Head`'s world rotation off by roughly 4° of pitch and 9°
of yaw, confirmed with the real `THREE.Bone` state in a loaded scene and
again by a rendered screenshot. `levelHead(boneMap)`, exported from
`js/mocap-retarget.js` and called once from `js/mocap.js`'s `setupScene()`
alongside `buildRestDirections`, resets `Head`'s local quaternion to
identity at load so its world rotation tracks `Neck`'s exactly instead of
carrying that baked-in tilt forever after.

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

- **One character.** `RobotExpressive.glb` only. A second rig needs its own
  bone-name mapping in `js/mocap-retarget.js`, documented in
  `assets/character/README.md`.
- **No recording, no export, no photo/video capture of any kind.** The stage
  is live-only; closing or navigating away discards everything, which is also
  what keeps the privacy claim structural rather than promised.
- **Body pose and head/neck orientation, not hands or face.** BlazePose's 33
  landmarks cover torso, limbs and the nose. Head orientation follows via the
  `Neck` bone (shoulder midpoint to ear midpoint); finger bones and facial
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
| F03-AC09 | Bone names referenced in `js/mocap-retarget.js` match the names actually present in `RobotExpressive.glb`. | Human: logged `[...boneMap.keys()]` at runtime and diffed against `BONE_DIRECTIONS`; the dotted names an earlier version used (`UpperArm.L`) never matched and were corrected to the rig's real, dotless names (`UpperArmL`) |

---

## Deferred

| Item | Note |
| --- | --- |
| Hand and face tracking | BlazePose's 33 landmarks don't cover them; would need an additional model and a larger inference budget per frame. |
| A second character | Needs its own bone-name mapping; deliberately not built until a first one is proven out. |
| WebGPU acceleration | `loadLiteRt` currently requests no backend preference; revisit if inference latency on WASM alone proves too slow to feel responsive. |
| Recording or sharing a session | Rejected, not deferred — it would dissolve the property the feature exists to demonstrate. |
