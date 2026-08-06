// Echo: landmark -> bone rotation.
//
// This is not a boundary the way js/mocap.js is a boundary. Turning a pose
// landmark into a joint rotation is a rule — a real one, with a real failure
// mode if it's wrong — so per specs/F03_MOCAP.md it is argued here on its own
// terms rather than folded into "instantiate, call, draw" the way js/game.js
// gets to.
//
// Bone names below were read directly out of the running scene's own
// THREE.Bone nodes (root.traverse in buildBoneMap), not assumed from a naming
// convention: RobotExpressive.glb's rig has no dot separator — it is
// UpperArmL/UpperArmR, LowerArmL/LowerArmR, UpperLegL/UpperLegR,
// LowerLegL/LowerLegR, not the dotted "UpperArm.L" a Blender-export naming
// convention would suggest. Two nodes are named "Torso" (one directly under
// Hips as "Torso_1", one elsewhere) and Object3D.traverse() visits its first
// match, so buildBoneMap() below silently keeps whichever is visited first
// under a given name. That is a real ambiguity in the asset, not a bug in
// this file — see assets/character/README.md.

export const LANDMARK = Object.freeze({
  LEFT_EAR: 7,
  RIGHT_EAR: 8,
  LEFT_SHOULDER: 11,
  RIGHT_SHOULDER: 12,
  LEFT_ELBOW: 13,
  RIGHT_ELBOW: 14,
  LEFT_WRIST: 15,
  RIGHT_WRIST: 16,
  LEFT_HIP: 23,
  RIGHT_HIP: 24,
  LEFT_KNEE: 25,
  RIGHT_KNEE: 26,
  LEFT_ANKLE: 27,
  RIGHT_ANKLE: 28,
});

// Each entry maps a bone, by the name it has in the GLB, to the pair of
// landmarks whose direction (parent joint -> child joint) that bone should
// point along. "left"/"right" here are MediaPipe's own labels, and each pair
// below maps a bone directly onto the same-side landmark — UpperArmL to
// LEFT_SHOULDER/LEFT_ELBOW, UpperArmR to RIGHT_SHOULDER/RIGHT_ELBOW, and so
// on. A crossed mapping was tried on the theory that a character facing the
// visitor should move like a reflection, the way a mirror does; verified
// against a real camera, that theory was wrong — it drove the wrong arm bone
// for a given real arm, and the direct, same-side mapping is what actually
// tracks correctly. Do not re-cross these without a real-camera check, not
// just the sandboxed fake-camera one this repo's tooling can run.
//
// Neck is the one entry whose "from" is not a single landmark: BlazePose has
// no landmark at the base of the neck, so the shoulder midpoint stands in for
// it. retarget() below resolves an array `from`/`to` by averaging the
// landmarks it names, which is why Neck can sit in this same list rather than
// needing its own separate pass. Its "to" is the ear midpoint, not the nose:
// the nose sits well forward of the head's actual vertical axis even when a
// subject looks straight ahead, so using it baked a permanent forward slump
// into every frame that is not present with the ear midpoint.
//
// Neck also carries `flattenDepth: true`, which the arm and leg entries do
// not. Measured with synthetic landmarks against the real retarget() below:
// an ear-to-shoulder depth offset of just 0.05 in BlazePose's normalized
// coordinates — a forward lean well within how a visitor actually sits at a
// webcam, not a landmark glitch — produced roughly 11 degrees of extra Neck
// pitch on top of a ~6 degree baseline the rig's own rest pose already
// carries; 0.10 produced roughly 26. That baseline itself comes from
// Torso_1's own authored rest tilt (Torso_1 is not a mapped bone, so its
// world rotation never changes) and retarget()'s parent-relative math
// correctly compensating for it — that part is not a bug. The depth
// sensitivity is: a monocular depth estimate has no business contributing
// that much to a rotation this visible. Dropping the depth term for this one
// entry (see `flattenDepth` in retarget() below) held Neck's pitch exactly
// flat across every depth offset tested, with no change to the arm and leg
// entries, which still use full depth because a limb reaching toward or away
// from the camera needs it.
//
// What flattenDepth costs, beyond what the arm and leg entries pay: Neck's
// target is the vector from the shoulder midpoint to the ear midpoint, so it
// has exactly two components left once z is dropped, x and y, and both are
// weaker than they look. Averaging two points cancels any difference between
// them, so one ear higher than the other (a real lateral head tilt in place)
// leaves the midpoint exactly where it was — confirmed empirically: a
// 0.10-unit ear-height difference around an unmoved midpoint produced the
// identical Neck rotation as a level pose. And when the midpoint sits
// straight above the shoulder midpoint, as it does when facing the camera,
// changing only its distance (y magnitude, with x untouched) does not change
// the *direction* of a vector that is being normalized anyway — confirmed the
// same way: a real forward nod's signal was almost entirely the z term this
// flag just deleted, and moving only y produced no rotation at all. The one
// motion that still reaches Neck is the ear midpoint shifting sideways in x
// relative to the shoulder midpoint — leaning the whole head to one side —
// which reads mostly as roll. Forward/backward nod and in-place lateral tilt
// are both gone. That is a real scope cut, not a bug — see F03_MOCAP.md.
export const BONE_DIRECTIONS = Object.freeze([
  { bone: 'UpperArmL', from: LANDMARK.LEFT_SHOULDER, to: LANDMARK.LEFT_ELBOW },
  { bone: 'LowerArmL', from: LANDMARK.LEFT_ELBOW, to: LANDMARK.LEFT_WRIST },
  { bone: 'UpperArmR', from: LANDMARK.RIGHT_SHOULDER, to: LANDMARK.RIGHT_ELBOW },
  { bone: 'LowerArmR', from: LANDMARK.RIGHT_ELBOW, to: LANDMARK.RIGHT_WRIST },
  { bone: 'UpperLegL', from: LANDMARK.LEFT_HIP, to: LANDMARK.LEFT_KNEE },
  { bone: 'LowerLegL', from: LANDMARK.LEFT_KNEE, to: LANDMARK.LEFT_ANKLE },
  { bone: 'UpperLegR', from: LANDMARK.RIGHT_HIP, to: LANDMARK.RIGHT_KNEE },
  { bone: 'LowerLegR', from: LANDMARK.RIGHT_KNEE, to: LANDMARK.RIGHT_ANKLE },
  { bone: 'Neck', from: [LANDMARK.LEFT_SHOULDER, LANDMARK.RIGHT_SHOULDER], to: [LANDMARK.LEFT_EAR, LANDMARK.RIGHT_EAR], flattenDepth: true },
]);

// Below this a landmark's own visibility score (BlazePose's fourth value per
// point) says the model is guessing rather than seeing, and applying its
// direction does more harm than freezing the limb in its last known pose.
const MIN_VISIBILITY = 0.5;

// Head is never in BONE_DIRECTIONS — there is no landmark pair that names a
// direction *through* the head the way an elbow-to-wrist pair names one
// through a forearm, so it cannot be solved by the same setFromUnitVectors
// swing retarget() uses for every other bone. Measured directly against
// RobotExpressive.glb: even feeding retarget() a perfectly neutral, symmetric,
// forward-facing, zero-depth pose (so Neck's own world rotation comes out
// level, within a thousandth of a degree), Head's world orientation still
// came out roughly 4 degrees pitched and 9 degrees yawed off level. That
// residual is Head's *own* authored local rotation from the GLB, composed on
// top of whatever Neck is doing, and no amount of correcting Neck's target
// vector can cancel a bias that lives one bone further down the chain.
// levelHead() resets Head's local quaternion to identity once, at load, so
// Head's world orientation tracks Neck's exactly rather than carrying that
// baked-in tilt on every frame. Call once, like buildRestDirections — the
// identity this sets is the rest state applyHeadYaw() below turns away from
// and back to every frame, not a value nothing touches again.
export function levelHead(boneMap) {
  boneMap.get('Head')?.quaternion.identity();
}

// Turning the head left/right is a twist around its own long axis, not a
// swing between two directions, and setFromUnitVectors — everything else in
// this file — has no way to produce a twist: it returns the minimal rotation
// between two vectors, which by construction has no component about the axis
// those vectors share. That is why yaw gets its own function instead of a new
// BONE_DIRECTIONS entry. Confirmed against the real retarget() with synthetic
// landmarks: feeding it a deliberately asymmetric ear-depth pair (one ear
// pushed toward the camera, the other away, holding both ears' x and y fixed)
// produced no change at all in Neck's rotation once flattenDepth zeroed the
// term, and even without flattenDepth the shoulder/ear *midpoint* averaging
// in resolveLandmark() would have cancelled that antisymmetric signal anyway
// — only a symmetric depth change (both ears moving together, the forward-lean
// slump flattenDepth exists to suppress) survives an average of two points.
// The signal a turn actually leaves is the *difference* between the two ears'
// z, which never reaches Neck under either mechanism.
//
// This writes Head, not Neck, and that is a deliberate choice, not an
// arbitrary one: Head has no mapped children, so a wrong or noisy yaw here
// cannot propagate into a swing rotation already verified against synthetic
// landmarks and a rendered screenshot. Composing a twist onto Neck's own
// quaternion would put the two fixes in the same rotation, so a mistake in
// this new, unverified-on-a-real-camera code could not be told apart from a
// regression in the Neck math that already earned that verification.
//
// The deadzone and clamp below were originally sized for a normalized
// signal in the range ~0.05-0.10, on the assumption that this reads the same
// small monocular z channel that produced the flattenDepth bug. A debug
// overlay measured against a real camera (see js/mocap.js) showed that
// assumption was wrong by two to three orders of magnitude: this model's raw
// landmark z is not normalized to roughly [-1, 1] the way x and y are — it is
// in the same untransformed units the "Identity" tensor emits everything in,
// which for this signal came out to a real full-turn magnitude of roughly
// 29-35 and a real at-rest noise band of roughly ±10 around baseline
// (measured swinging from -8.8 to +2.5 while holding still). At the old
// 0.12 clamp, every one of those real readings was already 25-280x past it,
// so `magnitude` below was pinned to EAR_DEPTH_AT_MAX_YAW on every tracked
// frame and yaw was always exactly ±45 degrees, never anything in between —
// "never looks straight" is not a damping problem, it is this saturation.
// The noise band crossing zero while genuinely at rest is what then read as
// fidgeting: a sign flip on a saturated signal jumps straight from one
// pinned extreme to the other.
//
// EAR_DEPTH_AT_MAX_YAW is set below the smallest measured real-turn
// magnitude so an actual turn can still reach the clamp. EAR_DEPTH_DEADZONE
// is set above the measured at-rest noise band so that band reads as still
// facing forward instead of a small persistent turn. Both are first cuts
// from four data points on one camera and one face, not a general
// calibration — re-measure with the same overlay before trusting these on a
// different camera. See specs/F03_MOCAP.md.
//
// Re-measured on a second camera/face: the debug overlay showed filteredDiff
// topping out around 5 during an active turn, never once reaching the old
// deadzone of 12 — so on this camera yaw computed to exactly 0 on every
// frame, turn or no turn, which is indistinguishable from "broken" without
// the overlay. This camera's whole usable range sits roughly an order of
// magnitude below the first camera's, not just below its old deadzone, so
// both constants move down together rather than only widening the deadzone.
// Still only two data points on one session, not a general calibration — the
// same "re-measure before trusting these on a different camera" caveat above
// applies to these new values too.
const EAR_DEPTH_DEADZONE = 4;
const EAR_DEPTH_AT_MAX_YAW = 18;
const MAX_HEAD_YAW = Math.PI / 4; // 45 degrees

// How many tracked frames applyHeadYaw() averages together before locking in
// the "forward" baseline described below. Raised from 10 once the real
// at-rest noise band turned out to be roughly ±10 (see above) rather than the
// ~0.05 this was first tuned against — a noise band that large needs more
// than a third of a second of averaging to land the lock near the true
// center rather than near whatever the first few frames happened to read.
const CALIBRATION_FRAMES = 30;

// How much each frame's raw ear-depth-difference reading moves the running
// filtered value used for the deadzone and sign check below, toward that
// frame's own reading. 0.15 means roughly six or seven frames to fold in a
// step change — deliberately similar in order of magnitude to
// HEAD_YAW_SLERP's own settle time, since the two dampers are solving
// adjacent halves of the same noise problem, not independent ones. Not a
// number a real camera has tuned; see the comment where this is applied.
const DEPTH_DIFF_FILTER_ALPHA = 0.15;

// How much each tracked frame's raw depthDiff moves the locked baseline
// itself, after CALIBRATION_FRAMES has already set it once. A real camera
// showed the same turn held for the same duration reaching a filteredDiff of
// roughly -42 one way and only +16 the other — not a per-ear asymmetry:
// re-centering both readings on the depthDiff actually observed at rest
// during that session (not the value CALIBRATION_FRAMES had locked in
// earlier) made the two turns agree to within a few percent. The baseline
// had drifted — auto-exposure, the subject shifting in frame, BlazePose's own
// temporal smoothing settling further — and the one-shot lock from startup
// had no way to follow it. This alpha lets the baseline keep tracking
// wherever depthDiff actually rests, an order of magnitude slower than
// DEPTH_DIFF_FILTER_ALPHA so a held turn (which also sits away from zero) is
// not mistaken for a new rest point for many seconds — long enough for any
// deliberate pose, short enough that the baseline does not need a second
// explicit calibration if the session runs long.
//
// That was the theory. A real camera showed the predicted cost happening much
// sooner than "tens of seconds": a held turn eased back to center within one
// short test, which reads exactly like something is pulling the head back —
// because something is, and it is this constant. The bug was applying it
// unconditionally, on every tracked frame, with no way to tell "the rest
// point has drifted" apart from "the subject is deliberately holding a turn
// right now" — the two look identical to this alpha, since both are just
// depthDiff sitting away from baseline for a while. applyHeadYaw() below now
// only drifts the baseline on a frame that already reads as within the yaw
// deadzone, i.e. a frame yaw itself has decided is "facing forward" rather
// than an active turn, which is the actual distinction this constant needs
// and the deadzone gate already computes for free. A genuine slow drift
// while at rest still gets corrected, at the same rate as before; a held
// turn no longer erases itself, because it is never read as "at rest" in the
// first place.
const BASELINE_DRIFT_ALPHA = 0.003;

// How far Head's current quaternion moves toward this frame's target each
// call, via slerp — not a value derived from any measurement, since there is
// no clean way to synthesize per-frame *noise* the way the deadzone/clamp
// above were tuned against a synthetic offset. It exists because a first,
// undamped version of this function snapped Head straight to whatever the
// current frame's raw ear-depth reading computed, and on a real camera that
// read as binary left/right with nothing in between: the depth channel is
// noisy enough that consecutive frames swing between small and near-clamped
// values rather than sweeping smoothly through the range in between, exactly
// the kind of jitter flagged as a risk before this was ever pointed at a
// camera. 0.25 means roughly a dozen frames to settle on a held pose, which
// is a first guess to make the motion visibly continuous rather than a
// number a real camera has confirmed is right.
const HEAD_YAW_SLERP = 0.25;

// How far each swing-mapped bone moves toward this frame's target each call,
// via slerp, instead of retarget() copying the target straight onto the bone
// the way it did before. A real camera showed a stationary subject's Head
// visibly rotating frame to frame with yaw pinned at exactly 0 — proof the
// motion was not applyHeadYaw()'s twist, which is already damped by
// HEAD_YAW_SLERP above, but retarget()'s swing write, which had no damping at
// all: every frame it solved boneQuat fresh from that frame's raw landmark
// positions and copied it straight onto the bone, so BlazePose's ordinary
// per-frame landmark noise reproduced itself directly as visible rotation.
// Head has no landmark pair of its own (see the note above levelHead()) and
// tracks Neck's world orientation exactly, so Neck's swing noise is what was
// actually visible. Started at the same value as HEAD_YAW_SLERP rather than a
// fresh guess: it is already tuned against this camera and this page, and the
// two dampers solve the same underlying problem — a per-frame landmark
// estimate that wobbles even when the tracked joint is still. Untested
// against a real camera; see specs/F03_MOCAP.md and the neckDeltaDeg
// diagnostic below, which exists so that claim can be checked with a number
// instead of eyeballing a screenshot.
const SWING_SLERP = 0.25;

export function applyHeadYaw(landmarks, boneMap, THREE, scratch) {
  const head = boneMap.get('Head');
  if (!head) return undefined;

  scratch.headYawAxis ??= new THREE.Vector3();
  scratch.headParentWorldQuat ??= new THREE.Quaternion();
  scratch.headYawQuat ??= new THREE.Quaternion();
  const { headYawAxis, headParentWorldQuat, headYawQuat } = scratch;

  const left = landmarks[LANDMARK.LEFT_EAR];
  const right = landmarks[LANDMARK.RIGHT_EAR];
  const tracked =
    left && right && left.visibility >= MIN_VISIBILITY && right.visibility >= MIN_VISIBILITY;

  // Losing track of either ear used to leave Head's quaternion untouched —
  // frozen at whatever the last confident frame computed, which on a real
  // camera reads as the head getting stuck turned and never coming back to
  // front, because a real turn is exactly when BlazePose tends to lose
  // confidence on the far ear. Falling through to yaw = 0 here instead means
  // a lost ear eases Head back toward front through the same slerp below,
  // rather than holding the last value forever.
  // Temporary diagnostics, not part of the shipped rule: see the debug
  // overlay wired up in js/mocap.js. Returned rather than logged from in
  // here so this file stays free of a DOM/console dependency it does not
  // otherwise have. Strip this and the overlay together once the real
  // camera noise floor is measured — see specs/F03_MOCAP.md.
  const diagnostics = { tracked };

  let yaw = 0;
  if (tracked) {
    // BlazePose z is depth relative to the hips with a *smaller* value
    // meaning *closer* to the camera (see the coordinate-system note in
    // retarget() below). Turning the head so the right ear leads — moves
    // toward the camera, so smaller z — should read as a positive yaw in
    // Three.js's right-handed convention (counter-clockwise looking down
    // +Y), which is (left.z - right.z): right ear closer makes this
    // positive. Untested against a real camera; this sign is derived from
    // the same convention note retarget() already relies on, not
    // independently verified here.
    const depthDiff = left.z - right.z;
    diagnostics.depthDiff = depthDiff;

    // A real camera showed this defaulting to a left turn while the subject
    // faced the camera dead on. That is not jitter around zero — jitter
    // would average out and only occasionally cross the deadzone in either
    // direction — it is a `depthDiff` that sits on one side of zero every
    // frame, which means "camera facing forward" and "depthDiff == 0" are
    // not the same thing for this camera/face. Likely causes are a webcam
    // that is not dead-level with the face or an ear whose monocular depth
    // BlazePose consistently misjudges relative to the other, and neither is
    // something a fixed deadzone can correct, because a bias that never
    // crosses zero never re-enters the deadzone to be caught by it.
    // Calibrating out whatever `depthDiff` reads treats some early reading as
    // "forward" and measures every later frame as a delta from it, which is
    // exactly the assumption this easter egg already makes implicitly — the
    // visitor is expected to be facing the camera when the feature starts.
    //
    // The first version of this calibration took a single frame as that
    // reading, and a real camera showed the opposite failure: the head then
    // defaulted to an exaggerated turn the *other* way. A single frame is
    // exactly as exposed to per-frame noise as the depthDiff signal it is
    // meant to correct — the very first tracked frame is also the one most
    // likely to catch the camera mid-auto-exposure-adjustment or the model's
    // own temporal smoothing not yet warmed up, so a noisy sample there
    // becomes a wrong, permanent baseline for the rest of the session.
    // Averaging over CALIBRATION_FRAMES tracked frames before locking the
    // baseline in — holding yaw at 0 for that short window rather than
    // computing it against an incomplete average — trades a brief, unmoving
    // start for a baseline no single noisy frame can dominate. Untested
    // against a real camera; verified only that a synthetic single noisy
    // outlier frame among otherwise-stable readings no longer swings the
    // locked baseline the way a single-frame calibration did.
    scratch.headYawCalibrationSum ??= 0;
    scratch.headYawCalibrationCount ??= 0;
    if (scratch.headYawBaseline === undefined) {
      scratch.headYawCalibrationSum += depthDiff;
      scratch.headYawCalibrationCount += 1;
      if (scratch.headYawCalibrationCount >= CALIBRATION_FRAMES) {
        scratch.headYawBaseline = scratch.headYawCalibrationSum / scratch.headYawCalibrationCount;
      }
    }

    diagnostics.baseline = scratch.headYawBaseline;

    if (scratch.headYawBaseline !== undefined) {
      const adjustedDiff = depthDiff - scratch.headYawBaseline;
      diagnostics.adjustedDiff = adjustedDiff;

      // Every fix above still fed the deadzone a raw, single-frame
      // adjustedDiff, and a real camera showed exactly the failure that
      // implies: the head oscillating left/right without ever settling.
      // That is not the deadzone being wrong or the baseline being wrong —
      // it is that the raw signal's own *sign* flips from one frame to the
      // next when its true value sits near zero, so the target this
      // function hands to the slerp below flips too, and a slerp chasing a
      // target that reverses every frame never converges no matter how slow
      // it is told to move. The deadzone and the output slerp both assumed
      // the noise they had to survive was small compared to a real turn;
      // this is the fix for when it isn't. Filtering the signal itself,
      // before it ever reaches the deadzone, means the value the deadzone
      // and sign check see has already had most of that frame-to-frame
      // noise averaged out, so it can no longer flip sign on noise alone.
      // DEPTH_DIFF_FILTER_ALPHA is deliberately close to the noise-vs-signal
      // problem HEAD_YAW_SLERP already exists to solve, and the two stack:
      // this smooths the target before it is computed, that smooths the
      // bone's approach to whatever target results. Untested against a real
      // camera; verified only that a synthetic signal alternating in sign
      // every frame, well past the deadzone in each direction, no longer
      // makes the computed yaw target flip sign every frame the way an
      // unfiltered reading did.
      scratch.headYawFilteredDiff ??= adjustedDiff;
      scratch.headYawFilteredDiff += DEPTH_DIFF_FILTER_ALPHA * (adjustedDiff - scratch.headYawFilteredDiff);
      const filteredDiff = scratch.headYawFilteredDiff;
      diagnostics.filteredDiff = filteredDiff;

      const magnitude = Math.min(Math.abs(filteredDiff), EAR_DEPTH_AT_MAX_YAW);
      if (magnitude >= EAR_DEPTH_DEADZONE) {
        const t = (magnitude - EAR_DEPTH_DEADZONE) / (EAR_DEPTH_AT_MAX_YAW - EAR_DEPTH_DEADZONE);
        yaw = Math.sign(filteredDiff) * t * MAX_HEAD_YAW;
      } else {
        // Only drift the baseline on a frame the deadzone already reads as
        // "facing forward" — see the BASELINE_DRIFT_ALPHA comment above for
        // why an unconditional drift here used to erase a held turn instead
        // of tracking genuine at-rest camera drift.
        scratch.headYawBaseline += BASELINE_DRIFT_ALPHA * (depthDiff - scratch.headYawBaseline);
      }
    }
  }
  diagnostics.yaw = yaw;

  // The axis to twist about is world-up, not Head's own local Y: the rig's
  // rest pose is not guaranteed to have the bone's local axes aligned with
  // the world, and levelHead() only zeroed Head's own local rotation, not its
  // parent chain's. Rotating world-up into Head's parent's local frame is the
  // same parent-relative transform retarget() already uses for its swing
  // target, applied here to an axis instead of a direction.
  head.parent.getWorldQuaternion(headParentWorldQuat);
  headYawAxis.set(0, 1, 0).applyQuaternion(headParentWorldQuat.clone().invert()).normalize();
  headYawQuat.setFromAxisAngle(headYawAxis, yaw);
  head.quaternion.slerp(headYawQuat, HEAD_YAW_SLERP);
  head.updateWorldMatrix(false, true);
  return diagnostics;
}

// Walks the loaded glTF scene once and returns { name -> THREE.Bone }, using
// each bone's *first* traversal match — see the Torso note above. Built once
// after the character loads, not per frame: bone identity does not change
// while the page runs.
export function buildBoneMap(root) {
  const bones = new Map();
  root.traverse((node) => {
    if (node.isBone && !bones.has(node.name)) {
      bones.set(node.name, node);
    }
  });
  return bones;
}

// For each bone this file drives, the direction from that bone to its own
// first child, expressed in the bone's *local* space, at the moment the
// character loaded. A bone's THREE.Object3D children carry their rest-pose
// offset in exactly that space already, so this needs no separate bind-pose
// file or T-pose calibration step: RobotExpressive's authored rest pose *is*
// the reference.
export function buildRestDirections(boneMap) {
  const rest = new Map();
  for (const { bone: name } of BONE_DIRECTIONS) {
    const bone = boneMap.get(name);
    const child = bone?.children.find((c) => c.isBone) ?? bone?.children[0];
    if (!bone || !child) continue;
    rest.set(name, child.position.clone().normalize());
  }
  return rest;
}

// Resolves a BONE_DIRECTIONS `from`/`to` entry to a landmark. A plain index
// reads landmarks[i] directly; an array (Neck's shoulder and ear midpoints,
// currently the only case) averages the named landmarks' positions and takes
// their lowest visibility, so a midpoint is exactly as willing to freeze as a
// single low-confidence landmark would be.
function resolveLandmark(landmarks, ref) {
  if (!Array.isArray(ref)) return landmarks[ref];
  let x = 0;
  let y = 0;
  let z = 0;
  let visibility = Infinity;
  for (const i of ref) {
    const l = landmarks[i];
    if (!l) return undefined;
    x += l.x;
    y += l.y;
    z += l.z;
    visibility = Math.min(visibility, l.visibility);
  }
  return { x: x / ref.length, y: y / ref.length, z: z / ref.length, visibility };
}

// Rotates each mapped bone so the direction from it to its child matches the
// corresponding landmark pair, leaving everything else (spine, hands, facial
// expression) exactly as RobotExpressive.glb authored it — see F03_MOCAP.md's
// scope cuts. Head orientation mostly follows via the Neck bone above; the
// one exception is yaw, applied separately by applyHeadYaw() straight to
// Head, for the reason argued in the comment above that function — this loop
// has no way to produce a twist.
//
// The math: a bone's world rotation is its parent's world rotation composed
// with the bone's own local quaternion, so the world-space direction to its
// child is (parentWorldQuat * boneQuat * restDirLocal). Solving for the
// boneQuat that makes this equal a target world direction is one
// setFromUnitVectors call between two vectors already in the same frame —
// restDirLocal, and the target direction rotated into that same frame by the
// parent's *current* world quaternion. Bones must therefore be processed
// parent-before-child in BONE_DIRECTIONS (they are, arm and leg bones have no
// mapped bone as their parent) and the scene's world matrices refreshed
// after each write, since the next bone's parent-world-quaternion read
// depends on it.
//
// This is swing only: it has no notion of the bone's own long-axis twist
// (bicep rotation, forearm pronation), because a straight-line landmark pair
// carries no twist information to recover it from. A real body's elbow can
// twist independently of where the wrist points; this puppet's can't.
export function retarget(landmarks, boneMap, restDirections, THREE, scratch) {
  // Assigned back with ??=, not read with ??: without the assignment this
  // allocated four fresh objects every frame and "scratch" was decoration.
  scratch.targetWorld ??= new THREE.Vector3();
  scratch.targetLocal ??= new THREE.Vector3();
  scratch.parentWorldQuat ??= new THREE.Quaternion();
  scratch.boneQuat ??= new THREE.Quaternion();
  scratch.neckPrevQuat ??= new THREE.Quaternion();
  const { targetWorld, targetLocal, parentWorldQuat, boneQuat, neckPrevQuat } = scratch;

  // Temporary measurement instrument, same lifetime as the debug overlay in
  // js/mocap.js: nothing upstream of retarget() had a number for how much
  // Neck's swing actually moved frame to frame, only the yaw twist's own
  // diagnostics, so a jitter complaint had nothing to check but a screenshot.
  // Only Neck is measured — it is the shortest landmark pair in
  // BONE_DIRECTIONS (shoulder midpoint to ear midpoint) and the one Head
  // tracks exactly via levelHead(), so it is where swing noise is most
  // visible, not because the other eight bones don't move.
  const diagnostics = {};

  for (const { bone: name, from, to, flattenDepth } of BONE_DIRECTIONS) {
    const bone = boneMap.get(name);
    const restDir = restDirections.get(name);
    if (!bone || !restDir) continue;

    const a = resolveLandmark(landmarks, from);
    const b = resolveLandmark(landmarks, to);
    if (!a || !b || a.visibility < MIN_VISIBILITY || b.visibility < MIN_VISIBILITY) {
      continue;
    }

    // See the coordinate-system note in js/mocap.js: landmark y is flipped
    // here to go from image-down to world-up. z needs the same treatment for
    // the same kind of reason: BlazePose's z is depth relative to the hips
    // with a *smaller* value meaning *closer* to the camera, while the scene
    // in js/mocap.js sits the camera on the character's +z side, so "closer
    // to the camera" is the *larger* z. Left unflipped, raising an arm toward
    // the camera (the real-world "forward") pointed the bone away from the
    // camera instead — the puppet reached backward for every forward motion.
    //
    // `flattenDepth` drops that z term to zero instead of computing it — see
    // the BONE_DIRECTIONS comment on Neck for the measurement that justifies
    // this for exactly that one entry.
    targetWorld.set(b.x - a.x, -(b.y - a.y), flattenDepth ? 0 : -(b.z - a.z));
    if (targetWorld.lengthSq() < 1e-8) continue;
    targetWorld.normalize();

    const parent = bone.parent;
    parent.getWorldQuaternion(parentWorldQuat);
    targetLocal.copy(targetWorld).applyQuaternion(parentWorldQuat.clone().invert());

    boneQuat.setFromUnitVectors(restDir, targetLocal);
    if (name === 'Neck') neckPrevQuat.copy(bone.quaternion);
    bone.quaternion.slerp(boneQuat, SWING_SLERP);
    bone.updateWorldMatrix(false, true);
    if (name === 'Neck') {
      diagnostics.neckDeltaDeg = (bone.quaternion.angleTo(neckPrevQuat) * 180) / Math.PI;
    }
  }

  return diagnostics;
}
