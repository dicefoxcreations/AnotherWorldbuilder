import * as THREE from "three";

/**
 * The throw, worked out before it is shown: dice fly, hit the table, bounce
 * off its edges and one another, and roll until each lies flat on a face.
 * Nothing steers them toward a number — which face ends on top is up to the
 * throw. The table then turns the numbers on each die (`DiceShapes.symmetries`)
 * so that the face lying on top shows what was rolled.
 */

/** A die as thrown: its corners and faces in its own space, and how it leaves the hand. */
export type Thrown = {
  corners: THREE.Vector3[];
  normals: THREE.Vector3[];
  /** Its corners' distance from its middle. */
  radius: number;
  pos: THREE.Vector3;
  vel: THREE.Vector3;
  turn: THREE.Quaternion;
  spin: THREE.Vector3;
};

export type Pose = { pos: THREE.Vector3; turn: THREE.Quaternion };

export type Track = {
  /** Per step, per die. */
  poses: Pose[][];
  /** Seconds until a die first hits the table — when it sounds. */
  hit: number;
};

/** Seconds per step. */
export const STEP = 1 / 240;
const GRAVITY = 40;
const BOUNCE = 0.32;
const WALL_BOUNCE = 0.5;
const GRIP = 0.45;
/** From then on the table holds them more and more, so no throw rolls on for long. */
const SLOW_FROM = 1.2;
export const LONGEST = 4;

/** Steps a die takes to lie exactly flat once it rests. */
const SETTLE_STEPS = 8;
const UP = new THREE.Vector3(0, 1, 0);

type Body = Thrown & {
  invInertia: number;
  inside: boolean;
  still: number;
  /** The step it came to rest, and how it then lies. */
  rested?: { step: number; pose: Pose };
};

const cross = (a: THREE.Vector3, b: THREE.Vector3) => new THREE.Vector3().crossVectors(a, b);

/** An impulse at a point of the die, along `n`. */
function push(b: Body, r: THREE.Vector3, j: THREE.Vector3) {
  b.vel.add(j);
  b.spin.addScaledVector(cross(r, j), b.invInertia);
}

/** The die's corner `r` (from its middle) meets a surface with normal `n`. */
function contact(b: Body, r: THREE.Vector3, n: THREE.Vector3, bounce: number) {
  const at = b.vel.clone().add(cross(b.spin, r));
  const vn = at.dot(n);
  if (vn >= 0) return;
  const rn = cross(r, n);
  const jn = (-(1 + (vn < -1 ? bounce : 0)) * vn) / (1 + b.invInertia * rn.lengthSq());
  push(b, r, n.clone().multiplyScalar(jn));
  // Friction: what slides along the surface, held by at most GRIP of the push.
  const after = b.vel.clone().add(cross(b.spin, r));
  const slide = after.sub(n.clone().multiplyScalar(after.dot(n)));
  const speed = slide.length();
  if (speed < 1e-6) return;
  const t = slide.divideScalar(speed);
  const rt = cross(r, t);
  const jt = Math.min(speed / (1 + b.invInertia * rt.lengthSq()), GRIP * jn);
  push(b, r, t.multiplyScalar(-jt));
}

/** Rolls the dice out on a table `half` wide and deep (from its middle). */
export function simulate(thrown: Thrown[], half: { x: number; z: number }, random: () => number = Math.random): Track {
  const bodies: Body[] = thrown.map((t) => ({
    ...t,
    pos: t.pos.clone(),
    vel: t.vel.clone(),
    turn: t.turn.clone(),
    spin: t.spin.clone(),
    invInertia: 1 / (0.25 * t.radius * t.radius),
    inside: false,
    still: 0,
  }));
  const poses: Pose[][] = [];
  let hit = -1;
  const steps = Math.round(LONGEST / STEP);
  for (let s = 0; s < steps; s++) {
    const time = s * STEP;
    for (const b of bodies) if (!b.rested) move(b, time, half, random, () => (hit < 0 ? (hit = time) : hit));
    apart(bodies);
    for (const b of bodies) if (!b.rested && b.inside) within(b, half);
    for (const b of bodies) if (!b.rested && b.still > 0.12) rest(b, s, random);
    poses.push(bodies.map((b) => (b.rested ? b.rested.pose : { pos: b.pos.clone(), turn: b.turn.clone() })));
    if (bodies.every((b) => b.rested)) break;
  }
  // Whatever still rolls when time is up lies down where it is.
  for (const b of bodies) if (!b.rested) b.rested = { step: poses.length - 1, pose: flat(b) };
  // The last bit of tilt goes over a few steps — too little to be seen as a turn.
  bodies.forEach((b, i) => {
    const { step, pose } = b.rested!;
    const from = Math.max(0, step - SETTLE_STEPS);
    for (let s = from; s < poses.length; s++) {
      const k = s >= step ? 1 : (s - from) / (step - from);
      const p = poses[s][i];
      poses[s][i] = { pos: p.pos.clone().lerp(pose.pos, k), turn: p.turn.clone().slerp(pose.turn, k) };
    }
  });
  return { poses, hit: Math.max(0, hit) };
}

function move(b: Body, time: number, half: { x: number; z: number }, random: () => number, landed: () => void) {
  b.vel.y -= GRAVITY * STEP;
  b.pos.addScaledVector(b.vel, STEP);
  const angle = b.spin.length() * STEP;
  if (angle > 0) b.turn.premultiply(new THREE.Quaternion().setFromAxisAngle(b.spin.clone().normalize(), angle)).normalize();
  const corners = () => b.corners.map((c) => c.clone().applyQuaternion(b.turn));
  // The table.
  let lowest = Math.min(...corners().map((c) => c.y + b.pos.y));
  let touching = false;
  if (lowest < 0) {
    landed();
    b.pos.y -= lowest;
    touching = true;
    for (let i = 0; i < 3; i++) for (const r of corners()) if (r.y + b.pos.y < 0.002) contact(b, r, UP, BOUNCE);
  }
  // The window's edges — once a die has come in over them.
  for (const axis of ["x", "z"] as const) {
    const edge = half[axis];
    if (Math.abs(b.pos[axis]) < edge - b.radius) b.inside = true;
    if (!b.inside) continue;
    for (const side of [-1, 1]) {
      const over = Math.max(...corners().map((r) => side * (r[axis] + b.pos[axis]) - edge));
      if (over <= 0) continue;
      b.pos[axis] -= side * over;
      const n = new THREE.Vector3();
      n[axis] = -side;
      for (const r of corners()) if (side * (r[axis] + b.pos[axis]) > edge - 0.002) contact(b, r, n, WALL_BOUNCE);
    }
  }
  // Air and the table's cloth take speed; late in the throw ever more.
  const late = Math.max(0, time - SLOW_FROM) * 5;
  b.spin.multiplyScalar(Math.exp(-(0.6 + (touching ? 2.5 : 0) + late) * STEP));
  const drag = Math.exp(-((touching ? 1.2 : 0.1) + late) * STEP);
  b.vel.x *= drag;
  b.vel.z *= drag;
  lowest = Math.min(...corners().map((c) => c.y + b.pos.y));
  b.still = lowest < 0.01 && b.vel.length() < 0.2 && b.spin.length() < 0.6 ? b.still + STEP : 0;
  // Balanced on an edge: a nudge, as a table would give.
  if (b.still > 0.1 && !lies(b) && random() < 0.2) b.spin.add(new THREE.Vector3(random() - 0.5, 0, random() - 0.5).multiplyScalar(6));
}

/** Whether a face lies on the table. */
function lies(b: Body): boolean {
  return b.normals.some((n) => n.clone().applyQuaternion(b.turn).y < -0.99);
}

function rest(b: Body, step: number, random: () => number) {
  if (!lies(b)) {
    b.still = 0;
    b.spin.add(new THREE.Vector3(random() - 0.5, 0, random() - 0.5).multiplyScalar(8));
    return;
  }
  b.rested = { step, pose: flat(b) };
  b.vel.set(0, 0, 0);
  b.spin.set(0, 0, 0);
}

/** The die on the face nearest the table, exactly flat. */
function flat(b: Body): Pose {
  const world = b.normals.map((n) => n.clone().applyQuaternion(b.turn));
  const down = world.reduce((a, n) => (n.y < a.y ? n : a));
  const turn = new THREE.Quaternion().setFromUnitVectors(down, UP.clone().negate()).multiply(b.turn);
  const local = b.normals[world.indexOf(down)];
  const height = Math.max(...b.corners.map((c) => c.dot(local)));
  return { pos: new THREE.Vector3(b.pos.x, height, b.pos.z), turn };
}

/** Knocked apart, a die may not leave the table over its edge. */
function within(b: Body, half: { x: number; z: number }) {
  for (const axis of ["x", "z"] as const) {
    const reach = Math.max(...b.corners.map((c) => Math.abs(c.clone().applyQuaternion(b.turn)[axis])));
    const edge = half[axis] - reach;
    b.pos[axis] = THREE.MathUtils.clamp(b.pos[axis], -edge, edge);
  }
}

/** Dice do not lie in one another: they knock apart. */
function apart(bodies: Body[]) {
  for (let i = 0; i < bodies.length; i++)
    for (let j = i + 1; j < bodies.length; j++) {
      const a = bodies[i];
      const b = bodies[j];
      const d = new THREE.Vector3().subVectors(b.pos, a.pos);
      const gap = (a.radius + b.radius) * 0.85 - d.length();
      if (gap <= 0) continue;
      const n = d.lengthSq() > 1e-9 ? d.normalize() : new THREE.Vector3(1, 0, 0);
      n.y = 0;
      if (n.lengthSq() < 1e-9) n.set(1, 0, 0);
      n.normalize();
      // A die at rest is pushed as little as it can be: it yields.
      const share = a.rested ? 0 : b.rested ? 1 : 0.5;
      a.pos.addScaledVector(n, -gap * share);
      b.pos.addScaledVector(n, gap * (1 - share));
      if (a.rested || b.rested) continue;
      const closing = new THREE.Vector3().subVectors(a.vel, b.vel).dot(n);
      if (closing <= 0) continue;
      const j2 = ((1 + 0.4) * closing) / 2;
      a.vel.addScaledVector(n, -j2);
      b.vel.addScaledVector(n, j2);
      a.spin.add(new THREE.Vector3(-n.z, 0, n.x).multiplyScalar(j2 * 2));
      b.spin.add(new THREE.Vector3(n.z, 0, -n.x).multiplyScalar(j2 * 2));
    }
}
