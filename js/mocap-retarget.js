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
// convention: godette_rigged.glb's exported node names carry the exporter's
// own numeric suffix (`Head_129`, `Arm_Upper_1L_157`) rather than the clean
// `UpperArmL` names an earlier character (RobotExpressive.glb) used, so every
// name below is a literal copy-paste of what buildBoneMap() actually saw, not
// a guess at what the rig "should" be called. It is also not a literal
// copy-paste of the GLB's own JSON: the exported node names carry a `.` (an
// L/R side marker, e.g. `Arm_Upper_1.L_157`), and THREE.GLTFLoader's node
// names are run through `PropertyBinding`'s track-path sanitizer, which
// strips that character before the bone ever reaches `buildBoneMap` —
// `boneMap.keys()` in a loaded scene reads `Arm_Upper_1L_157`, not the form
// the GLB's own JSON carries. Every name below is copied from the sanitized,
// post-load form, confirmed against a loaded scene rather than derived from
// the export. The rig also carries a parallel IK control skeleton
// (`IK_Arm_UpperL`, `IK_Leg_UpperL`, …) alongside the FK bones mapped below;
// those are left unmapped and untouched because nothing here drives them and
// there is no baked animation for them to reproduce.
//
// The arm and leg chains each split what a single "upper arm"/"upper leg"
// bone would be into two bones — `Arm_Upper_1L` then `Arm_Upper_2L`, same
// for the lower segment and for the leg's `Leg_UpperL`/`Leg_Upper_2L` — a
// twist-correction split for smoother deformation, not two independent
// joints. Only the first bone of each pair (`Arm_Upper_1L`, `Arm_Lower_1L`,
// `Leg_UpperL`, `Leg_LowerL`) is mapped below; the second, unmapped bone
// keeps its authored rest-pose offset from its parent and is carried along
// by it, the same way any other unmapped child bone would be.

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
// not, for the same reason regardless of which character is loaded: a
// monocular depth estimate has no business contributing to a rotation as
// visible as a forward nod. The specific magnitudes below (a ~6 degree rest
// baseline, ~11 degrees of extra pitch from a 0.05 depth offset, ~26 from
// 0.10) were measured against RobotExpressive.glb, the character this rig
// replaced, and have not been re-measured against godette_rigged.glb's own
// rest pose or its three-segment Neck_1/Neck_2/Neck_3 chain — this entry
// only drives Neck_1, the base segment, so the rest baseline in particular
// may differ. The reasoning (dropping the depth term removes a
// disproportionate noise source) is not character-specific and still holds;
// this rig's own magnitudes have not been measured.
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
// Bone names carry the sanitized form THREE.GLTFLoader's node names end up
// with, not the raw names in the GLB's own JSON: GLTFLoader strips
// characters unsafe for animation-track paths (`PropertyBinding`'s
// sanitizer), which drops the `.` out of names like `Arm_Upper_1.L_157`.
// A loaded `boneMap.keys()` reads `Arm_Upper_1L_157`, not the dotted form —
// confirmed here, not assumed.
export const BONE_DIRECTIONS = Object.freeze([
  { bone: 'Arm_Upper_1L_157', from: LANDMARK.LEFT_SHOULDER, to: LANDMARK.LEFT_ELBOW },
  { bone: 'Arm_Lower_1L_155', from: LANDMARK.LEFT_ELBOW, to: LANDMARK.LEFT_WRIST },
  { bone: 'Arm_Upper_1R_187', from: LANDMARK.RIGHT_SHOULDER, to: LANDMARK.RIGHT_ELBOW },
  { bone: 'Arm_Lower_1R_185', from: LANDMARK.RIGHT_ELBOW, to: LANDMARK.RIGHT_WRIST },
  { bone: 'Leg_UpperL_205', from: LANDMARK.LEFT_HIP, to: LANDMARK.LEFT_KNEE },
  { bone: 'Leg_LowerL_202', from: LANDMARK.LEFT_KNEE, to: LANDMARK.LEFT_ANKLE },
  { bone: 'Leg_UpperR_211', from: LANDMARK.RIGHT_HIP, to: LANDMARK.RIGHT_KNEE },
  { bone: 'Leg_LowerR_208', from: LANDMARK.RIGHT_KNEE, to: LANDMARK.RIGHT_ANKLE },
  { bone: 'Neck_1_132', from: [LANDMARK.LEFT_SHOULDER, LANDMARK.RIGHT_SHOULDER], to: [LANDMARK.LEFT_EAR, LANDMARK.RIGHT_EAR], flattenDepth: true },
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
  boneMap.get('Head_129')?.quaternion.identity();
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
const EAR_DEPTH_DEADZONE = 12;
const EAR_DEPTH_AT_MAX_YAW = 30;
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
// explicit calibration if the session runs long. The known cost: hold a turn
// for tens of seconds and the character eases back toward front under you,
// because the baseline has drifted onto your held position. Acceptable for an
// easter egg; not something a deadzone gate could avoid, since the whole
// point is correcting drift that has already carried the rest point outside
// the deadzone.
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

export function applyHeadYaw(landmarks, boneMap, THREE, scratch) {
  const head = boneMap.get('Head_129');
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
    } else {
      scratch.headYawBaseline += BASELINE_DRIFT_ALPHA * (depthDiff - scratch.headYawBaseline);
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

// Walks the loaded glTF scene once and returns { name -> THREE.Bone }. Uses
// each bone's *first* traversal match in case of a duplicate name — the
// rig this replaced (RobotExpressive.glb) carried two nodes both named
// `Torso`; godette_rigged.glb has no duplicate bone names, so this is
// defensive rather than load-bearing here. Built once after the character
// loads, not per frame: bone identity does not change while the page runs.
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
// file or T-pose calibration step: the character's authored rest pose *is*
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
// expression) exactly as godette_rigged.glb authored it — see F03_MOCAP.md's
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
  const { targetWorld, targetLocal, parentWorldQuat, boneQuat } = scratch;

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
    bone.quaternion.copy(boneQuat);
    bone.updateWorldMatrix(false, true);
  }
}
