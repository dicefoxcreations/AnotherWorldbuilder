import * as THREE from "three";

/**
 * The shapes of the dice, built from their corners, and what each face shows.
 * No drawing here — the table (`Dice3D`) and the throw (`DicePhysics`) both
 * use them.
 */

export type Face = {
  /** Outward normal, in the die's own space. */
  normal: THREE.Vector3;
  /** Which way its number stands upright, in its plane. */
  up: THREE.Vector3;
  /** The triangles' corners that make it. */
  corners: THREE.Vector3[];
  /** The die's corners it touches — a d4 numbers its corners. */
  vertices: number[];
};

export type Shape = { triangles: THREE.Vector3[][]; vertices: THREE.Vector3[] };

// --- Shapes -------------------------------------------------------------------

const PHI = (1 + Math.sqrt(5)) / 2;

/** Faces of a convex shape, from its corners: every plane all corners lie behind. */
function hull(vertices: number[][]): Shape {
  const v = vertices.map(([x, y, z]) => new THREE.Vector3(x, y, z));
  const faces: number[][] = [];
  const seen = new Set<string>();
  for (let a = 0; a < v.length; a++)
    for (let b = a + 1; b < v.length; b++)
      for (let c = b + 1; c < v.length; c++) {
        const n = new THREE.Vector3().subVectors(v[b], v[a]).cross(new THREE.Vector3().subVectors(v[c], v[a]));
        if (n.lengthSq() < 1e-9) continue;
        n.normalize();
        const d = n.dot(v[a]);
        const side = v.map((p) => n.dot(p) - d);
        let sign = 0;
        if (side.every((s) => s <= 1e-6)) sign = 1;
        else if (side.every((s) => s >= -1e-6)) sign = -1;
        if (!sign) continue;
        const normal = n.clone().multiplyScalar(sign);
        const key = normal.toArray().map((x) => x.toFixed(3)).join(",");
        if (seen.has(key)) continue;
        seen.add(key);
        // The corners on that plane, around its middle.
        const on = v.map((_, i) => i).filter((i) => Math.abs(side[i]) < 1e-6);
        const mid = on.reduce((m, i) => m.add(v[i]), new THREE.Vector3()).divideScalar(on.length);
        const u = new THREE.Vector3().subVectors(v[on[0]], mid).normalize();
        const w = new THREE.Vector3().crossVectors(normal, u);
        on.sort((i, j) => {
          const pi = new THREE.Vector3().subVectors(v[i], mid);
          const pj = new THREE.Vector3().subVectors(v[j], mid);
          return Math.atan2(pi.dot(w), pi.dot(u)) - Math.atan2(pj.dot(w), pj.dot(u));
        });
        faces.push(on);
      }
  // As large as a ball of radius 1 around it.
  const scale = Math.max(...v.map((p) => p.length()));
  const at = v.map((p) => p.clone().divideScalar(scale));
  const triangles = faces.flatMap((f) => f.slice(1, -1).map((_, i) => [at[f[0]], at[f[i + 1]], at[f[i + 2]]]));
  return { triangles, vertices: at };
}

export function shapeOf(sides: number): Shape {
  switch (sides) {
    case 4:
      return hull([
        [1, 1, 1],
        [-1, -1, 1],
        [-1, 1, -1],
        [1, -1, -1],
      ]);
    case 6:
      return hull([-1, 1].flatMap((x) => [-1, 1].flatMap((y) => [-1, 1].map((z) => [x, y, z]))));
    case 8:
      return hull([
        [1, 0, 0],
        [-1, 0, 0],
        [0, 1, 0],
        [0, -1, 0],
        [0, 0, 1],
        [0, 0, -1],
      ]);
    case 12: {
      const a = 1 / PHI;
      return hull([
        ...[-1, 1].flatMap((x) => [-1, 1].flatMap((y) => [-1, 1].map((z) => [x, y, z]))),
        ...[-1, 1].flatMap((s) => [-1, 1].flatMap((t) => [
          [0, s * a, t * PHI],
          [s * a, t * PHI, 0],
          [s * PHI, 0, t * a],
        ])),
      ]);
    }
    case 20:
      return hull(
        [-1, 1].flatMap((s) => [-1, 1].flatMap((t) => [
          [0, s, t * PHI],
          [s, t * PHI, 0],
          [t * PHI, 0, s],
        ])),
      );
    default: {
      // d10: a pentagonal trapezohedron — two rings of five, a point above
      // and below, high enough that each kite is flat.
      const h = 0.11;
      const c = Math.cos(Math.PI / 5);
      const top = (h * (1 + c)) / (1 - c);
      const ring = (k: number, y: number, turn: number) => [Math.cos(((2 * k + turn) * Math.PI) / 5), y, Math.sin(((2 * k + turn) * Math.PI) / 5)];
      return hull([[0, top, 0], [0, -top, 0], ...[0, 1, 2, 3, 4].map((k) => ring(k, h, 0)), ...[0, 1, 2, 3, 4].map((k) => ring(k, -h, 1))]);
    }
  }
}

/** The faces of a shape: its triangles, grouped by the plane they lie in. */
export function facesOf(shape: Shape): Face[] {
  const faces: Face[] = [];
  for (const t of shape.triangles) {
    const normal = new THREE.Vector3().subVectors(t[1], t[0]).cross(new THREE.Vector3().subVectors(t[2], t[0])).normalize();
    let face = faces.find((f) => f.normal.dot(normal) > 0.999);
    if (!face) {
      face = { normal, up: new THREE.Vector3(), corners: [], vertices: [] };
      faces.push(face);
    }
    face.corners.push(...t);
    for (const p of t) {
      const i = shape.vertices.findIndex((v) => v.distanceTo(p) < 1e-6);
      if (i >= 0 && !face.vertices.includes(i)) face.vertices.push(i);
    }
  }
  for (const f of faces) {
    // Upright toward the corner farthest from its middle — the point of a kite.
    const mid = middle(f);
    const far = f.vertices.map((i) => shape.vertices[i]).reduce((a, b) => (b.distanceTo(mid) > a.distanceTo(mid) + 1e-6 ? b : a));
    f.up = new THREE.Vector3().subVectors(far, mid).projectOnPlane(f.normal).normalize();
  }
  return faces;
}

export function middle(f: Face): THREE.Vector3 {
  const unique: THREE.Vector3[] = [];
  for (const p of f.corners) if (!unique.some((q) => q.distanceTo(p) < 1e-6)) unique.push(p);
  return unique.reduce((m, p) => m.add(p), new THREE.Vector3()).divideScalar(unique.length);
}

/**
 * What each face shows: opposite faces add up as on a real die (1 and 6, 1
 * and 20). A d10 counts from 0, its tens die in tens.
 */
export function labels(faces: Face[], sides: number, tens: boolean): string[] {
  const n = faces.length;
  const out: string[] = new Array(n).fill("");
  const left = faces.map((_, i) => i);
  let low = 0;
  while (left.length) {
    const i = left.shift()!;
    const j = left.findIndex((k) => faces[k].normal.dot(faces[i].normal) < -0.999);
    const opposite = j >= 0 ? left.splice(j, 1)[0] : -1;
    const value = (v: number) => (sides === 10 ? (tens ? `${v}0`.replace(/^00$/, "00") : String(v)) : String(v + 1));
    out[i] = value(low);
    if (opposite >= 0) out[opposite] = value(n - 1 - low);
    low++;
  }
  return out;
}


/**
 * The turns that leave a shape as it is: every way to pick it up and put it
 * down again so that it looks the same. Any face can be turned onto any other.
 */
export function symmetries(vertices: THREE.Vector3[]): THREE.Quaternion[] {
  const frame = (a: THREE.Vector3, b: THREE.Vector3) => {
    const x = a.clone().normalize();
    const z = new THREE.Vector3().crossVectors(a, b).normalize();
    const y = new THREE.Vector3().crossVectors(z, x);
    return new THREE.Matrix4().makeBasis(x, y, z);
  };
  const a = vertices[0];
  const b = vertices.find((v) => new THREE.Vector3().crossVectors(a, v).lengthSq() > 1e-6)!;
  const from = frame(a, b).invert();
  const out: THREE.Quaternion[] = [];
  for (const a2 of vertices) {
    if (Math.abs(a2.length() - a.length()) > 1e-6) continue;
    for (const b2 of vertices) {
      if (Math.abs(b2.length() - b.length()) > 1e-6 || Math.abs(a2.dot(b2) - a.dot(b)) > 1e-6) continue;
      const turn = new THREE.Quaternion().setFromRotationMatrix(frame(a2, b2).multiply(from));
      const fits = vertices.every((v) => {
        const w = v.clone().applyQuaternion(turn);
        return vertices.some((u) => u.distanceTo(w) < 1e-5);
      });
      if (fits && !out.some((q) => Math.abs(q.dot(turn)) > 1 - 1e-9)) out.push(turn);
    }
  }
  return out;
}
