// A Markdown table as cells — `shared/TableText`.
import assert from "node:assert/strict";
import test from "node:test";
import { readTable, tableAt, writeTable } from "../../src/shared/TableText.ts";

const text = "| Item Rarity | Obaya's Reward |\n| ----------- | -------------: |\n| Common      | 10 pp          |\n| Very rare   | 5,000 pp   | ^magic-item-rewards ";

test("a table reads as cells, its mark in its last cell", () => {
  const t = readTable(text);
  assert.deepEqual(t.head, ["Item Rarity", "Obaya's Reward"]);
  assert.deepEqual(t.align, ["", "right"]);
  assert.deepEqual(t.rows, [["Common", "10 pp"], ["Very rare", "5,000 pp ^magic-item-rewards"]]);
});

test("written back, it lines up and reads the same", () => {
  const t = readTable(text);
  t.rows[0][1] = "a | b";
  const out = writeTable(t);
  assert.equal(out.split("\n")[2], "| Common      | a \\| b                       |");
  assert.deepEqual(readTable(out), t);
});

test("a table is found where its head and rule stand", () => {
  const lines = ["Text", ...text.split("\n"), "", "After"];
  assert.equal(tableAt(lines, 0), 0);
  assert.equal(tableAt(lines, 1), 4);
  assert.equal(tableAt(["| a |", "no rule"], 0), 0);
});
