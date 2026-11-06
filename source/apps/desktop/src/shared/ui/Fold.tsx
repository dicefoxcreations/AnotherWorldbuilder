import { useState, type ReactNode } from "react";

/**
 * A section that folds: its head always there — a title, and in one line
 * what is inside — and its body only when open. Actions on the right of the
 * head work without opening it.
 */
export function Fold({
  title,
  summary,
  actions,
  open: initially = false,
  level = 1,
  children,
}: {
  title: ReactNode;
  summary?: ReactNode;
  actions?: ReactNode;
  open?: boolean;
  /** 1: a section of a page; 2: a part inside one. */
  level?: 1 | 2;
  children: ReactNode;
}) {
  const [open, setOpen] = useState(initially);
  return (
    <section className={`foldout level-${level}${open ? " open" : ""}`}>
      <div className="fold-head">
        <button className="fold-toggle" aria-expanded={open} onClick={() => setOpen(!open)}>
          <span className="fold-chevron" aria-hidden>
            ›
          </span>
          <span className="fold-title">{title}</span>
          {summary && <span className="fold-summary">{summary}</span>}
        </button>
        {actions && <span className="fold-actions">{actions}</span>}
      </div>
      {open && <div className="fold-body">{children}</div>}
    </section>
  );
}
