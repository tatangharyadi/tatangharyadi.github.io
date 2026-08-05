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
// point along. "left"/"right" here are MediaPipe's, which are the subject's
// own left and right as seen from behind the lens looking out — i.e. already
// mirrored the way a mirror mirrors you, not the way a photo of you does. A
// front-facing selfie camera needs no un-mirroring for this reason; a
// rear-facing one would.
export const BONE_DIRECTIONS = Object.freeze([
  { bone: 'UpperArmL', from: LANDMARK.LEFT_SHOULDER, to: LANDMARK.LEFT_ELBOW },
  { bone: 'LowerArmL', from: LANDMARK.LEFT_ELBOW, to: LANDMARK.LEFT_WRIST },
  { bone: 'UpperArmR', from: LANDMARK.RIGHT_SHOULDER, to: LANDMARK.RIGHT_ELBOW },
  { bone: 'LowerArmR', from: LANDMARK.RIGHT_ELBOW, to: LANDMARK.RIGHT_WRIST },
  { bone: 'UpperLegL', from: LANDMARK.LEFT_HIP, to: LANDMARK.LEFT_KNEE },
  { bone: 'LowerLegL', from: LANDMARK.LEFT_KNEE, to: LANDMARK.LEFT_ANKLE },
  { bone: 'UpperLegR', from: LANDMARK.RIGHT_HIP, to: LANDMARK.RIGHT_KNEE },
  { bone: 'LowerLegR', from: LANDMARK.RIGHT_KNEE, to: LANDMARK.RIGHT_ANKLE },
]);

// Below this a landmark's own visibility score (BlazePose's fourth value per
// point) says the model is guessing rather than seeing, and applying its
// direction does more harm than freezing the limb in its last known pose.
const MIN_VISIBILITY = 0.5;

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

// Rotates each mapped bone so the direction from it to its child matches the
// corresponding landmark pair, leaving everything else (spine, hands, face)
// exactly as RobotExpressive.glb authored it — see F03_MOCAP.md's scope cuts.
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

  for (const { bone: name, from, to } of BONE_DIRECTIONS) {
    const bone = boneMap.get(name);
    const restDir = restDirections.get(name);
    if (!bone || !restDir) continue;

    const a = landmarks[from];
    const b = landmarks[to];
    if (!a || !b || a.visibility < MIN_VISIBILITY || b.visibility < MIN_VISIBILITY) {
      continue;
    }

    // See the coordinate-system note in js/mocap.js: landmark y is flipped
    // here to go from image-down to world-up before anything else touches it.
    targetWorld.set(b.x - a.x, -(b.y - a.y), b.z - a.z);
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
