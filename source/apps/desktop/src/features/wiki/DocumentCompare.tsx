import { useEffect, useState } from "react";
import { api, type ArticleView, type Version, type VersionDocument, type VersionMarks } from "../../api/client";
import { late } from "./Versions";
import { changes, marked, type MarkKind, type Range } from "../../shared/Marks";

/** The chosen version: as it stood, and whether it can be taken back. */
export function useVersion(world: string, article: string, chosen: string | null, revision: unknown, onError: (e: unknown) => void) {
  const [doc, setDoc] = useState<VersionDocument | null>(null);
  const [marks, setMarks] = useState<VersionMarks | null>(null);
  const [mayRevert, setMayRevert] = useState(false);
  useEffect(() => {
    setDoc(null);
    setMarks(null);
    setMayRevert(false);
    if (!chosen) return;
    let gone = false;
    api
      .versionDocument(world, article, chosen)
      .then((d) => !gone && setDoc(d))
      .catch(onError);
    api
      .versionMarks(world, article, chosen)
      .then((m) => !gone && setMarks(m))
      .catch(() => {});
    api
      .versionCompare(world, article, chosen)
      .then((c) => !gone && setMayRevert(c.some((x) => x.mayRevert)))
      .catch(() => {});
    return () => {
      gone = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world, article, chosen, revision]);
  return { doc, marks, mayRevert };
}

/**
 * How a comparison reads. A version in the text stands left as it was,
 * marked with what is gone since; now stands right with what is new. A
 * version that waits — or was turned down — stands left as the text would be
 * with it: what it would add is green there, what it would take is red in
 * now.
 */
export function sides(marks: VersionMarks | null) {
  const counted = !marks || marks.review === "counted";
  return {
    label: counted ? "This version" : marks.review === "waiting" ? "With this version" : "With the turned-down version",
    then: (counted ? "removed" : "added") as MarkKind,
    now: (counted ? "added" : "removed") as MarkKind,
  };
}

/** Two finished pages side by side: the version, and now. */
export function ComparePages({
  then,
  now,
  marks,
  page,
}: {
  then: ArticleView;
  now: ArticleView;
  marks: VersionMarks | null;
  page: (a: ArticleView) => React.ReactNode;
}) {
  const [left, right] = markedViews(then, now, marks);
  return (
    <div className="two-columns compare-pages">
      <div>
        <h4 className="compare-label">{sides(marks).label}</h4>
        {page(left)}
      </div>
      <div>
        <h4 className="compare-label">Now</h4>
        {page(right)}
      </div>
    </div>
  );
}

/**
 * Both versions of a page with their differences marked in their texts —
 * by the version's own marks where it has them, otherwise by comparing the texts.
 */
function markedViews(then: ArticleView, now: ArticleView, marks: VersionMarks | null): [ArticleView, ArticleView] {
  const whole = (text: string): Range[] => [{ start: 0, end: text.length }];
  const kinds = sides(marks);
  const left = then.layers.map((l) => {
    const other = now.layers.find((n) => n.id === l.id);
    const gone = marks ? (marks.then.layers[l.id] ?? []) : other ? changes(l.text, other.text).removed : whole(l.text);
    return { ...l, text: marked(l.text, gone, kinds.then) };
  });
  const right = now.layers.map((l) => {
    const other = then.layers.find((t) => t.id === l.id);
    const added = marks ? (marks.now.layers[l.id] ?? []) : other ? changes(other.text, l.text).added : whole(l.text);
    return { ...l, text: marked(l.text, added, kinds.now) };
  });
  return [
    { ...then, layers: left },
    { ...now, layers: right },
  ];
}

/**
 * What came in that one should know of: versions written offline that
 * arrived since one last looked, and versions not in the text yet. Remembered
 * per document on this device.
 */
export function useNews(world: string, article: string, revision: unknown) {
  const key = `aw.seen.${world}.${article}`;
  const read = () => {
    try {
      return Number(localStorage.getItem(key) ?? "0") || 0;
    } catch {
      return 0;
    }
  };
  const [versions, setVersions] = useState<Version[]>([]);
  const [seenUntil, setSeenUntil] = useState(read);
  useEffect(() => {
    setSeenUntil(read());
    api.articleVersions(world, article).then(setVersions).catch(() => setVersions([]));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world, article, revision]);
  const remember = (until: number) => {
    setSeenUntil(until);
    try {
      localStorage.setItem(key, String(until));
    } catch {
      // Without storage the note simply comes again.
    }
  };
  const items = versions.filter((v) => v.stale || v.review === "waiting" || (late(v) && v.arrived > seenUntil));
  return {
    items,
    seen: (v: Version) => remember(Math.max(seenUntil, v.arrived)),
    dismiss: () => remember(Math.max(seenUntil, ...versions.map((v) => v.arrived))),
  };
}
