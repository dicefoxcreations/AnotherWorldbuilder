import * as THREE from "three";
import { RoomEnvironment } from "three/examples/jsm/environments/RoomEnvironment.js";
import { STEP, simulate, type Thrown, type Track } from "./DicePhysics";
import { facesOf, labels, middle, shapeOf, symmetries } from "./DiceShapes";

/**
 * Dice that tumble across the window and come to rest showing what was
 * rolled.
 *
 * The app rolls first (`shared/Dice`, with the world's rules); the dice only
 * show it. The throw is worked out in full before it is shown
 * (`DicePhysics`): each die flies, bounces and rolls until it lies on a face.
 * Then its numbers are put on so that this face shows what was rolled — a
 * die looks the same from every side it can lie on, so nothing turns at the
 * end and what lies on the table is always what the log says. Nothing is
 * loaded from outside: the shapes are built here, the numbers drawn onto
 * them in the reader's colours.
 */

/** The colours of the dice — this device's, see `DiceArea`. */
export type DiceLook = { die: string; number: string };

/** A die to show: its sides and what it shows. */
export type ShownDie = { sides: number; value: number };

/** The dice that have a shape here. */
export const SHAPED = [4, 6, 8, 10, 12, 20, 100];

// --- Drawing ----------------------------------------------------------------

const TEXTURE = 256;

/** A colour a share of the way toward another. */
function mix(a: string, b: string, k: number): string {
  return `#${new THREE.Color(a).lerp(new THREE.Color(b), k).getHexString()}`;
}

function faceTexture(look: DiceLook, text: string, corners?: { text: string; at: [number, number] }[]): THREE.CanvasTexture {
  const canvas = document.createElement("canvas");
  canvas.width = canvas.height = TEXTURE;
  const g = canvas.getContext("2d")!;
  const c = TEXTURE / 2;
  // Lighter in the middle, darker toward the edges: a face with worn, rounded rims.
  const shade = g.createRadialGradient(c, c, TEXTURE * 0.08, c, c, TEXTURE * 0.5);
  shade.addColorStop(0, mix(look.die, "#ffffff", 0.1));
  shade.addColorStop(0.62, look.die);
  shade.addColorStop(1, mix(look.die, "#000000", 0.28));
  g.fillStyle = shade;
  g.fillRect(0, 0, TEXTURE, TEXTURE);
  g.textAlign = "center";
  g.textBaseline = "middle";
  const font = (px: number) => `800 ${px}px Georgia, "Times New Roman", serif`;
  // Cut into the die: a dark rim above the number, a light one below.
  const cut = (write: (dx: number, dy: number) => void) => {
    g.fillStyle = mix(look.die, "#000000", 0.45);
    write(-2, -2);
    g.fillStyle = mix(look.die, "#ffffff", 0.35);
    write(2, 2);
    g.fillStyle = look.number;
    write(0, 0);
  };
  if (corners) {
    // A d4: each corner's number near it, upright toward it.
    g.font = font(70);
    for (const k of corners) {
      const [x, y] = k.at;
      cut((dx, dy) => {
        g.save();
        g.translate(x * TEXTURE + dx, y * TEXTURE + dy);
        g.rotate(Math.atan2(y - 0.5, x - 0.5) + Math.PI / 2);
        g.fillText(k.text, 0, 0);
        g.restore();
      });
    }
  } else {
    const size = text.length > 1 ? 96 : 124;
    g.font = font(size);
    cut((dx, dy) => {
      g.fillText(text, c + dx, c + 6 + dy);
      // 6 and 9 are told apart by a line beneath.
      if (text === "6" || text === "9") g.fillRect(c - 26 + dx, c + size / 2 + dy, 52, 9);
    });
  }
  const t = new THREE.CanvasTexture(canvas);
  t.colorSpace = THREE.SRGBColorSpace;
  t.anisotropy = 4;
  return t;
}

type Built = {
  mesh: THREE.Mesh;
  /** Where a value lies: the direction that must face up, and which way its number reads. */
  upFor: (value: string) => { up: THREE.Vector3; text: THREE.Vector3 } | null;
  corners: THREE.Vector3[];
  normals: THREE.Vector3[];
};

/** The turns that leave each shape as it is, worked out once. */
const turnsOf = new Map<number, THREE.Quaternion[]>();

/** A die as a mesh, and where each value lies. */
function buildDie(sides: number, look: DiceLook, tens = false): Built {
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
    materials.push(
      new THREE.MeshPhysicalMaterial({ map: texture, roughness: 0.38, metalness: 0, clearcoat: 0.45, clearcoatRoughness: 0.3, flatShading: true }),
    );
  });
  geometry.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
  geometry.setAttribute("uv", new THREE.Float32BufferAttribute(uvs, 2));
  geometry.computeVertexNormals();
  const mesh = new THREE.Mesh(geometry, materials);
  mesh.castShadow = true;
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
  if (!turnsOf.has(sides)) turnsOf.set(sides, symmetries(shape.vertices));
  return { mesh, upFor, corners: shape.vertices, normals: faces.map((f) => f.normal) };
}

/**
 * How to put the numbers on so that the face on top at the end shows `value`:
 * a turn of the die onto itself, among those the one that leaves the number
 * most upright to the reader.
 */
function numbering(built: Built, sides: number, value: string, end: THREE.Quaternion): THREE.Quaternion {
  const want = built.upFor(value);
  if (!want) return new THREE.Quaternion();
  const up = new THREE.Vector3(0, 1, 0).applyQuaternion(end.clone().invert());
  let best = new THREE.Quaternion();
  let score = -Infinity;
  for (const turn of turnsOf.get(sides) ?? []) {
    if (want.up.clone().applyQuaternion(turn).dot(up) < 0.99) continue;
    // The camera looks down with the window's top at -z.
    const reads = -want.text.clone().applyQuaternion(turn).applyQuaternion(end).z;
    if (reads > score) {
      score = reads;
      best = turn;
    }
  }
  return best;
}

// --- The table ----------------------------------------------------------------

type Shown = { mesh: THREE.Mesh; numbering: THREE.Quaternion };

const DIE = 0.6;
/** How much larger than the ball around it a shape is drawn. */
const SIZE: Record<number, number> = { 4: 1.35, 8: 1.12, 10: 1.05, 100: 1.05 };
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
  private dice: Shown[] = [];
  private frame = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private half = { x: 6, z: 4 };

  constructor() {
    this.scene.add(new THREE.HemisphereLight(0xffffff, 0x445566, 1.1));
    const sun = new THREE.DirectionalLight(0xffffff, 2);
    sun.position.set(-4, 12, 6);
    sun.castShadow = true;
    sun.shadow.mapSize.set(1024, 1024);
    sun.shadow.radius = 4;
    Object.assign(sun.shadow.camera, { left: -14, right: 14, top: 10, bottom: -10 });
    this.scene.add(sun);
    const floor = new THREE.Mesh(new THREE.PlaneGeometry(60, 60), new THREE.ShadowMaterial({ opacity: 0.3 }));
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
    // A soft room to shine in: the gloss on the dice.
    const room = new THREE.PMREMGenerator(r);
    this.scene.environment = room.fromScene(new RoomEnvironment(), 0.04).texture;
    this.scene.environmentIntensity = 0.25;
    room.dispose();
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
    this.half = { x: z * this.camera.aspect, z };
  }

  /**
   * Throws these dice; they come to rest showing their values. Gives the
   * milliseconds until the first one hits the table, or null if none has a
   * shape here.
   */
  throw(dice: ShownDie[], look: DiceLook): number | null {
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
    if (!shown.length) return null;
    this.clear();
    this.fit();
    const r = this.canvas();
    r.domElement.classList.remove("fading");
    // From one side of the window, toward the middle, low over the table.
    const from = Math.random() < 0.5 ? -1 : 1;
    const built = shown.map((d) => buildDie(d.sides, look, d.tens));
    const thrown: Thrown[] = shown.map((d, i) => {
      // The small shapes a little larger, so all look alike in size.
      const size = DIE * (SIZE[d.sides] ?? 1);
      const lane = THREE.MathUtils.clamp((i - (shown.length - 1) / 2) * 1.3 + (Math.random() - 0.5), -this.half.z + 1, this.half.z - 1);
      return {
        corners: built[i].corners.map((c) => c.clone().multiplyScalar(size)),
        normals: built[i].normals,
        radius: size,
        pos: new THREE.Vector3(from * (this.half.x + 1.2), 1.2 + Math.random() * 1.2, lane),
        vel: new THREE.Vector3(-from * (10 + Math.random() * 5), 1 + Math.random() * 2, (Math.random() - 0.5) * 6),
        turn: new THREE.Quaternion().setFromEuler(new THREE.Euler(Math.random() * 6, Math.random() * 6, Math.random() * 6)),
        spin: new THREE.Vector3(Math.random() - 0.5, Math.random() - 0.5, Math.random() - 0.5).multiplyScalar(28),
      };
    });
    const track: Track = simulate(thrown, this.half);
    const end = track.poses[track.poses.length - 1];
    shown.forEach((d, i) => {
      const { mesh } = built[i];
      mesh.scale.setScalar(thrown[i].radius);
      this.scene.add(mesh);
      this.dice.push({ mesh, numbering: numbering(built[i], d.sides, d.label, end[i].turn) });
    });
    const start = performance.now();
    const step = (now: number) => {
      const at = Math.min(track.poses.length - 1, Math.max(0, Math.floor((now - start) / 1000 / STEP)));
      track.poses[at].forEach((pose, i) => {
        const d = this.dice[i];
        d.mesh.position.copy(pose.pos);
        d.mesh.quaternion.copy(pose.turn).multiply(d.numbering);
      });
      r.render(this.scene, this.camera);
      if (at < track.poses.length - 1) this.frame = requestAnimationFrame(step);
      else this.timer = setTimeout(() => this.fade(), HOLD_MS);
    };
    this.frame = requestAnimationFrame(step);
    return track.hit * 1000;
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
      for (const m of d.mesh.material as THREE.MeshPhysicalMaterial[]) {
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
