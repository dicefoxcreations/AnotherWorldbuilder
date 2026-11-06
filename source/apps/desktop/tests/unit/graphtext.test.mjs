// A graph as text: cut out of a part, and written back.
import assert from "node:assert/strict";
import test from "node:test";
import { graphText, pieces } from "../../src/shared/GraphText.ts";

const TEXT = `Some words.

<graph around=self depth=2>
<person who=Tim at=12,-6 color=rose sub=note note="The heir">
<person who="Anna Lee" at=x,1>
<line between="Tim, Anna Lee & Franklin" color=amber sides="bottom 30, nowhere" shape=corner arrow=both label=30 dock=", 30" group="nonsense, each" say="Parents, ">
<line between="A & B & C, D">
</graph>
More words.
`;

test("a part is cut into text and graphs, and joins back the same", () => {
  const p = pieces(TEXT);
  assert.deepEqual(p.map((x) => x.kind), ["text", "graph", "text"]);
  assert.equal(p.map((x) => x.text).join(""), TEXT);
  const g = p[1].graph;
  assert.deepEqual([g.around, g.depth], ["self", 2]);
  assert.deepEqual(g.people, [{ who: "Tim", x: 12, y: -6, color: "rose", sub: "note", note: "The heir" }]);
  assert.deepEqual(g.lines, [{ between: ["Tim", "Anna Lee & Franklin"], color: "amber", sides: ["bottom 30", ""], shape: "corner", arrow: "both", label: "30", dock: ["", "30"], group: ["", "each"], say: ["Parents", ""] }]);
});

test("a graph is written back the way Rust reads it", () => {
  const g = pieces(TEXT)[1].graph;
  const text = graphText(g);
  assert.equal(
    text,
    '<graph around=self depth=2>\n<person who=Tim at=12,-6 color=rose sub=note note="The heir">\n<line between="Tim, Anna Lee & Franklin" color=amber sides="bottom 30, " shape=corner arrow=both dock=", 30" group=", each" say="Parents, " label=30>\n</graph>\n',
  );
  assert.deepEqual(pieces(text)[0].graph, g, "and reads back the same");
  // An unclosed graph is the line alone; plain text stays text.
  assert.deepEqual(pieces("<graph>\nwords").map((x) => x.kind), ["graph", "text"]);
  assert.deepEqual(pieces("<graphic>\n").map((x) => x.kind), ["text"]);
});

test("an image stands on a line of its own, cut out of the text around it", () => {
  const text = "<image src=a align=right>\nThe coast.\n<image src=b>\nMore.";
  assert.deepEqual(pieces(text).map((p) => p.kind), ["image", "text", "image", "text"]);
  assert.equal(pieces(text).map((p) => p.text).join(""), text, "nothing lost when cut");
  // Not at the start of a line, it is text.
  assert.deepEqual(pieces("The coast.<image src=b>\n").map((p) => p.kind), ["text"]);
});
