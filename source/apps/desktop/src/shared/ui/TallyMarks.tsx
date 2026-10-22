import { tally } from "../Tally";

/**
 * How well, drawn: for `●` small circles — full, half, or an empty ring for
 * none — the same at every font; any other sign as written, so many times.
 */
export function TallyMarks({ sign, n, className, title }: { sign: string; n: number; className: string; title?: string }) {
  const t = tally(sign, n);
  if (sign !== "●")
    return (
      <span className={`${className}${t.none ? " none" : ""}`} title={title}>
        {t.text}
      </span>
    );
  const whole = Math.max(0, Math.min(5, Math.floor(n)));
  const half = n - Math.floor(n) >= 0.5 && whole < 5;
  return (
    <span className={`${className} dots`} title={title}>
      {t.none ? (
        <i className="dot none" />
      ) : (
        <>
          {Array.from({ length: whole }, (_, i) => (
            <i key={i} className="dot" />
          ))}
          {half && <i className="dot half" />}
        </>
      )}
    </span>
  );
}
