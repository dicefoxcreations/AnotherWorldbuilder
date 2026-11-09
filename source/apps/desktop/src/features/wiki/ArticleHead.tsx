import type { ReactNode } from "react";

/**
 * The head of an article page: icon and name, below them a hairline with the
 * article type on the left and the visibility on the right.
 *
 * The same form in all three views — in the builder, while creating and in
 * the finished article. Otherwise one builds a page against a picture that
 * does not exist like that later: name and icon used to sit in the toolbar,
 * but in the article they stand above the content.
 *
 * The head is not part of the grid. It stands above the canvas and is the
 * same in every template; only what comes below it can be placed.
 */
export function ArticleHead({
  icon,
  title,
  type,
  marks,
}: {
  /** The icon of the article type — in the builder, the picker for it. */
  icon: ReactNode;
  /** The name of the article, or a placeholder for it. */
  title: ReactNode;
  /** The article type: a link, an input field or a bare label. */
  type: ReactNode;
  marks?: ReactNode;
}) {
  return (
    <header className="article-head">
      <h1>
        <span className="type-icon">{icon}</span>
        {title}
      </h1>
      <div className="article-meta">
        {type}
        <span className="meta-line" />
        {marks}
      </div>
    </header>
  );
}
