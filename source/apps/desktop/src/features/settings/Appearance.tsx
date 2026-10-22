import { useState } from "react";
import { DEFAULT_COLORS, loadColors, parseHex, saveColors, SCHEMES, type Colors } from "../../shared/Theme";

const FIELDS: [keyof Colors, string][] = [
  ["background", "Background"],
  ["text", "Text"],
  ["accent", "Accent"],
  ["heading", "Headings"],
];

/**
 * The app's colours — this device's, for every world. Four are chosen, the
 * rest follow (`shared/Theme`); every change shows at once.
 */
export function AppearanceArea() {
  const [colors, setColors] = useState<Colors>(loadColors);
  const change = (fresh: Colors) => {
    setColors(fresh);
    saveColors(fresh);
  };
  const isOn = (c: Colors) => (Object.keys(c) as (keyof Colors)[]).every((k) => c[k].toLowerCase() === colors[k].toLowerCase());
  return (
    <div className="type-page appearance">
      <h2>Appearance</h2>
      <p className="small">On this device, for every world.</p>

      <div className="schemes">
        {SCHEMES.map((s) => (
          <button
            key={s.name}
            className={`scheme${isOn(s.colors) ? " on" : ""}`}
            style={{ background: s.colors.background, color: s.colors.text }}
            onClick={() => change(s.colors)}
          >
            <strong style={{ color: s.colors.heading }}>{s.name}</strong>
            <span>
              Text and <u style={{ color: s.colors.accent }}>a link</u>
            </span>
          </button>
        ))}
      </div>

      <div className="color-rows">
        {FIELDS.map(([key, label]) => (
          <ColorRow key={key} label={label} value={colors[key]} onChange={(v) => change({ ...colors, [key]: v })} />
        ))}
      </div>
      <button disabled={isOn(DEFAULT_COLORS)} onClick={() => change(DEFAULT_COLORS)}>
        Reset
      </button>
    </div>
  );
}

/** `#rrggbb`, from anything `parseHex` reads. */
function full(written: string): string | null {
  const rgb = parseHex(written);
  return rgb && "#" + rgb.map((v) => v.toString(16).padStart(2, "0")).join("");
}

/** A colour: picked, or written as `#rrggbb`. */
export function ColorRow({ label, value, onChange }: { label: string; value: string; onChange: (v: string) => void }) {
  const [written, setWritten] = useState(value);
  const [was, setWas] = useState(value);
  // Picked or reset elsewhere: what stands follows.
  if (value !== was) {
    setWas(value);
    setWritten(value);
  }
  return (
    <label className="color-row">
      <span>{label}</span>
      <input type="color" value={value} onChange={(e) => onChange(e.target.value)} />
      <input
        className={`color-hex${parseHex(written) ? "" : " wrong"}`}
        value={written}
        spellCheck={false}
        onChange={(e) => {
          setWritten(e.target.value);
          // While typing, only all six digits: `#abc` may still become `#abcdef`.
          if (/^#?[0-9a-f]{6}$/i.test(e.target.value.trim())) onChange(full(e.target.value)!);
        }}
        onBlur={() => (full(written) ? onChange(full(written)!) : setWritten(value))}
      />
    </label>
  );
}
