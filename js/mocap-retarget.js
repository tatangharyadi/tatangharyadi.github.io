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

// Head is never in BONE_DIRECTIONS — there is no landmark for it, and
// retarget() below only ever writes a bone it maps. Measured directly against
// RobotExpressive.glb: even feeding retarget() a perfectly neutral, symmetric,
// forward-facing, zero-depth pose (so Neck's own world rotation comes out
// level, within a thousandth of a degree), Head's world orientation still
// came out roughly 4 degrees pitched and 9 degrees yawed off level. That
// residual is Head's *own* authored local rotation from the GLB, composed on
// top of whatever Neck is doing, and no amount of correcting Neck's target
// vector can cancel a bias that lives one bone further down the chain.
// levelHead() resets Head's local quaternion to identity once, at load, so
// Head's world orientation tracks Neck's exactly rather than carrying that
// baked-in tilt on every frame. Call once, like buildRestDirections — nothing
// after load ever writes Head's local quaternion again, so nothing needs to
// re-level it per frame.
export function levelHead(boneMap) {
  boneMap.get('Head')?.quaternion.identity();
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
// scope cuts. Head orientation follows via the Neck bone above; nothing turns
// the Head bone itself, so the character's face stays fixed relative to its
// neck the way a real head does not independently swivel past it.
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
