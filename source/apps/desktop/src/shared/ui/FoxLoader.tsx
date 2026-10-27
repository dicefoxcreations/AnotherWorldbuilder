import fox from "../../assets/fox-loader.webp";

/**
 * The DiceFox with his d20 — the die fills as something loads. `progress`
 * from 0 to 1; `null` while how far is not known: the die glows on and off.
 */
export function FoxLoader({ progress, label, detail }: { progress: number | null; label: string; detail?: string }) {
  const share = progress === null ? null : Math.max(0.04, Math.min(1, progress));
  return (
    <div className="fox-loader" role="status" aria-live="polite">
      <div className="fox-frame">
        <img src={fox} alt="" draggable={false} />
        {/* The die's outline in the picture: the fill rises inside it. */}
        <div className="fox-die">
          <div className={share === null ? "fox-fill waiting" : "fox-fill"} style={share === null ? undefined : { height: `${share * 100}%` }} />
        </div>
      </div>
      <strong>{label}</strong>
      <small className="small muted">{detail ?? (share === null ? "" : `${Math.floor(share * 100)} %`)}</small>
    </div>
  );
}
