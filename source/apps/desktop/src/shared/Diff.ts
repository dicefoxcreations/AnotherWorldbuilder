/**
 * Two texts side by side: what is gone marked on the one side, what is new
 * on the other. First line by line; where lines changed, word by word inside
 * them — so a changed word stands out, not the whole paragraph. And where a
 * word changed into a similar one, letter by letter: `aaaa` → `aaabbb` marks
 * one `a` gone and `bbb` new, while `aaa` stays as it was.
 */

export type Segment = { text: string; kind: "same" | "gone" | "new" };

type Op<T> = { kind: "same" | "gone" | "new"; item: T };

/** The longest common subsequence as a list of steps — or `null` when too big. */
function steps<T>(a: T[], b: T[], limit = 2_000_000): Op<T>[] | null {
  const n = a.length;
  const m = b.length;
  if (n * m > limit) return null;
  const lcs: Uint32Array[] = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--)
    for (let j = m - 1; j >= 0; j--) lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
  const out: Op<T>[] = [];
  let i = 0;
  let j = 0;
  while (i < n || j < m) {
    if (i < n && j < m && a[i] === b[j]) {
      out.push({ kind: "same", item: a[i] });
      i++;
      j++;
    } else if (j < m && (i === n || lcs[i][j + 1] >= lcs[i + 1][j])) out.push({ kind: "new", item: b[j++] });
    else out.push({ kind: "gone", item: a[i++] });
  }
  return out;
}

/** Words, spaces and single signs — the pieces a change is shown in. */
function words(text: string): string[] {
  return text.match(/\s+|[\p{L}\p{N}_]+|[^\s\p{L}\p{N}_]/gu) ?? [];
}

/**
 * A replaced run of words, letter by letter — when the two are alike. Two
 * words with little in common are shown whole: letter by letter they would
 * fall apart into a confetti of single signs.
 */
function letters(gone: string, added: string): Op<string>[] {
  const chars = steps([...gone], [...added], 250_000);
  if (chars) {
    const shared = chars.filter((o) => o.kind === "same").length;
    if (shared >= 2 && shared * 2 >= Math.min([...gone].length, [...added].length)) return chars;
  }
  return [
    { kind: "gone", item: gone },
    { kind: "new", item: added },
  ];
}

/** Word steps with each replaced run looked at letter by letter. */
function refined(ops: Op<string>[]): Op<string>[] {
  const out: Op<string>[] = [];
  let gone = "";
  let added = "";
  const flush = () => {
    if (gone && added) out.push(...letters(gone, added));
    else if (gone) out.push({ kind: "gone", item: gone });
    else if (added) out.push({ kind: "new", item: added });
    gone = "";
    added = "";
  };
  for (const op of ops) {
    if (op.kind === "same") {
      flush();
      out.push(op);
    } else if (op.kind === "gone") gone += op.item;
    else added += op.item;
  }
  flush();
  return out;
}

function push(list: Segment[], text: string, kind: Segment["kind"]) {
  if (!text) return;
  const last = list[list.length - 1];
  if (last && last.kind === kind) last.text += text;
  else list.push({ text, kind });
}

/** `then` on the left with what is gone, `now` on the right with what is new. */
export function sideBySide(then: string, now: string): { left: Segment[]; right: Segment[] } {
  const left: Segment[] = [];
  const right: Segment[] = [];
  const lines = (s: string) => s.split(/(?<=\n)/).filter((l) => l !== "");
  const a = lines(then);
  const b = lines(now);
  const byLine = steps(a, b) ?? [...a.map((item) => ({ kind: "gone" as const, item })), ...b.map((item) => ({ kind: "new" as const, item }))];

  let gone: string[] = [];
  let added: string[] = [];
  const flush = () => {
    if (gone.length && added.length) {
      const inside = steps(words(gone.join("")), words(added.join("")));
      if (inside) {
        for (const op of refined(inside)) {
          if (op.kind !== "new") push(left, op.item, op.kind);
          if (op.kind !== "gone") push(right, op.item, op.kind);
        }
      } else {
        push(left, gone.join(""), "gone");
        push(right, added.join(""), "new");
      }
    } else {
      push(left, gone.join(""), "gone");
      push(right, added.join(""), "new");
    }
    gone = [];
    added = [];
  };
  for (const op of byLine) {
    if (op.kind === "same") {
      flush();
      push(left, op.item, "same");
      push(right, op.item, "same");
    } else if (op.kind === "gone") gone.push(op.item);
    else added.push(op.item);
  }
  flush();
  return { left, right };
}
