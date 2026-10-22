import { useEffect, useState } from "react";
import { api, type WorldInfo } from "../../api/client";
import { eraYmd, formatDay, toDay, type Era, type Moon, type WorldCalendar } from "../../shared/Calendar";
import { useLatestDay, WorldMomentPicker } from "../../shared/ui/DateInputs";
import { Fold } from "../../shared/ui/Fold";
import { MonthGrid } from "../wiki/MonthView";

/** What keeps a calendar from being saved — as Rust checks it (`WorldCalendar::validate`). */
function problem(c: WorldCalendar): string | null {
  if (c.eras.some((e) => e.months.length === 0 || e.months.some((m) => !m.name.trim() || m.days < 1))) return "Every month needs a name and days.";
  if (c.eras.some((e, i) => i > 0 && e.start <= c.eras[i - 1].start)) return "Each era begins after the one before.";
  if (c.eras.length > 1) {
    const marks = [...c.eras.map((e) => e.abbr.trim()), ...(c.eras[0].before.trim() ? [c.eras[0].before.trim()] : [])];
    if (marks.some((m) => !m) || new Set(marks).size < marks.length) return "Each era needs a mark of its own.";
  }
  if (c.eras.some((e) => e.moons.some((m) => !(m.period > 0)))) return "A moon needs its days.";
  return null;
}

/** A number as typed — `29.5` or `29,5`. */
const typedNumber = (text: string) => Number(text.replace(",", ".").replace(/[^\d.]/g, ""));

/** What an era's calendar is, in a line: `12 months · 362 days`. */
const yearLine = (e: Era) => `${e.months.length} months · ${e.months.reduce((n, m) => n + m.days, 0)} days`;

/**
 * One era, folded: its head says what it is and from when; inside, its
 * name and mark, and its calendar in parts of its own — months, weekdays,
 * leap day. Another era's calendar can be taken over with one pick, to be
 * changed where this one differs.
 */
function EraFold({
  calendar,
  index,
  gm,
  open,
  onEra,
  onRemove,
  latestDay,
}: {
  calendar: WorldCalendar;
  index: number;
  gm: boolean;
  open: boolean;
  /** Where the story stands — a new moon's first full day. */
  latestDay: number | null;
  onEra: (part: Partial<Era>) => void;
  onRemove: () => void;
}) {
  const era = calendar.eras[index];
  const earlier: WorldCalendar = { eras: calendar.eras.slice(0, index) };
  const drawable = calendar.eras.every((e) => e.months.length > 0 && e.months.every((m) => m.days > 0));
  const others = calendar.eras.map((e, i) => ({ e, i })).filter(({ i }) => i !== index);
  const half = Math.ceil(era.months.length / 2);
  const setMoon = (n: number, part: Partial<Moon>) => onEra({ moons: era.moons.map((m, i) => (i === n ? { ...m, ...part } : m)) });
  const when = index === 0 ? "from the beginning" : drawable ? `from ${formatDay(earlier, era.start)}` : "";
  return (
    <Fold
      level={2}
      open={open}
      title={
        <>
          {era.name || "Unnamed era"} {era.abbr && <span className="cal-mark">{era.abbr}</span>}
        </>
      }
      summary={when}
      actions={
        gm &&
        index > 0 && (
          <button className="cal-x" title={`Remove ${era.name || "this era"}`} onClick={onRemove}>
            ×
          </button>
        )
      }
    >
      <div className="cal-fields">
        <label>
          <small>Name</small>
          <input className="cal-name" value={era.name} disabled={!gm} onChange={(e) => onEra({ name: e.target.value })} />
        </label>
        <label>
          <small>Mark</small>
          <input className="cal-short" value={era.abbr} disabled={!gm} onChange={(e) => onEra({ abbr: e.target.value })} placeholder="AR" />
        </label>
        {index === 0 ? (
          <label title="After a year before year 1, counted back">
            <small>Before</small>
            <input className="cal-short" value={era.before} disabled={!gm} onChange={(e) => onEra({ before: e.target.value })} placeholder="BR" />
          </label>
        ) : (
          <label>
            <small>Begins</small>
            <WorldMomentPicker days calendar={earlier} value={era.start} onValue={(v) => gm && v !== null && onEra({ start: Math.floor(v) })} />
          </label>
        )}
        {gm && others.length > 0 && (
          <label title="Months, weekdays, leap day and moons of another era — to change where this one differs">
            <small>Calendar</small>
            <select
              className="cal-copy"
              value=""
              onChange={(e) => {
                const from = calendar.eras[Number(e.target.value)];
                if (from)
                  onEra({
                    months: from.months.map((m) => ({ ...m })),
                    weekdays: [...from.weekdays],
                    leap: from.leap && { ...from.leap },
                    moons: from.moons.map((m) => ({ ...m })),
                  });
              }}
            >
              <option value="" disabled>
                Copy from
              </option>
              {others.map(({ e, i }) => (
                <option key={i} value={i}>
                  {e.name || `Era ${i + 1}`}
                </option>
              ))}
            </select>
          </label>
        )}
      </div>

      <Fold level={2} title="Months" summary={yearLine(era)}>
        <ol className="cal-months" style={{ gridTemplateRows: `repeat(${half}, auto)` }}>
          {era.months.map((m, i) => (
            <li key={i}>
              <span className="cal-n">{i + 1}</span>
              <input value={m.name} disabled={!gm} onChange={(e) => onEra({ months: era.months.map((x, j) => (j === i ? { ...x, name: e.target.value } : x)) })} />
              <input
                className="cal-days"
                inputMode="numeric"
                value={m.days}
                disabled={!gm}
                title="Days"
                onChange={(e) => {
                  const days = Math.max(0, Math.min(999, Number(e.target.value.replace(/\D/g, "")) || 0));
                  onEra({ months: era.months.map((x, j) => (j === i ? { ...x, days } : x)) });
                }}
              />
              {gm && era.months.length > 1 ? (
                <button className="cal-x" title={`Remove ${m.name}`} onClick={() => onEra({ months: era.months.filter((_, j) => j !== i) })}>
                  ×
                </button>
              ) : (
                <span />
              )}
            </li>
          ))}
        </ol>
        {gm && (
          <button className="compact ghost cal-add" onClick={() => onEra({ months: [...era.months, { name: "", days: 30 }] })}>
            ＋ Month
          </button>
        )}
      </Fold>

      <Fold level={2} title="Weekdays" summary={era.weekdays.length ? era.weekdays.filter(Boolean).join(", ") : "none"}>
        <ol className="cal-weekdays">
          {era.weekdays.map((d, i) => (
            <li key={i}>
              <span className="cal-n">{i + 1}</span>
              <input value={d} disabled={!gm} onChange={(e) => onEra({ weekdays: era.weekdays.map((x, j) => (j === i ? e.target.value : x)) })} />
              {gm ? (
                <button className="cal-x" title={`Remove ${d}`} onClick={() => onEra({ weekdays: era.weekdays.filter((_, j) => j !== i) })}>
                  ×
                </button>
              ) : (
                <span />
              )}
            </li>
          ))}
        </ol>
        {gm && (
          <button className="compact ghost cal-add" onClick={() => onEra({ weekdays: [...era.weekdays, ""] })}>
            ＋ Day
          </button>
        )}
      </Fold>

      <Fold level={2} title="Leap day" summary={era.leap ? `every ${era.leap.every} years in ${era.months[era.leap.month_index]?.name ?? "?"}` : "none"}>
        <div className="cal-leap">
          <label className="cal-toggle">
            <input
              type="checkbox"
              checked={!!era.leap}
              disabled={!gm}
              onChange={(e) => onEra({ leap: e.target.checked ? { every: 4, month_index: era.months.length - 1 } : null })}
            />
          </label>
          {era.leap ? (
            <>
              <span>every</span>
              <input
                className="cal-days"
                inputMode="numeric"
                disabled={!gm}
                value={era.leap.every}
                onChange={(e) => era.leap && onEra({ leap: { ...era.leap, every: Math.max(1, Number(e.target.value.replace(/\D/g, "")) || 1) } })}
              />
              <span>years in</span>
              <select disabled={!gm} value={era.leap.month_index} onChange={(e) => era.leap && onEra({ leap: { ...era.leap, month_index: Number(e.target.value) } })}>
                {era.months.map((m, i) => (
                  <option key={i} value={i}>
                    {m.name || `Month ${i + 1}`}
                  </option>
                ))}
              </select>
            </>
          ) : (
            <span className="muted">none</span>
          )}
        </div>
      </Fold>

      <Fold level={2} title="Moons" summary={era.moons.length ? era.moons.map((m) => m.name || "—").join(", ") : "none"}>
        {era.moons.length > 0 && (
          <ul className="cal-moons">
            {era.moons.map((m, i) => (
              <li key={i}>
                <label className="cal-swatch" style={{ background: m.color }} title="Colour">
                  <input type="color" value={m.color} disabled={!gm} onChange={(e) => setMoon(i, { color: e.target.value })} />
                </label>
                <input className="cal-moon-name" value={m.name} disabled={!gm} placeholder="Name" onChange={(e) => setMoon(i, { name: e.target.value })} />
                <span className="muted">every</span>
                <input
                  className="cal-days"
                  inputMode="decimal"
                  disabled={!gm}
                  defaultValue={m.period}
                  title="Days from full to full"
                  onChange={(e) => setMoon(i, { period: typedNumber(e.target.value) })}
                />
                <span className="muted">days, full on</span>
                {drawable && <WorldMomentPicker days calendar={calendar} value={m.full} onValue={(v) => gm && v !== null && setMoon(i, { full: Math.floor(v) })} />}
                {gm && (
                  <button className="cal-x" title={`Remove ${m.name}`} onClick={() => onEra({ moons: era.moons.filter((_, j) => j !== i) })}>
                    ×
                  </button>
                )}
              </li>
            ))}
          </ul>
        )}
        {gm && (
          <button
            className="compact ghost cal-add"
            onClick={() => onEra({ moons: [...era.moons, { name: "", color: "#d8dcef", period: 29.5, full: Math.max(era.start, latestDay ?? era.start) }] })}
          >
            ＋ Moon
          </button>
        )}
      </Fold>
    </Fold>
  );
}

/**
 * How the world counts its days — the GM's. Its eras, one after another,
 * each with a calendar and moons of its own. Each a section that folds,
 * its head saying in a line what is inside. Dates already written are day
 * counts: they stay the same days and are only written anew.
 */
export function CalendarArea({ world, onChanged, onError }: { world: WorldInfo; onChanged: () => void; onError: (e: unknown) => void }) {
  const [draft, setDraft] = useState<WorldCalendar>(world.calendar);
  // Which era was added last — it opens.
  const [fresh, setFresh] = useState<number | null>(null);
  // Only a calendar that really changed replaces the draft — not the same one loaded again.
  const saved = JSON.stringify(world.calendar);
  useEffect(() => setDraft(JSON.parse(saved)), [saved]);
  const gm = world.isGm;
  const changed = JSON.stringify(draft) !== saved;
  const setEra = (at: number, part: Partial<Era>) => setDraft((c) => ({ ...c, eras: c.eras.map((e, i) => (i === at ? { ...e, ...part } : e)) }));
  const wrong = problem(draft);
  const last = draft.eras[draft.eras.length - 1];
  const latestDay = useLatestDay();
  const addEra = () => {
    // Like the last era, a hundred of its years later.
    const [y] = eraYmd(last, last.start);
    const start = toDay(last, y + 100, 1, 1) ?? last.start + 36000;
    setDraft((c) => ({ ...c, eras: [...c.eras, { ...last, name: "", abbr: "", before: "", start }] }));
    setFresh(draft.eras.length);
  };

  return (
    <div className="type-page calendar-area">
      <h2>Calendar</h2>

      <Fold open title="Eras" summary={draft.eras.map((e) => e.name || "—").join(" → ")}>
        {draft.eras.map((_, i) => (
          <EraFold
            // A new key when eras come or go: a fold opens for the era now in its place.
            key={`${i}-${draft.eras.length}`}
            calendar={draft}
            index={i}
            gm={gm}
            open={fresh === i || (fresh === null && draft.eras.length === 1)}
            onEra={(part) => setEra(i, part)}
            latestDay={latestDay}
            onRemove={() => {
              setDraft((c) => ({ ...c, eras: c.eras.filter((_, j) => j !== i) }));
              setFresh(null);
            }}
          />
        ))}
        {gm && (
          <button className="compact ghost cal-add" onClick={addEra}>
            ＋ Era
          </button>
        )}
      </Fold>


      {!wrong && (
        <Fold title="Preview" summary={formatDay(draft, latestDay ?? last.start)}>
          <MonthGrid key={JSON.stringify(draft.eras.map((e) => e.start))} calendar={draft} start={latestDay ?? last.start} />
        </Fold>
      )}

      {gm && (
        <div className="cal-save">
          <button
            className="primary"
            disabled={!changed || !!wrong}
            onClick={() =>
              api
                .calendarSet(world.id, { ...draft, eras: draft.eras.map((e) => ({ ...e, weekdays: e.weekdays.filter((d) => d.trim()) })) })
                .then(onChanged)
                .catch(onError)
            }
          >
            Save
          </button>
          {changed && <button onClick={() => setDraft(JSON.parse(saved))}>Reset</button>}
          {wrong && <span className="small muted">{wrong}</span>}
        </div>
      )}
    </div>
  );
}
