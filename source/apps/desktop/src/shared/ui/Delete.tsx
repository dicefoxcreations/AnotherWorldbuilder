import { useState } from "react";

/**
 * Deleting — the same everywhere.
 *
 * There used to be four variants: a trash can in warning colour, one
 * without, a two-step dialog and a browser `confirm()`. Whoever deletes
 * something should see the same thing and have to do the same thing
 * everywhere.
 *
 * The rule is a single step with a typed name. A second “really?” after it
 * adds nothing — whoever has typed the name means it; that sort of thing gets
 * clicked through anyway. The typed name is at the same time the
 * confirmation Rust demands.
 */

/**
 * The delete button: red, everywhere. The bin is drawn, not an emoji — an
 * emoji keeps its own colours and would stay grey.
 */
export function DeleteButton({ what, onClick }: { what: string; onClick: () => void }) {
  return (
    <button
      className="danger icon compact delete-button"
      title={`Delete ${what}`}
      aria-label={`Delete ${what}`}
      onClick={(e) => {
        e.stopPropagation();
        onClick();
      }}
    >
      <svg viewBox="0 0 24 24" width="15" height="15" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4h6v3" />
      </svg>
    </button>
  );
}

/** The question before deleting: a small window in the middle. */
export function DeleteDialog({
  title,
  name,
  warning,
  onCancel,
  onDelete,
}: {
  /** Heading, usually “Delete <name>”. */
  title: string;
  /** The name that has to be typed to confirm. */
  name: string;
  /** What is irretrievably lost by this. */
  warning: string;
  onCancel: () => void;
  /** Gets the typed name — it is the confirmation for Rust. */
  onDelete: (confirmation: string) => void;
}) {
  const [input, setInput] = useState("");
  const matches = input.trim() === name.trim();

  return (
    <div className="dialog-backdrop" onClick={onCancel}>
      <div
        className="delete-dialog"
        role="dialog"
        aria-modal="true"
        onClick={(e) => e.stopPropagation()}
      >
        <h3>{title}</h3>
        <p className="small">{warning}</p>
        <label>
          Type the name to confirm:
          <input
            autoFocus
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={name}
            onKeyDown={(e) => {
              if (e.key === "Escape") onCancel();
              if (e.key === "Enter" && matches) onDelete(input.trim());
            }}
          />
        </label>
        <div className="line">
          <button className="danger" disabled={!matches} onClick={() => onDelete(input.trim())}>
            🗑 Delete for good
          </button>
          <button onClick={onCancel}>Cancel</button>
        </div>
      </div>
    </div>
  );
}
