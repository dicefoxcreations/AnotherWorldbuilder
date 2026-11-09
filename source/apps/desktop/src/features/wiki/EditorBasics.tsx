import { useState } from "react";
import { Icon } from "../../shared/ui/Icons";
import { type api } from "../../api/client";

/** One pane or two while editing; which one, alone. */
export type Layout = "single" | "split";
export type Side = "code" | "visual";

/** How long typing may pause before what was typed is kept. */
export const SAVE_AFTER_MS = 600;
/** … and while someone else writes in the same document. */
export const SAVE_TOGETHER_MS = 150;
/** Typing on without a pause: what was typed is kept at least this often — the others see it come. */
export const SAVE_AT_MOST_MS = 1000;

/** A fresh editing session's name. */
export function newSession(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

/** A document as opened for writing. */
export type Opened = Awaited<ReturnType<typeof api.documentOpen>>;

/**
 * Something was not kept: a red mark in the header, the explanation beside it
 * on hover — and on click, so the button inside can be reached.
 */
export function SaveProblem({ title, onRetry, children }: { title: string; onRetry?: () => void; children: React.ReactNode }) {
  // Hover shows it; a click keeps it open until the next click.
  const [open, setOpen] = useState(false);
  return (
    <div className={open ? "save-problem open" : "save-problem"}>
      <button className="save-problem-mark danger icon round" aria-label={title} aria-expanded={open} onClick={() => setOpen(!open)}>
        <Icon name="warning" />
      </button>
      <div className="popover right" role="alert">
        <h4>{title}</h4>
        {children}
        {onRetry && <button className="compact" onClick={onRetry}>Try again</button>}
      </div>
    </div>
  );
}
