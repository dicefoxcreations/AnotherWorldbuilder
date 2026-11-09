import { useEffect, useRef, useState } from "react";
import { Icon } from "../../shared/ui/Icons";
import { api, type ArticleView, type Bases, type Caret, type Seen, type VersionDocument, type VersionMarks, type DocumentText, type LineError, type Template, type WorldInfo, type TemplateBlock } from "../../api/client";
import { moveBlock } from "./BlockSettings";
import { ArticleHead } from "./ArticleHead";
import { AccessLine } from "../../shared/ui/Visibility";
import { withAccess } from "../../shared/Policies";
import { keepTyped, mergeTyped, sameContent } from "../../shared/editor/Content";
import { mapPosition, rebase } from "../../shared/editor/Merge";
import { type RemoteCaret } from "../../shared/editor/RemoteCarets";
import { changes, type Range } from "../../shared/Marks";
import { FAST_MS, pace } from "../../app/live";
import { HeaderTools } from "../../app/HeaderTools";
import { useSideBar } from "../../shared/ui/SideBar";
import { AccessRows, Inspector, InspectorSection } from "./Inspector";
import { TypeRows } from "./TypeRows";
import { asRead } from "../../shared/BlockOrder";
import { typeOf } from "../../shared/types";
import { CurrentDocument, useWorldData } from "../worlds/WorldData";
import { CoverRows, coverLabel } from "./CoverControl";
import { CodeEditor } from "./CodeEditor";
import { type Layout, type Opened, SAVE_AFTER_MS, SAVE_AT_MOST_MS, SAVE_TOGETHER_MS, SaveProblem, type Side } from "./EditorBasics";
import { sides } from "./DocumentCompare";
import { type Links, Toolbar, type ToolbarField, ToolbarTarget } from "./EditorTools";
import { mayEditPart } from "./PartEditor";
import { VisualEditor } from "./VisualEditor";
import { flat, trace } from "./editorTrace";

type Status = "saved" | "saving" | "invalid" | "failed";



export function DocumentEditor({
  world,
  article,
  templates,
  all,
  layout,
  side,
  compare,
  marks,
  session,
  switches,
  historyButton,
  deleteButton,
  onRetype,
  onSaved,
  onError,
  opened,
}: {
  world: WorldInfo;
  article: ArticleView;
  templates: Template[];
  all: ArticleView[];
  /** Deleting the document — here, while editing, not beside what one reads. */
  deleteButton?: React.ReactNode;
  /** Making it one of another type — then it is opened anew. */
  onRetype?: (articleType: string, parents: string[]) => Promise<void>;
  /** Opened already: shown at once, without asking again. */
  opened?: Opened | null;
  layout: Layout;
  side: Side;
  /** A version to show beside the document — the editor then shows both. */
  compare: VersionDocument | null;
  /** Where that version and now differ, as the journal knows it. */
  marks: VersionMarks | null;
  session: string;
  switches: React.ReactNode;
  historyButton: React.ReactNode;
  onSaved: () => void;
  onError: (e: unknown) => void;
}) {
  const [text, setText] = useState<string | null>(opened?.code ?? null);
  const [doc, setDoc] = useState<DocumentText | null>(opened?.document ?? null);
  // The properties are data, not text: folded away until asked for — in
  // the code and on the visual side alike.
  const [propsOpen, setPropsOpen] = useState(false);
  // The document's own settings — who sees and changes it, its cover — in the side bar.
  const [inspecting, setInspecting] = useState(false);
  useSideBar(`document:${article.id}`, inspecting, () => setInspecting(false));
  const [errors, setErrors] = useState<LineError[]>([]);
  const [status, setStatus] = useState<Status>("saved");
  const [problem, setProblem] = useState<string | null>(null);
  // The document before this keystroke — the blocks keep their ids from it.
  const docRef = useRef<DocumentText | null>(null);
  docRef.current = doc;
  const textRef = useRef<string | null>(null);
  textRef.current = text;
  // What is kept: the code as it stands, and the state of each layer it
  // stands on. Saving writes the difference against these.
  const kept = useRef<{ code: string; base: Bases } | null>(opened ? { code: opened.code, base: opened.base } : null);
  // The document as kept — what one's own changes on the visual side are measured against.
  const keptDoc = useRef<DocumentText | null>(opened?.document ?? null);
  const busy = useRef(false);
  // Typed on while a save was on its way.
  const again = useRef(false);
  // What others wrote, passed over while saving: looked at once the save is done.
  const missed = useRef(false);
  // The last change on the visual side — only its code goes into the text.
  const writeSeq = useRef(0);
  // Since when what was typed waits to be kept.
  const unsavedSince = useRef<number | null>(null);
  // Changes on the visual side whose code is still being written.
  const writing = useRef(0);
  // Whether this session wrote anything — only then does leaving load anew.
  const wrote = useRef(false);
  // The save on its way, if any — leaving waits for it.
  const pending = useRef<Promise<unknown>>(Promise.resolve());
  // A change made on the visual side is already in `doc`; reading its code
  // back would only race the next keystroke.
  const fromVisual = useRef(!!opened);
  // Where one's own caret stands, and who else is here.
  const caret = useRef<Caret | null>(null);
  const [others, setOthers] = useState<Seen[]>([]);
  // The toolbar learns here which field one writes in.
  const toolbarTarget = useRef<((field: ToolbarField | null) => void) | null>(null);

  /** Takes in the document as it now stands — keeping what was typed since. */
  /** `sentDoc`: the reading of `sent` — what one's own changes are measured against. */
  const adopt = (code: string, document: DocumentText, base: Bases, sent: string, sentDoc: DocumentText | null) => {
    const now = textRef.current ?? sent;
    const before = sentDoc;
    const ours = docRef.current;
    trace("adopt", { theirs: flat(document), before: before && flat(before), ours: ours && flat(ours), typedInCode: now !== sent, sent, now });
    kept.current = { code, base };
    keptDoc.current = document;
    // Typed on the visual side meanwhile: merged there, part by part — the
    // code follows from it. Reading merged code back would race the next
    // keystroke and lose it.
    const merged = before && ours && !sameContent(before, ours) ? mergeTyped(before, ours, document) : null;
    trace("adopt-merged", merged && flat(merged));
    if (merged) {
      changeDoc(merged);
    } else if (now === sent) {
      fromVisual.current = true;
      setText(code);
      setDoc(keepTyped(document, docRef.current));
    } else {
      // Typed on while this was on its way: that goes on top.
      setText(rebase(sent, now, code));
    }
  };

  useEffect(() => {
    if (opened) return;
    api
      .documentOpen(world.id, article.id)
      .then((r) => {
        kept.current = { code: r.code, base: r.base };
        keptDoc.current = r.document;
        fromVisual.current = true;
        setText(r.code);
        setDoc(r.document);
      })
      .catch(onError);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world.id, article.id]);

  // Code typed → the visual side follows.
  useEffect(() => {
    if (text === null || docRef.current === null) return;
    if (fromVisual.current) {
      fromVisual.current = false;
      return;
    }
    let gone = false;
    const from = docRef.current;
    api
      .documentRead(world.id, article.id, text, docRef.current)
      .then((r) => {
        // Changed on the visual side meanwhile: that is newer than this reading.
        if (gone || docRef.current !== from) return;
        setErrors(r.errors);
        setDoc(r.document);
      })
      .catch(() => {});
    return () => {
      gone = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text]);

  const save = async () => {
    const sending = textRef.current;
    const sentDoc = docRef.current;
    if (sending === null || !kept.current || sending === kept.current.code) return;
    if (busy.current) {
      // Typed on while one is on its way: kept once it is back.
      again.current = true;
      return;
    }
    busy.current = true;
    unsavedSince.current = null;
    setStatus("saving");
    trace("save", { doc: sentDoc && flat(sentDoc), code: sending, base: Object.keys(kept.current.base).length });
    const started = Date.now();
    try {
      const saving = api.documentSave(world.id, article.id, sending, kept.current.base, session);
      pending.current = saving.catch(() => {});
      const r = await saving;
      wrote.current = true;
      // What was saved means what was typed: the text stays as typed — an
      // empty line, an empty block one is about to fill. Only where the saved
      // document says something else (someone else wrote, a link got its
      // type) does the code follow it.
      const meant = await api.documentRead(world.id, article.id, sending, r.document).catch(() => null);
      if (meant && meant.errors.length === 0 && sameContent(meant.document, r.document)) {
        kept.current = { code: sending, base: r.base };
        keptDoc.current = r.document;
        // Typed on meanwhile — perhaps not yet in the code: the field stays as it is.
        const untouched = writing.current === 0 && docRef.current === sentDoc && textRef.current === sending;
        if (untouched) setDoc(keepTyped(r.document, docRef.current));
      } else {
        adopt(r.code, r.document, r.base, sending, meant && meant.errors.length === 0 ? meant.document : null);
      }
      setStatus("saved");
      setProblem(null);
      trace("saved", { ms: Date.now() - started, same: !!(meant && sameContent(meant.document, r.document)), code: r.code });
    } catch (e) {
      trace("save-failed", { ms: Date.now() - started, error: String(e) });
      setStatus("failed");
      setProblem(String(e));
    } finally {
      busy.current = false;
    }
    // What was typed meanwhile, and what others wrote meanwhile.
    if (again.current || (textRef.current !== null && kept.current && textRef.current !== kept.current.code)) {
      again.current = false;
      setTimeout(() => void save(), 50);
    } else if (missed.current) {
      missed.current = false;
      lookAgain();
    }
  };

  // Kept as it is typed — after a short pause, and only what reads.
  useEffect(() => {
    if (text === null || !kept.current || text === kept.current.code) return;
    if (errors.length > 0) {
      setStatus("invalid");
      return;
    }
    unsavedSince.current ??= Date.now();
    const pause = others.length > 0 ? SAVE_TOGETHER_MS : SAVE_AFTER_MS;
    const timer = setTimeout(() => void save(), Math.min(pause, Math.max(0, unsavedSince.current + SAVE_AT_MOST_MS - Date.now())));
    return () => clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [text, errors]);

  // Where one writes goes out, where the others write comes in.
  // Alone, now and then is enough to notice someone coming; together, often.
  useEffect(() => {
    let gone = false;
    let tick = 0;
    const tell = () => {
      if (!pace.network) {
        pace.together = false;
        setOthers([]);
        return;
      }
      if (!pace.together && tick++ % 5 !== 0) return;
      api
        .presence(world.id, article.id, caret.current)
        .then((seen) => {
          if (gone) return;
          setOthers(seen);
          pace.together = seen.length > 0;
        })
        .catch(() => {});
    };
    tell();
    const timer = setInterval(tell, FAST_MS);
    return () => {
      gone = true;
      clearInterval(timer);
      pace.together = false;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [world.id, article.id]);

  // Leaving keeps what was typed last, ends the session — it becomes a
  // version — and the page shows what now stands.
  useEffect(
    () => () => {
      const last = textRef.current;
      void pending.current
        .then(() => {
          const k = kept.current;
          if (!k || last === null || last === k.code) return null;
          wrote.current = true;
          return api.documentSave(world.id, article.id, last, k.base, session);
        })
        // Only what was written loads the world again: with thousands of
        // documents, a look into the editor should not cost a reload.
        .then(() => wrote.current && onSaved(), onError);
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  // What others wrote arrives while one writes: it comes in, what one typed
  // stays.
  const retry = useRef<ReturnType<typeof setTimeout> | null>(null);
  /** Passed over now: looked at again in a moment, whatever else happens. */
  const later = () => {
    missed.current = true;
    if (retry.current) return;
    retry.current = setTimeout(() => {
      retry.current = null;
      if (missed.current) {
        missed.current = false;
        lookAgain();
      }
    }, 700);
  };
  useEffect(() => () => void (retry.current && clearTimeout(retry.current)), []);
  const lookAgain = () => {
    if (!kept.current) return;
    if (busy.current || writing.current > 0) {
      later();
      return;
    }
    const sent = kept.current.code;
    api
      .documentOpen(world.id, article.id)
      .then((r) => {
        if (busy.current || writing.current > 0 || !kept.current || kept.current.code !== sent) {
          later();
          return;
        }
        if (r.code === sent) {
          kept.current = { code: sent, base: r.base };
          return;
        }
        adopt(r.code, r.document, r.base, sent, keptDoc.current);
      })
      .catch(() => {});
  };
  // eslint-disable-next-line react-hooks/exhaustive-deps
  useEffect(lookAgain, [article]);

  // Visual change → the code follows.
  const changeDoc = (next: DocumentText) => {
    setDoc(next);
    docRef.current = next;
    writing.current++;
    const seq = ++writeSeq.current;
    api
      .documentWrite(next)
      .then((code) => {
        // A later change's code is on its way — it holds this one.
        if (seq !== writeSeq.current) return;
        fromVisual.current = true;
        setText(code);
        setErrors([]);
      })
      .catch(onError)
      .finally(() => {
        writing.current--;
        if (writing.current === 0 && missed.current && !busy.current) {
          missed.current = false;
          lookAgain();
        }
      });
  };

  /**
   * A section before or after its neighbour `other` — as Up and Down in the
   * side bar: the GM moves the type's sections in the type, one's own in
   * the document; whoever else, and a document that keeps an order of its
   * own, in this document alone.
   */
  const worldData = useWorldData();
  const moveSection = (key: string, other: string) => {
    const now = docRef.current;
    if (!now) return;
    const typedKey = (k: string) => !!docType?.blocks.some((b) => b.key === k);
    const keeps = (article.order ?? []).some(typedKey);
    const [i, j] = [now.blocks.findIndex((b) => b.key === key), now.blocks.findIndex((b) => b.key === other)];
    if (world.isGm && !keeps && typedKey(key) && typedKey(other)) {
      moveBlock(world.id, templates, article.articleType, key, other, j > i)
        .then((all) => worldData?.onTemplates?.(all))
        .catch(onError);
    } else if (world.isGm && !keeps && !typedKey(key) && !typedKey(other)) {
      const blocks = [...now.blocks];
      [blocks[i], blocks[j]] = [blocks[j], blocks[i]];
      changeDoc({ ...now, blocks });
    } else {
      const keys = now.blocks.map((b) => b.key);
      [keys[i], keys[j]] = [keys[j], keys[i]];
      api
        .blockOrderSet(world.id, article.id, keys)
        .then(() => worldData?.onChanged())
        .catch(onError);
    }
  };

  // The sections in the code as they are read — a section moved or made to
  // look otherwise on the visual side, or in its type, moves along here.
  const docType = typeOf(templates, article.articleType);
  useEffect(() => {
    const now = docRef.current;
    if (!now) return;
    const next = asRead(now.blocks, docType, article);
    const said = (blocks: TemplateBlock[]) => JSON.stringify(blocks.map(({ key, title, style, place, fold, heading, width }) => [key, title, style, place, fold, heading, width]));
    if (said(next) !== said(now.blocks)) changeDoc({ ...now, blocks: next });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [docType, article.order, article.looks, doc === null]);

  if (text === null || doc === null) return <p className="note">Opening …</p>;

  const links: Links = { all, templates, selfId: article.id };
  // Their places count in the code as kept; what was typed since moves them.
  const base = kept.current?.code ?? text;
  const codeCarets: RemoteCaret[] = others.flatMap((o) =>
    o.caret
      ? [{ id: o.id, name: o.name, start: mapPosition(base, text, o.caret.codeStart), end: mapPosition(base, text, o.caret.codeEnd) }]
      : [],
  );
  const partCarets = (key: string): RemoteCaret[] =>
    others.flatMap((o) =>
      o.caret && o.caret.part === key ? [{ id: o.id, name: o.name, start: o.caret.partStart, end: o.caret.partEnd }] : [],
    );
  // Beside a version: what differs marked in both. The version's marks fit
  // the text as kept; while something typed is not kept yet, comparing the
  // texts stands in for them.
  const kinds = sides(marks);
  const exact = marks !== null && text === marks.nowCode;
  const newInCode = !compare ? [] : exact ? marks!.now.code : changes(compare.code, text).added;
  const partText = (d: DocumentText, key: string) =>
    d.blocks.flatMap((b) => b.parts).find((p) => p.key === key)?.text ?? "";
  const partMarks = (side: "then" | "now") => (key: string): Range[] => {
    if (!compare) return [];
    if (side === "then" && marks) return marks.then.parts[key] ?? [];
    if (side === "now" && exact) return marks!.now.parts[key] ?? [];
    const then = partText(compare.document, key);
    const now = partText(doc, key);
    return side === "now" ? changes(then, now).added : changes(then, now).removed;
  };
  const thenInCode: Range[] = !compare ? [] : marks ? marks.then.code : changes(compare.code, text).removed;
  const code = (
    <CodeEditor
      text={text}
      errors={errors}
      gm={world.isGm}
      links={links}
      propsOpen={propsOpen}
      marks={{ kind: kinds.now, ranges: newInCode }}
      carets={codeCarets}
      onPropsOpen={setPropsOpen}
      onText={setText}
      onCaret={(c) => (caret.current = c)}
      movable={(i, by) => {
        const here = doc.blocks[i];
        const other = doc.blocks[i + by];
        return !!here && !!other && (here.place ?? "flow") === (other.place ?? "flow");
      }}
      onMove={(i, by) => moveSection(doc.blocks[i].key, doc.blocks[i + by].key)}
    />
  );
  const visual = (
    <VisualEditor
      world={world}
      article={article}
      templates={templates}
      all={all}
      doc={doc}
      propsOpen={propsOpen}
      onPropsOpen={setPropsOpen}
      links={links}
      carets={partCarets}
      marks={{ kind: kinds.now, of: partMarks("now") }}
      onDoc={changeDoc}
      onCaret={(c) => (caret.current = c)}
    />
  );
  // The version, as it stood — to read, not to write.
  const then = compare && (
    <div>
      <h4 className="compare-label">{kinds.label}</h4>
      {side === "code" ? (
        <pre className="compare-side code">
          {pieces(compare.code, thenInCode).map((s, i) => (
            <span key={i} className={s.marked ? `mark-${kinds.then}` : undefined}>
              {s.text}
            </span>
          ))}
        </pre>
      ) : (
        <fieldset disabled className="then">
          <VisualEditor
            world={world}
            article={article}
            templates={templates}
            all={all}
            doc={compare.document}
            propsOpen={propsOpen}
            onPropsOpen={setPropsOpen}
            links={links}
            carets={() => []}
            marks={{ kind: kinds.then, of: partMarks("then") }}
            onDoc={() => {}}
            onCaret={() => {}}
          />
        </fieldset>
      )}
    </div>
  );

  const dirty = kept.current !== null && text !== kept.current.code;
  return (
    <ToolbarTarget.Provider value={toolbarTarget}>
      <HeaderTools>
        {switches}
        {status === "invalid" || status === "failed" ? (
          <SaveProblem
            title={status === "invalid" ? "Not kept yet" : "Not kept"}
            onRetry={status === "failed" ? () => void save() : undefined}
          >
            {status === "invalid" ? (
              <>
                <p>The code does not read — nothing is kept until it does. Fix the marked line:</p>
                <ul>
                  {errors.map((e) => (
                    <li key={`${e.line} ${e.text}`}>
                      <strong>Line {e.line}</strong> {e.text}
                    </li>
                  ))}
                </ul>
              </>
            ) : (
              <p>Saving failed: {problem ?? "unknown error"}. What you typed is still here.</p>
            )}
          </SaveProblem>
        ) : status === "saving" || dirty ? (
          <span className="save-state" title="Saving …">
            …
          </span>
        ) : (
          <span className="save-state" title="Kept">
            <Icon name="check" />
          </span>
        )}
        {historyButton}
      </HeaderTools>
      {/* The head as it reads — who sees and who changes the document, the GM's to set. */}
      <CurrentDocument.Provider value={article.id}>
        <ArticleHead
          icon={typeOf(templates, article.articleType)?.icon || (article.articleType ? "📄" : world.icon)}
          title={
            world.isGm || mayEditPart(article, "properties") ? (
              <span className={`pick-title${inspecting ? " on" : ""}`} onClick={() => setInspecting(true)} title="Who sees and changes it">
                {doc.name || article.name}
              </span>
            ) : (
              doc.name || article.name
            )
          }
          type={
            <>
              {typeOf(templates, article.articleType) && <span className="chip">{typeOf(templates, article.articleType)!.name}</span>}
              {/* In the head, not above the text: editing moves nothing that reads. */}
              <button className={`ghost compact props-toggle${propsOpen ? " on" : ""}`} onClick={() => setPropsOpen(!propsOpen)} aria-expanded={propsOpen}>
                Properties <small className="small">{doc.properties.length}</small>
              </button>
            </>
          }
          marks={
            <>
              {world.isGm || mayEditPart(article, "properties") ? (
                <button className={`access-button${inspecting ? " on" : ""}`} onClick={() => setInspecting(true)} title="Who sees and changes it">
                  <AccessLine world={world} view={doc.access.view} edit={doc.access.edit} />
                  {coverLabel(doc.cover) && <span className="chip cover-chip">{coverLabel(doc.cover)}</span>}
                </button>
              ) : (
                <AccessLine world={world} view={doc.access.view} edit={doc.access.edit} />
              )}
              {deleteButton}
            </>
          }
        />
      </CurrentDocument.Provider>
      {inspecting && (
        <Inspector title={`Document · ${doc.name || article.name}`} onClose={() => setInspecting(false)}>
          <InspectorSection title="Access">
            <AccessRows
              world={world}
              value={doc.access}
              viewOnly={!world.isGm}
              onChange={(patch) => changeDoc(withAccess(doc, patch, (p) => world.isGm || mayEditPart(article, p.key ?? "")))}
            />
          </InspectorSection>
          {onRetype && <TypeRows world={world} article={article} templates={templates} all={all} onRetype={onRetype} />}
          {world.isGm && (
            <InspectorSection title="Cover">
              <CoverRows name={doc.name || article.name} cover={doc.cover} onCover={(cover) => changeDoc({ ...doc, cover })} />
            </InspectorSection>
          )}
        </Inspector>
      )}
      {!then && <Toolbar target={toolbarTarget} />}
      {then ? (
        <div className="two-columns document-columns compare-live">
          {then}
          <div>
            <h4 className="compare-label">Now</h4>
            {side === "code" ? code : visual}
          </div>
        </div>
      ) : layout === "split" ? (
        <div className="two-columns document-columns">
          {code}
          <div className="preview-side">{visual}</div>
        </div>
      ) : side === "code" ? (
        code
      ) : (
        visual
      )}
    </ToolbarTarget.Provider>
  );
}

/** A text cut into what is marked and what is not. */
function pieces(text: string, ranges: Range[]): { text: string; marked: boolean }[] {
  const out: { text: string; marked: boolean }[] = [];
  let at = 0;
  for (const r of [...ranges].sort((a, b) => a.start - b.start)) {
    if (r.start < at) continue;
    if (r.start > at) out.push({ text: text.slice(at, r.start), marked: false });
    out.push({ text: text.slice(r.start, r.end), marked: true });
    at = r.end;
  }
  if (at < text.length) out.push({ text: text.slice(at), marked: false });
  return out;
}
