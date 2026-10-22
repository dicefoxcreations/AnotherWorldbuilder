import * as THREE from "three";

/**
 * Dice that tumble across the window and come to rest showing what was
 * rolled.
 *
 * The app rolls first (`shared/Dice`, with the world's rules); the dice only
 * show it. Each one is thrown, bounces and spins on its own, and in its last
 * moments turns onto the face it rolled — so what lies on the table is
 * always what the log says. Nothing is loaded from outside: the shapes are
 * built here, the numbers drawn onto them in the reader's colours.
 */

/** The colours of the dice — this device's, see `DiceArea`. */
export type DiceLook = { die: string; number: string };

/** A die to show: its sides and what it shows. */
export type ShownDie = { sides: number; value: number };

/** The dice that have a shape here. */
export const SHAPED = [4, 6, 8, 10, 12, 20, 100];

type Face = {
  /** Outward normal, in the die's own space. */
  normal: THREE.Vector3;
  /** Which way its number stands upright, in its plane. */
  up: THREE.Vector3;
  /** The triangles' corners that make it. */
  corners: THREE.Vector3[];
  /** The die's corners it touches — a d4 numbers its corners. */
  vertices: number[];
};

type Shape = { triangles: THREE.Vector3[][]; vertices: THREE.Vector3[] };

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

function shapeOf(sides: number): Shape {
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
function facesOf(shape: Shape): Face[] {
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

function middle(f: Face): THREE.Vector3 {
  const unique: THREE.Vector3[] = [];
  for (const p of f.corners) if (!unique.some((q) => q.distanceTo(p) < 1e-6)) unique.push(p);
  return unique.reduce((m, p) => m.add(p), new THREE.Vector3()).divideScalar(unique.length);
}

/**
 * What each face shows: opposite faces add up as on a real die (1 and 6, 1
 * and 20). A d10 counts from 0, its tens die in tens.
 */
function labels(faces: Face[], sides: number, tens: boolean): string[] {
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

// --- Drawing ----------------------------------------------------------------

const TEXTURE = 256;

function faceTexture(look: DiceLook, text: string, corners?: { text: string; at: [number, number] }[]): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = TEXTURE;
  const g = canvas.getContext("2d")!;
  g.fillStyle = look.die;
  g.fillRect(0, 0, TEXTURE, TEXTURE);
  g.fillStyle = look.number;
  g.textAlign = "center";
  g.textBaseline = "middle";
  const font = (px: number) => `700 ${px}px system-ui, "Segoe UI", sans-serif`;
  if (corners) {
    // A d4: each corner's number near it, upright toward it.
    g.font = font(72);
    for (const c of corners) {
      const [x, y] = c.at;
      g.save();
      g.translate(x * TEXTURE, y * TEXTURE);
      g.rotate(Math.atan2(y - 0.5, x - 0.5) + Math.PI / 2);
      g.fillText(c.text, 0, 0);
      g.restore();
    }
  } else {
    const size = text.length > 1 ? 96 : 120;
    g.font = font(size);
    g.fillText(text, TEXTURE / 2, TEXTURE / 2 + 4);
    // 6 and 9 are told apart by a line beneath.
    if (text === "6" || text === "9") g.fillRect(TEXTURE / 2 - 26, TEXTURE / 2 + size / 2, 52, 9);
  }
  const t = new THREE.CanvasTexture(canvas);
  t.colorSpace = THREE.SRGBColorSpace;
  t.anisotropy = 4;
  return t;
}

/** A die as a mesh, and where each value lies: the direction that must face up. */
function buildDie(sides: number, look: DiceLook, tens = false): { mesh: THREE.Mesh; upFor: (value: string) => { up: THREE.Vector3; text: THREE.Vector3 } | null; rest: number } {
  const shape = shapeOf(sides === 100 ? 10 : sides);
  const faces = facesOf(shape);
  const geometry = new THREE.BufferGeometry();
  const positions: number[] = [];
  const uvs: number[] = [];
  const materials: THREE.Material[] = [];
  const vertexLabels = shape.vertices.map((_, i) => String(i + 1));
  const faceLabels = sides === 4 ? [] : labels(faces, sides === 100 ? 10 : sides, tens);
  faces.forEach((f, i) => {
    const mid = middle(f);
    const side = new THREE.Vector3().crossVectors(f.up, f.normal);
    const radius = Math.max(...f.corners.map((p) => p.distanceTo(mid)));
    const uv = (p: THREE.Vector3): [number, number] => {
      const d = new THREE.Vector3().subVectors(p, mid);
      return [0.5 + (d.dot(side) / radius) * 0.5, 0.5 + (d.dot(f.up) / radius) * 0.5];
    };
    geometry.addGroup(positions.length / 3, f.corners.length, i);
    for (const p of f.corners) {
      positions.push(p.x, p.y, p.z);
      uvs.push(...uv(p));
    }
    const texture =
      sides === 4
        ? faceTexture(look, "", f.vertices.map((v) => {
            const [x, y] = uv(shape.vertices[v]);
            return { text: vertexLabels[v], at: [0.5 + (x - 0.5) * 0.58, 1 - (0.5 + (y - 0.5) * 0.58)] as [number, number] };
          }))
        : faceTexture(look, faceLabels[i]);
    materials.push(new THREE.MeshStandardMaterial({ map: texture, roughness: 0.45, metalness: 0.05, flatShading: true }));
  });
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
  geometry.computeVertexNormals();
  const mesh = new THREE.Mesh(geometry, materials);
  mesh.castShadow = true;
  // How high its middle rests above the table: its faces' distance from it.
  const rest = Math.min(...faces.map((f) => middle(f).dot(f.normal)));
  const upFor = (value: string) => {
    if (sides === 4) {
      // A d4 shows the number of the corner on top.
      const i = vertexLabels.indexOf(value);
      if (i < 0) return null;
      const up = shape.vertices[i].clone().normalize();
      const face = faces.find((f) => !f.vertices.includes(i))!;
      return { up, text: new THREE.Vector3().subVectors(shape.vertices[face.vertices[0]], middle(face)).projectOnPlane(up).normalize() };
    }
    const i = faceLabels.indexOf(value);
    return i < 0 ? null : { up: faces[i].normal.clone(), text: faces[i].up.clone() };
  };
  return { mesh, upFor, rest };
}

// --- The table ----------------------------------------------------------------

type Moving = {
  mesh: THREE.Mesh;
  pos: THREE.Vector3;
  vel: THREE.Vector3;
  spin: THREE.Vector3;
  turn: THREE.Quaternion;
  target: THREE.Quaternion;
  rest: number;
};

const DIE = 0.6;
/** How much larger than the ball around it a shape is drawn. */
const SIZE: Record<number, number> = { 4: 1.35, 8: 1.12, 10: 1.05, 100: 1.05 };
const THROW_MS = 1500;
const SETTLE_FROM = 0.55;
const HOLD_MS = 1600;
const FADE_MS = 450;

/**
 * The table: one canvas over the whole window, there only while dice roll.
 * Pointer and keys go through it to the app.
 */
export class DiceTable {
  private renderer: THREE.WebGLRenderer | null = null;
  private scene = new THREE.Scene();
  // Narrow and high: little perspective, so dice at the edges lie as flat as in the middle.
  private camera = new THREE.PerspectiveCamera(20, 1, 0.1, 100);
  private dice: Moving[] = [];
  private frame = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private half = { x: 6, z: 4 };

  constructor() {
    this.scene.add(new THREE.HemisphereLight(0xffffff, 0x445566, 1.6));
    const sun = new THREE.DirectionalLight(0xffffff, 1.8);
    sun.position.set(-4, 12, 6);
    sun.castShadow = true;
    sun.shadow.mapSize.set(1024, 1024);
    Object.assign(sun.shadow.camera, { left: -14, right: 14, top: 10, bottom: -10 });
    this.scene.add(sun);
    const floor = new THREE.Mesh(new THREE.PlaneGeometry(60, 60), new THREE.ShadowMaterial({ opacity: 0.28 }));
    floor.rotation.x = -Math.PI / 2;
    floor.receiveShadow = true;
    this.scene.add(floor);
  }

  private canvas(): THREE.WebGLRenderer {
    if (this.renderer) return this.renderer;
    const r = new THREE.WebGLRenderer({ alpha: true, antialias: true });
    r.setPixelRatio(Math.min(2, window.devicePixelRatio));
    r.shadowMap.enabled = true;
    r.shadowMap.type = THREE.PCFSoftShadowMap;
    r.domElement.className = "dice-table";
    document.body.appendChild(r.domElement);
    this.renderer = r;
    return r;
  }

  private fit() {
    const r = this.canvas();
    const w = window.innerWidth;
    const h = window.innerHeight;
    r.setSize(w, h);
    this.camera.aspect = w / h;
    // Straight down, the window's top at the far side.
    const height = 26;
    this.camera.position.set(0, height, 0);
    this.camera.up.set(0, 0, -1);
    this.camera.lookAt(0, 0, 0);
    this.camera.updateProjectionMatrix();
    const z = Math.tan(THREE.MathUtils.degToRad(this.camera.fov / 2)) * height;
    this.half = { x: z * this.camera.aspect - DIE, z: z - DIE };
  }

  /** Throws these dice; they come to rest showing their values. */
  throw(dice: ShownDie[], look: DiceLook) {
    const shown = dice.flatMap((d): { sides: number; label: string; tens?: boolean }[] => {
      if (d.sides === 100) {
        const v = d.value % 100;
        return [
          { sides: 100, label: `${Math.floor(v / 10)}0`.replace(/^00$/, "00"), tens: true },
          { sides: 10, label: String(v % 10) },
        ];
      }
      if (!SHAPED.includes(d.sides)) return [];
      return [{ sides: d.sides, label: d.sides === 10 ? String(d.value % 10) : String(d.value) }];
    }).slice(0, 24);
    if (!shown.length) return;
    this.clear();
    this.fit();
    const r = this.canvas();
    r.domElement.classList.remove("fading");
    // From one side of the window, toward the middle.
    const from = Math.random() < 0.5 ? -1 : 1;
    shown.forEach((d, i) => {
      const built = buildDie(d.sides, look, d.tens);
      // The small shapes a little larger, so all look alike in size.
      const size = DIE * (SIZE[d.sides] ?? 1);
      built.mesh.scale.setScalar(size);
      const where = built.upFor(d.label);
      const turn = new THREE.Quaternion().setFromEuler(new THREE.Euler(Math.random() * 6, Math.random() * 6, Math.random() * 6));
      const target = new THREE.Quaternion();
      if (where) {
        target.setFromUnitVectors(where.up, new THREE.Vector3(0, 1, 0));
        // Its number upright to the reader, give or take a little.
        const text = where.text.clone().applyQuaternion(target);
        const yaw = Math.atan2(text.x, -text.z) + (Math.random() - 0.5) * 0.5;
        target.premultiply(new THREE.Quaternion().setFromAxisAngle(new THREE.Vector3(0, 1, 0), yaw));
      }
      const lane = THREE.MathUtils.clamp((i - (shown.length - 1) / 2) * 1.3 + (Math.random() - 0.5), -this.half.z, this.half.z);
      const pos = new THREE.Vector3(from * (this.half.x + 1.5), 2 + Math.random() * 2, lane);
      const vel = new THREE.Vector3(-from * (9 + Math.random() * 5), 2 + Math.random() * 3, (Math.random() - 0.5) * 6);
      const spin = new THREE.Vector3(Math.random() - 0.5, Math.random() - 0.5, Math.random() - 0.5).multiplyScalar(26);
      built.mesh.position.copy(pos);
      built.mesh.quaternion.copy(turn);
      this.scene.add(built.mesh);
      this.dice.push({ mesh: built.mesh, pos, vel, spin, turn, target, rest: built.rest * size });
    });
    const start = performance.now();
    let last = start;
    const step = (now: number) => {
      const t = now - start;
      const dt = Math.min(0.035, (now - last) / 1000);
      last = now;
      const settle = Math.min(1, Math.max(0, (t / THROW_MS - SETTLE_FROM) / (1 - SETTLE_FROM)));
      const ease = settle * settle * (3 - 2 * settle);
      for (const d of this.dice) this.move(d, dt, ease);
      this.apart();
      r.render(this.scene, this.camera);
      if (t < THROW_MS) this.frame = requestAnimationFrame(step);
      else this.timer = setTimeout(() => this.fade(), HOLD_MS);
    };
    this.frame = requestAnimationFrame(step);
  }

  private move(d: Moving, dt: number, ease: number) {
    d.vel.y -= 40 * dt;
    d.pos.addScaledVector(d.vel, dt * (1 - ease * 0.9));
    if (d.pos.y < d.rest) {
      d.pos.y = d.rest;
      d.vel.y = Math.abs(d.vel.y) * 0.42;
      d.vel.x *= 0.78;
      d.vel.z *= 0.78;
      d.spin.multiplyScalar(0.72);
    }
    for (const axis of ["x", "z"] as const) {
      const edge = this.half[axis];
      // Off the edge and still going: back. Coming in from beyond it is no bounce.
      if (Math.abs(d.pos[axis]) > edge && Math.sign(d.pos[axis]) === Math.sign(d.vel[axis])) d.vel[axis] *= -0.6;
    }
    const angle = d.spin.length() * dt;
    if (angle > 0) d.turn.premultiply(new THREE.Quaternion().setFromAxisAngle(d.spin.clone().normalize(), angle));
    d.mesh.quaternion.copy(d.turn).slerp(d.target, ease);
    d.mesh.position.set(d.pos.x, THREE.MathUtils.lerp(d.pos.y, d.rest, ease), d.pos.z);
  }

  /** Dice do not lie in one another. */
  private apart() {
    for (let i = 0; i < this.dice.length; i++)
      for (let j = i + 1; j < this.dice.length; j++) {
        const a = this.dice[i].pos;
        const b = this.dice[j].pos;
        const d = new THREE.Vector3(b.x - a.x, 0, b.z - a.z);
        const gap = DIE * 2.1 - d.length();
        if (gap > 0) {
          const push = (d.lengthSq() ? d.normalize() : new THREE.Vector3(1, 0, 0)).multiplyScalar(gap / 2);
          a.sub(push);
          b.add(push);
        }
      }
  }

  private fade() {
    const el = this.renderer?.domElement;
    if (!el) return;
    el.classList.add("fading");
    this.timer = setTimeout(() => this.clear(), FADE_MS);
  }

  /** Takes the dice off the table. */
  clear() {
    cancelAnimationFrame(this.frame);
    clearTimeout(this.timer);
    for (const d of this.dice) {
      this.scene.remove(d.mesh);
      d.mesh.geometry.dispose();
      for (const m of d.mesh.material as THREE.MeshStandardMaterial[]) {
        m.map?.dispose();
        m.dispose();
      }
    }
    this.dice = [];
    if (this.renderer) {
      this.renderer.render(this.scene, this.camera);
      this.renderer.domElement.classList.remove("fading");
    }
  }
}
