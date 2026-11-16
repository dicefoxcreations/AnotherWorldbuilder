import { useWorldData } from "../../features/worlds/WorldData";
import { Icon } from "./Icons";
import { formatMoment, latest, monthDays, momentOf, parseMoment, splitMoment, toDay, toYmd, type WorldCalendar } from "../Calendar";
import { isProperties } from "../types";
import { perLists } from "../Cache";
import type { ArticleView, Template } from "../../api/client";

/** Where the story stands: the latest date any document has. */
export function useLatestDay(): number | null {
  const data = useWorldData();
  return data ? latestDay(data.articles, data.templates) : null;
}

/** Worked out once per list of documents, see `shared/Cache`. */
const latestDay = perLists((articles: ArticleView[], templates: Template[]): number | null => {
  const dated = new Set(templates.flatMap((t) => t.properties.filter((f) => f.kind === "world-date").map((f) => f.key)));
  const moments = articles.flatMap((a) =>
    a.layers.flatMap((l) =>
      isProperties(l.section)
        ? Object.entries(l.fields ?? {})
            .filter(([k, v]) => dated.has(k) && typeof v === "number")
            .map(([, v]) => v as number)
        : [],
    ),
  );
  const at = latest(moments);
  return at === null ? null : Math.floor(at);
});

/**
 * A time of day — `19:00` — or none. Without one it shows `--:--`, not the
 * sample time some systems put into an empty field, which reads as set.
 */
function TimeInput({ value, onValue }: { value: string; onValue: (v: string) => void }) {
  return (
    <span className={value ? "time-slot" : "time-slot empty"}>
      <input type="time" className="date-time" value={value} onChange={(e) => onValue(e.target.value)} aria-label="Time" />
    </span>
  );
}

/**
 * A moment in the world, picked: day, month, year, era — and a time, if
 * wanted. Empty until one sets it; it then starts where the story stands.
 */
export function WorldMomentPicker({
  calendar,
  value,
  onValue,
  days = false,
}: {
  calendar: WorldCalendar;
  value: number | null;
  onValue: (v: number | null) => void;
  /** Only a day — no time, and never none. */
  days?: boolean;
}) {
  const latestDay = useLatestDay();
  if (value === null) {
    return (
      <button className="date-set compact ghost" onClick={() => onValue(latestDay ?? calendar.eras[calendar.eras.length - 1]?.start ?? 0)}>
        Set date
      </button>
    );
  }
  const [day, minutes] = splitMoment(value);
  const hasTime = value % 1 !== 0;
  const { era, index, year: y, month: m, day: d } = toYmd(calendar, day);
  const set = (eraIndex: number, year: number, month: number, dayOf: number, time: number | null) => {
    const to = calendar.eras[eraIndex];
    const last = monthDays(to, year, Math.min(month, to.months.length) - 1);
    const fresh = toDay(to, year, Math.min(month, to.months.length), Math.min(dayOf, last));
    if (fresh !== null) onValue(momentOf(fresh, time));
  };
  const time = hasTime ? minutes : null;
  // The marks one picks a year by: each era's, and the first era's back count.
  const back = index === 0 && y < 1 && !!era.before;
  const marks = [
    ...(calendar.eras[0].before ? [{ key: "before", label: calendar.eras[0].before }] : []),
    ...calendar.eras.map((e, i) => ({
      key: String(i),
      label: e.abbr || e.name,
    })),
  ];
  const mark = back ? "before" : String(index);
  const shownYear = back ? 1 - y : y;
  const setYear = (written: number, markKey: string) => {
    if (markKey === "before") return set(0, 1 - written, m, d, time);
    // The same year, day and month — of the era picked.
    set(Number(markKey), written, m, d, time);
  };
  return (
    <span className="date-pick" title={formatMoment(calendar, value)}>
      <select value={d} onChange={(e) => set(index, y, m, Number(e.target.value), time)} aria-label="Day">
        {Array.from({ length: monthDays(era, y, m - 1) }, (_, i) => (
          <option key={i} value={i + 1}>
            {i + 1}
          </option>
        ))}
      </select>
      <select value={m} onChange={(e) => set(index, y, Number(e.target.value), d, time)} aria-label="Month">
        {era.months.map((mo, i) => (
          <option key={i} value={i + 1}>
            {mo.name}
          </option>
        ))}
      </select>
      <input
        className="date-year"
        inputMode="numeric"
        size={Math.max(3, String(shownYear).length + 1)}
        value={shownYear}
        onChange={(e) => /^-?\d+$/.test(e.target.value) && setYear(Number(e.target.value), mark)}
        aria-label="Year"
      />
      {marks.length > 1 ? (
        <select value={mark} onChange={(e) => setYear(shownYear, e.target.value)} aria-label="Era">
          {marks.map((k) => (
            <option key={k.key} value={k.key}>
              {k.label}
            </option>
          ))}
        </select>
      ) : (
        era.abbr && <span className="muted">{era.abbr}</span>
      )}
      {!days && (
        <TimeInput
          value={time === null ? "" : `${String(Math.floor(time / 60)).padStart(2, "0")}:${String(time % 60).padStart(2, "0")}`}
          onValue={(v) => {
            const [h, mi] = v.split(":").map(Number);
            set(index, y, m, d, v ? h * 60 + mi : null);
          }}
        />
      )}
      {!days && (
        <button className="pick-x" title="No date" onClick={() => onValue(null)}>
          <Icon name="close" />
        </button>
      )}
    </span>
  );
}

/** The same for a moment as written in a document's code. */
export function WrittenMomentPicker({ calendar, value, onValue }: { calendar: WorldCalendar; value: string; onValue: (v: string) => void }) {
  const moment = value.trim() ? parseMoment(calendar, value) : null;
  // What does not read as a date stays as typed, to be fixed by hand.
  if (value.trim() && moment === null) {
    return <input value={value} onChange={(e) => onValue(e.target.value)} title="Not a date of this world's calendar" className="invalid" />;
  }
  return <WorldMomentPicker calendar={calendar} value={moment} onValue={(v) => onValue(v === null ? "" : formatMoment(calendar, v))} />;
}

/** Today, as our own calendar writes it: `2026-09-12`. */
function today(): string {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
}

/**
 * A date of ours — `2026-09-12` — and a time, if wanted: `2026-09-12T19:00`.
 * Empty until one sets it; it then starts today, without a time.
 */
export function OurMomentInput({ value, onValue }: { value: string; onValue: (v: string) => void }) {
  const [date, time = ""] = value.split("T");
  if (!date) {
    return (
      <button className="date-set compact ghost" onClick={() => onValue(today())}>
        Set date
      </button>
    );
  }
  const shownTime = time.slice(0, 5);
  return (
    <span className="date-pick our-moment">
      <input type="date" value={date} onChange={(e) => e.target.value && onValue(shownTime ? `${e.target.value}T${shownTime}` : e.target.value)} aria-label="Date" />
      <TimeInput value={shownTime} onValue={(v) => onValue(v ? `${date}T${v}` : date)} />
      <button className="pick-x" title="No date" onClick={() => onValue("")}>
        <Icon name="close" />
      </button>
    </span>
  );
}
