import { useRef } from "react";
import { Icon } from "./Icons";

/** A search field with ✕ once something is in it: one click empties it, as Escape does. */
export function SearchField({
  value,
  onChange,
  placeholder,
  className = "",
}: {
  value: string;
  onChange: (v: string) => void;
  placeholder: string;
  className?: string;
}) {
  const field = useRef<HTMLInputElement>(null);
  return (
    <span className={`search-field ${className}`}>
      <input
        ref={field}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        aria-label={placeholder}
        onKeyDown={(e) => e.key === "Escape" && onChange("")}
      />
      {value && (
        <button
          className="search-clear"
          title="Clear"
          aria-label="Clear"
          onClick={() => {
            onChange("");
            field.current?.focus();
          }}
        >
          <Icon name="close" />
        </button>
      )}
    </span>
  );
}
