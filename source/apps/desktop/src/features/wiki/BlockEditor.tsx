import { useRef, useState } from "react";
import type { ArticleView, BlockStyle, Place, Template, WorldInfo } from "../../api/client";
import { linker } from "../../shared/Links";
import { MarkdownText } from "./MarkdownText";
import { useMentions } from "../../shared/editor/Mentions";
import { VisibilityPicker } from "../../shared/ui/Visibility";
import { ImagePicker } from "../../shared/ui/ImagePicker";
import { imageTag } from "../../shared/ImageTag";

/**
 * A text block while writing.
 *
 * Markdown is the storage format, not the interface: above the field sits a
 * bar that inserts the characters. Whoever knows the syntax types it
 * directly; whoever does not, clicks. The field grows with its content and
 * cannot be dragged out of its frame — a block no longer has a fixed height.
 *
 * The visibility stands as a field beside it, never in the text. A typo in
 * prose must not open a secret.
 */
export function BlockEditor({
  world,
  title,
  style,
  text,
  policy,
  level,
  inherited,
  articles,
  templates,
  onText,
  onPolicy,
  onPlace,
}: {
  world: WorldInfo;
  title: string;
  style: BlockStyle;
  text: string;
  policy: string;
  level: string;
  /** Whether the visibility is only the article's — then it steps back. */
  inherited?: boolean;
  /** For `@@` suggestions and the preview of links. */
  articles?: ArticleView[];
  templates?: Template[];
  onText: (fresh: string) => void;
  onPolicy?: (fresh: string) => void;
  onPlace?: (o: Place) => void;
}) {
  const [preview, setPreview] = useState(false);
  const [picking, setPicking] = useState(false);
  const fieldRef = useRef<HTMLTextAreaElement>(null);

  const mentions = useMentions({
    field: fieldRef,
    text,
    onText,
    all: articles ?? [],
    templates: templates ?? [],
  });

  /** Puts characters around the selection — or at the start of the line. */
  const insert = (before: string, after = "", lineStart = false) => {
    const field = fieldRef.current;
    if (!field) return;
    const { selectionStart: from, selectionEnd: to } = field;

    if (lineStart) {
      const start = text.lastIndexOf("\n", from - 1) + 1;
      onText(text.slice(0, start) + before + text.slice(start));
      queueMicrotask(() => {
        field.focus();
        field.setSelectionRange(from + before.length, to + before.length);
      });
      return;
    }

    const middle = text.slice(from, to);
    onText(text.slice(0, from) + before + middle + after + text.slice(to));
    queueMicrotask(() => {
      field.focus();
      field.setSelectionRange(from + before.length, from + before.length + middle.length);
    });
  };

  return (
    <section className={`block ${style}${level !== "open" ? ` level-${level}` : ""}`}>
      <div className="block-mark">
        {/* A part of a block has no title of its own — then none stands
            there either. “untitled” was information about nothing. */}
        {title && <span className="block-title">{title}</span>}
        <span className="block-tools">
          <button className="compact" onClick={() => insert("**", "**")} title="Bold">
            <strong>B</strong>
          </button>
          <button className="compact" onClick={() => insert("*", "*")} title="Italic">
            <em>I</em>
          </button>
          <button className="compact" onClick={() => insert("## ", "", true)} title="Heading">
            H
          </button>
          <button className="compact" onClick={() => insert("- ", "", true)} title="List">
            ☰
          </button>
          <button className="compact" onClick={() => insert("> ", "", true)} title="Quote">
            ❞
          </button>
          <button className="compact" onClick={() => insert("@@")} title="Link to a document — type @@ and pick one">
            ⇥
          </button>
          <button className="compact" onClick={() => setPicking(true)} title="An image — upload one or pick it from the gallery">
            🖼
          </button>
          <button
            className="compact"
            onClick={() => insert('\n<list type="', '" columns=2>\n')}
            title="List the documents of a type — type its name"
          >
            ▤
          </button>
          <button
            className={preview ? "compact on" : "compact"}
            onClick={() => setPreview((v) => !v)}
            title="Preview"
          >
            👁
          </button>
        </span>
        {onPolicy && (
          // If the block says nothing of its own, its visibility does not
          // stand in the way: it is the article's and only shows on a closer look.
          <span className={inherited ? "block-view quiet" : "block-view"}>
            <VisibilityPicker world={world} value={policy} onChange={onPolicy} small />
          </span>
        )}
      </div>

      {preview ? (
        <MarkdownText
          text={text}
          resolve={linker(articles ?? [], templates ?? [])}
          onPlace={onPlace ?? (() => {})}
        />
      ) : (
        <textarea
          {...mentions.handlers}
          ref={fieldRef}
          className="block-text"
          value={text}
          onChange={(e) => onText(e.target.value)}
          rows={Math.max(4, text.split("\n").length + 1)}
          placeholder="Write here. **bold**, *italic*, ## heading, @@ for a link …"
        />
      )}
      {mentions.element}
      {picking && (
        <ImagePicker
          world={world.id}
          onChosen={(id, image) => {
            setPicking(false);
            // On a line of its own, where the caret stands.
            const at = fieldRef.current?.selectionEnd ?? text.length;
            const before = at > 0 && text[at - 1] !== "\n" ? "\n" : "";
            onText(`${text.slice(0, at)}${before}${imageTag(id, image).replace('caption="«»"', 'caption=""')}\n${text.slice(at)}`);
          }}
          onCancel={() => setPicking(false)}
        />
      )}
    </section>
  );
}
