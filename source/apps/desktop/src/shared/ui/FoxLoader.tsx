import { DiceFox } from "./DiceFox";

/**
 * The DiceFox builds the landscape inside his world die. `progress`
 * from 0 to 1; null repeats the build without claiming a percentage.
 * `onBuilt`: the world is finished in his die — the loader may go.
 */
export function FoxLoader({ progress, label, detail, onBuilt }: { progress: number | null; label: string; detail?: string; onBuilt?: () => void }) {
  const share = progress === null ? null : Number.isFinite(progress) ? Math.max(0, Math.min(1, progress)) : 0;
  return (
    <div className="fox-loader" role="status" aria-live="polite">
      <div className="fox-frame">
        <DiceFox progress={share} onBuilt={onBuilt} />
      </div>
      <strong>{label}</strong>
      <small className="small muted">{detail ?? (share === null || share === 1 ? "" : `${Math.floor(share * 100)} %`)}</small>
      <span className="fox-progress" role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={share === null ? undefined : Math.floor(share * 100)} />
    </div>
  );
}
