import assert from "node:assert/strict";
import test, { beforeEach } from "node:test";
import { registerHooks } from "node:module";

const transport = { worlds: [], calls: [], emitted: [], offline: false };
globalThis.__awOnlineTransport = transport;
globalThis.window = new EventTarget();

const core = `export async function invoke(command, args) {
  const state = globalThis.__awOnlineTransport;
  state.calls.push({ command, args });
  if (command === 'worlds_list') return state.worlds;
  if (state.offline && (command === 'world_call' || command === 'world_connect')) throw '[online] Server unavailable';
  if (command === 'world_connect') return state.worlds.find((w) => w.id === args.world);
  return [];
}`;
const events = `export async function emit(name, payload) { globalThis.__awOnlineTransport.emitted.push({name, payload}); }
export async function listen() { return () => {}; }`;
registerHooks({
  resolve(specifier, context, next) {
    const code = specifier === "@tauri-apps/api/core" ? core : specifier === "@tauri-apps/api/event" ? events : null;
    return code ? { url: `data:text/javascript,${encodeURIComponent(code)}`, shortCircuit: true } : next(specifier, context);
  },
});

const { api } = await import("../../src/api/client.ts");
const raw = (extra = {}) => ({
  id: "world:test", name: "Online test", icon: "🌍", public_key: "public", token_file: "", folder: "local-cache",
  role: "gm", is_gm: true, members: [], article_count: 1, server: "endpoint@server:7447", server_id: "endpoint",
  online: false, copies: false, ...extra,
});

beforeEach(() => {
  transport.calls.length = 0;
  transport.emitted.length = 0;
  transport.offline = false;
});

// Each case starts with an explicit connection; a failed case must not
// authorize the next one to silently resume a suspended world.
beforeEach(async () => {
  transport.worlds = [raw()];
  await api.worldConnect("world:test");
  transport.calls.length = 0;
});

test("a GM server world uses the online backend even if a cached folder exists", async () => {
  transport.worlds = [raw()];
  const [world] = await api.worldsList();
  assert.equal(world.online, true);
  await api.view(world.id);
  assert.deepEqual(transport.calls.map((c) => c.command), ["worlds_list", "world_call"]);
  assert.equal(transport.calls[1].args.command, "view");
});

test("removing a server address leaves the world online and never invokes local view", async () => {
  transport.worlds = [raw({ server: "" })];
  const [world] = await api.worldsList();
  assert.equal(world.online, true);
  transport.offline = true;
  await assert.rejects(api.view(world.id), (error) => String(error).includes("[online]"));
  assert.deepEqual(transport.calls.map((c) => c.command), ["worlds_list", "world_call"]);
});

test("retaining a GM copy does not authorize an offline server read", async () => {
  transport.worlds = [raw({ copies: true })];
  await api.worldsList();
  transport.offline = true;
  await assert.rejects(api.view("world:test"), (error) => String(error).includes("[online]"));
  assert.equal(transport.calls.at(-1).command, "world_call");
  assert.equal(transport.calls.filter((c) => c.command === "view").length, 0);
});

test("a failed player save raises the connection event and never retries locally or announces a save", async () => {
  transport.worlds = [raw({ role: "player", is_gm: false })];
  await api.worldsList();
  transport.offline = true;
  const lost = [];
  const listener = (event) => lost.push(event.detail);
  window.addEventListener("online-unavailable", listener);
  try {
    await assert.rejects(api.documentSave("world:test", "art:test", "unsaved", {}, "s"), (error) => String(error).includes("[online]"));
    await new Promise((resolve) => setTimeout(resolve, 300));
    assert.deepEqual(transport.calls.map((c) => c.command), ["worlds_list", "world_call"]);
    assert.equal(lost.length, 1);
    assert.equal(lost[0].world, "world:test");
    assert.equal(transport.emitted.length, 0);
  } finally {
    window.removeEventListener("online-unavailable", listener);
  }
});

test("an explicitly local world continues using the local backend", async () => {
  transport.worlds = [raw({ server: "", server_id: "", copies: false })];
  const [world] = await api.worldsList();
  assert.equal(world.online, false);
  await api.view(world.id);
  assert.deepEqual(transport.calls.map((c) => c.command), ["worlds_list", "view"]);
});

test("a connection failure suspends commands until an explicit reconnect even if the server returns", async () => {
  transport.worlds = [raw()];
  await api.worldsList();
  transport.offline = true;
  await assert.rejects(api.view("world:test"));
  transport.offline = false;
  const before = transport.calls.length;
  await assert.rejects(api.documentSave("world:test", "art:test", "stale", {}, "s"));
  assert.equal(transport.calls.length, before, "no pending edit is retried on its own");
  await api.worldConnect("world:test");
  await api.view("world:test");
  assert.equal(transport.calls.at(-1).command, "world_call");
});
