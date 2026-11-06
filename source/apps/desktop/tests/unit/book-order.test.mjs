import test from "node:test";
import assert from "node:assert/strict";
import { registerHooks } from "node:module";

globalThis.__awBookOrder = { raw: [], calls: [] };
registerHooks({
  resolve(specifier, context, next) {
    const code =
      specifier === "@tauri-apps/api/core"
        ? `export async function invoke(command,args){
    const state=globalThis.__awBookOrder;state.calls.push({command,args});
    if(command==='templates_load')return state.raw;
    if(command==='template_save'){state.raw=[args.template];return state.raw;}
    return [];
  }`
        : specifier === "@tauri-apps/api/event"
          ? "export async function emit(){};export async function listen(){return ()=>{}};"
          : null;
    return code ? { url: "data:text/javascript," + encodeURIComponent(code), shortCircuit: true } : next(specifier, context);
  },
});
const { api } = await import("../../src/api/client.ts");
const { asTree } = await import("../../src/shared/documentTree.ts");
const { documentOrder, orderBy, sortWritten } = await import("../../src/shared/types.ts");
const chapterType = {
  id: "book-chapter",
  name: "Book chapter",
  sort: ["sequence"],
  navigation_hidden: true,
  properties: [
    { key: "book", label: "Book", kind: "reference", article_type: "book", parent: true },
    { key: "sequence", label: "Order", kind: "number" },
  ],
};
const doc = (id, name, sequence, parent = "book-root") => ({
  id,
  name,
  articleType: "book-chapter",
  layers: [{ section: "properties", fields: { sequence, book: parent } }],
});

test("native chapter sort survives loading and saving, in both the navigation and contents tree", async () => {
  globalThis.__awBookOrder.raw = [chapterType];
  const [type] = await api.templatesLoad("world:test");
  assert.deepEqual(type.sort, ["sequence"]);
  const docs = [doc("credits", "Credits", 18), doc("intro", "Introduction", 1), doc("tracking", "Tracking", 3)];
  const navigation = [...docs].sort(documentOrder([type], type.id, docs));
  assert.deepEqual(
    navigation.map((d) => d.name),
    ["Introduction", "Tracking", "Credits"],
  );
  const contents = [...docs].sort(orderBy([type], docs, sortWritten(type, ["Order"])));
  assert.deepEqual(
    asTree([type], contents).map((row) => row.article.name),
    ["Introduction", "Tracking", "Credits"],
  );
  await api.templateSave("world:test", type);
  const saved = globalThis.__awBookOrder.calls.find((c) => c.command === "template_save").args.template;
  assert.deepEqual(saved.sort, ["sequence"]);
  assert.equal(saved.navigation_hidden, true);
});

test("trees preserve an explicit sibling order for nested chapters and visit cycles only once", async () => {
  globalThis.__awBookOrder.raw = [chapterType];
  const [type] = await api.templatesLoad("world:test");
  const docs = [doc("root", "Root", 0, ""), doc("z", "Z first", 1, "root"), doc("a", "A second", 2, "root")];
  assert.deepEqual(
    asTree([type], docs).map((row) => [row.article.name, row.depth]),
    [
      ["Root", 0],
      ["Z first", 1],
      ["A second", 1],
    ],
  );
  const cycle = [doc("z", "Z first", 1, "a"), doc("a", "A second", 2, "z")];
  assert.deepEqual(
    asTree([type], cycle).map((row) => row.article.id),
    ["z", "a"],
  );
});
