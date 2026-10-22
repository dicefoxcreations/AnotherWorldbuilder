import { useEffect, useMemo, useRef, useState } from "react";
import type { ArticleView, Place, Template } from "../../api/client";
import { findType, isProperties } from "../types";
import { comboOf, FIXED, loadShortcuts, shownCombo } from "../Shortcuts";

/**
 * Keys that reach the whole world, not one field:
 *
 * - Ctrl+P opens a document by its name — the closest names first.
 * - Ctrl+Shift+F searches the text of every document this reader can open;
 *   the one chosen opens with the words found on it (Ctrl+F's bar).
 * - `?` (outside a text field) shows every key.
 */
type Mode = "open" | "search" | "keys";

/** A document's text as one reads it: links by their words, tags left out. */
function readable(text: string, names: Map<string, string>): string {
  return text
    .replace(/@@([^@|]*)\|([^@]*)@@/g, "$2")
    .replace(/@@art:([0-9A-Za-z]{26})@@/g, (_, id) => names.get(`art:${id}`) ?? "")
    .replace(/@@(?:[^@:]*:)?([^@]*)@@/g, "$1")
    .replace(/<[^>]*>/g, " ")
    .replace(/[#*_>`]/g, "")
    .replace(/\s+/g, " ");
}

/** Where a page's words are wanted: the find bar opens with them. */
export const FIND_EVENT = "aw:find";

export function QuickOpen({
  world,
  articles,
  templates,
  onPlace,
}: {
  world: string;
  articles: ArticleView[];
  templates: Template[];
  onPlace: (p: Place) => void;
}) {
  const [mode, setMode] = useState<Mode | null>(null);
  const [words, setWords] = useState("");
  const [at, setAt] = useState(0);
  const field = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      const combo = comboOf(e);
      if (combo === "ctrl+p" || combo === "ctrl+shift+f") {
        e.preventDefault();
        setMode(combo === "ctrl+p" ? "open" : "search");
        setWords("");
        setAt(0);
        return;
      }
      const t = e.target as HTMLElement | null;
      const writing = !!t && (t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName));
      if (e.key === "?" && !writing && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        setMode((m) => (m === "keys" ? null : "keys"));
      }
    };
    document.addEventListener("keydown", key, true);
    return () => document.removeEventListener("keydown", key, true);
  }, []);
  useEffect(() => setAt(0), [words, mode]);

  const names = useMemo(() => new Map(articles.map((a) => [a.id, a.name])), [articles]);
  const q = words.trim().toLowerCase();
  const hits = useMemo((): { article: ArticleView; snippet?: string }[] => {
    if (mode === "open") {
      if (!q) return [];
      return articles
        .map((a) => ({ a, at: a.name.toLowerCase().indexOf(q) }))
        .filter((x) => x.at >= 0)
        .sort((x, y) => (x.at === 0) === (y.at === 0) ? x.a.name.length - y.a.name.length || x.a.name.localeCompare(y.a.name) : x.at === 0 ? -1 : 1)
        .slice(0, 30)
        .map((x) => ({ article: x.a }));
    }
    if (mode === "search") {
      if (q.length < 2) return [];
      const out: { article: ArticleView; snippet?: string }[] = [];
      for (const a of articles) {
        const text = readable(a.layers.filter((l) => !isProperties(l.section)).map((l) => l.text).join(" "), names);
        const i = text.toLowerCase().indexOf(q);
        if (i < 0 && !a.name.toLowerCase().includes(q)) continue;
        out.push({ article: a, snippet: i < 0 ? "" : `${i > 40 ? "…" : ""}${text.slice(Math.max(0, i - 40), i + q.length + 60)}…` });
        if (out.length >= 60) break;
      }
      return out;
    }
    return [];
  }, [mode, q, articles, names]);

  if (!mode) return null;
  const close = () => setMode(null);
  const choose = (a: ArticleView) => {
    close();
    onPlace({ kind: "article", id: a.id });
    // Searched words: the page opens with them found.
    if (mode === "search" && words.trim()) setTimeout(() => document.dispatchEvent(new CustomEvent(FIND_EVENT, { detail: words.trim() })), 400);
  };

  if (mode === "keys") {
    const mine = loadShortcuts(world);
    return (
      <div className="dialog-backdrop" onClick={close}>
        <div className="quick-open keys-help" role="dialog" aria-label="Shortcuts" onClick={(e) => e.stopPropagation()}>
          <h3>Shortcuts</h3>
          <table className="shortcut-table">
            <tbody>
              {[...FIXED, ...mine.map((s) => ({ combo: s.combo, what: `Link to a ${findType(templates, s.type)?.name ?? "?"}` }))].map((f) => (
                <tr key={f.combo}>
                  <td>
                    <kbd>{shownCombo(f.combo)}</kbd>
                  </td>
                  <td>{f.what}</td>
                </tr>
              ))}
            </tbody>
          </table>
          <p className="small muted">Your own: Options › Shortcuts.</p>
        </div>
      </div>
    );
  }

  return (
    <div className="dialog-backdrop" onClick={close}>
      <div className="quick-open" role="dialog" aria-label={mode === "open" ? "Open" : "Search"} onClick={(e) => e.stopPropagation()}>
        <input
          ref={field}
          autoFocus
          value={words}
          placeholder={mode === "open" ? "Open …" : "Search all documents …"}
          onChange={(e) => setWords(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") close();
            else if (e.key === "ArrowDown") (e.preventDefault(), setAt((i) => Math.min(i + 1, hits.length - 1)));
            else if (e.key === "ArrowUp") (e.preventDefault(), setAt((i) => Math.max(i - 1, 0)));
            else if (e.key === "Enter" && hits[at]) choose(hits[at].article);
          }}
        />
        <ul className="quick-hits">
          {hits.map(({ article, snippet }, i) => (
            <li key={article.id}>
              <button className={i === at ? "ghost on" : "ghost"} onMouseEnter={() => setAt(i)} onClick={() => choose(article)}>
                <span className="quick-name">
                  {findType(templates, article.articleType)?.icon ?? "📄"} {article.name}
                  <span className="small muted"> {findType(templates, article.articleType)?.name}</span>
                </span>
                {snippet && <span className="small muted quick-snippet">{snippet}</span>}
              </button>
            </li>
          ))}
          {q && hits.length === 0 && <li className="small muted quick-none">Nothing found.</li>}
        </ul>
      </div>
    </div>
  );
}
