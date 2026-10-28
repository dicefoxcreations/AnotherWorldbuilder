import { DiceFox } from "./DiceFox";

/**
 * The DiceFox builds the landscape inside his world die. `progress`
 * from 0 to 1; null repeats the build without claiming a percentage.
 */
export function FoxLoader({ progress, label, detail }: { progress: number | null; label: string; detail?: string }) {
  const share = progress === null ? null : Number.isFinite(progress) ? Math.max(0, Math.min(1, progress)) : 0;
  return (
    <div className="fox-loader" role="status" aria-live="polite">
      <div className="fox-frame">
        <DiceFox progress={share} />
      </div>
      <strong>{label}</strong>
      <small className="small muted">{detail ?? (share === null ? "" : `${Math.floor(share * 100)} %`)}</small>
      <span className="fox-progress" role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={share === null ? undefined : Math.floor(share * 100)} />
    </div>
  );
}
