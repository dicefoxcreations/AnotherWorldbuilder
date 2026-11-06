// The 3D dice: thrown, they roll until they lie flat, and any face can be
// turned to the top without the die looking any different.
import assert from "node:assert/strict";
import test from "node:test";
import * as THREE from "three";
import { LONGEST, STEP, simulate } from "../../src/features/dice/DicePhysics.ts";
import { facesOf, shapeOf, symmetries } from "../../src/features/dice/DiceShapes.ts";

/** The same throws on every run. */
function seeded(seed) {
  return () => ((seed = (seed * 1664525 + 1013904223) % 4294967296) / 4294967296);
}

const KINDS = [4, 6, 8, 10, 12, 20];

test("each shape turns onto itself so that any face can lie on top", () => {
  const sizes = { 4: 12, 6: 24, 8: 24, 10: 10, 12: 60, 20: 60 };
  for (const sides of KINDS) {
    const shape = shapeOf(sides);
    const turns = symmetries(shape.vertices);
    assert.equal(turns.length, sizes[sides], `d${sides}`);
    const faces = facesOf(shape);
    for (const f of faces)
      assert.ok(turns.some((t) => f.normal.clone().applyQuaternion(t).dot(faces[0].normal) > 0.999), `d${sides}: a face no turn brings up`);
  }
});

test("thrown dice come to rest flat on a face, on the table, in time", () => {
  const random = seeded(7);
  const half = { x: 9, z: 5 };
  for (const sides of KINDS) {
    const shape = shapeOf(sides);
    const normals = facesOf(shape).map((f) => f.normal);
    for (let round = 0; round < 12; round++) {
      const count = 1 + Math.floor(random() * 10);
      const from = random() < 0.5 ? -1 : 1;
      const thrown = Array.from({ length: count }, (_, i) => ({
        corners: shape.vertices.map((c) => c.clone().multiplyScalar(0.6)),
        normals,
        radius: 0.6,
        pos: new THREE.Vector3(from * (half.x + 1.2), 1.2 + random() * 1.2, (i - count / 2) * 1.3),
        vel: new THREE.Vector3(-from * (10 + random() * 5), 1 + random() * 2, (random() - 0.5) * 6),
        turn: new THREE.Quaternion().setFromEuler(new THREE.Euler(random() * 6, random() * 6, random() * 6)),
        spin: new THREE.Vector3(random() - 0.5, random() - 0.5, random() - 0.5).multiplyScalar(28),
      }));
      const track = simulate(thrown, half, random);
      const seconds = track.poses.length * STEP;
      assert.ok(seconds < LONGEST - 0.05, `d${sides}: still rolling after ${seconds.toFixed(2)} s`);
      for (const pose of track.poses.at(-1)) {
        const down = Math.min(...normals.map((n) => n.clone().applyQuaternion(pose.turn).y));
        assert.ok(down < -0.9999, `d${sides}: not flat (${down})`);
        const lowest = Math.min(...thrown[0].corners.map((c) => c.clone().applyQuaternion(pose.turn).y + pose.pos.y));
        assert.ok(Math.abs(lowest) < 1e-6, `d${sides}: not on the table (${lowest})`);
        for (const c of thrown[0].corners) {
          const at = c.clone().applyQuaternion(pose.turn).add(pose.pos);
          assert.ok(Math.abs(at.x) <= half.x + 1e-6 && Math.abs(at.z) <= half.z + 1e-6, `d${sides}: over the edge`);
        }
      }
      assert.ok(track.hit > 0 && track.hit < 0.6, `d${sides}: lands at ${track.hit}`);
    }
  }
});
