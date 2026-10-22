import { useContext, useState } from "react";
import type { Node, Template } from "../../api/client";
import { formatMoment, monthDays, splitMoment, toDay, toYmd, weekdayOf, yearText, type WorldCalendar, moonsAt } from "../../shared/Calendar";
import { MoonsOf } from "../../shared/ui/MoonPhase";
import { CurrentDocument, useWorldData } from "../worlds/WorldData";
import { datedEntries, hueOf, type Entry } from "./Timeline";

type CalendarNode = Extract<Node, { kind: "calendar" }>;

/** The first day of the month a day lies in — as a day of the world. */
function monthStart(calendar: WorldCalendar, day: number): number {
  const { era, year, month } = toYmd(calendar, day);
  return toDay(era, year, month, 1) ?? day;
}

/**
 * One month of the world's calendar: its days under their weekdays, the
 * moons as they stand each day, and what happened on it. ‹ › go a month
 * back and on — across years and eras.
 */
export function MonthGrid({
  calendar,
  start,
  entries = [],
  templates = [],
  onOpen,
}: {
  calendar: WorldCalendar;
  /** A day of the month shown first. */
  start: number;
  entries?: Entry[];
  templates?: Template[];
  onOpen?: (id: string) => void;
}) {
  const [shown, setShown] = useState(() => monthStart(calendar, start));
  const first = monthStart(calendar, shown);
  const { era, year, month } = toYmd(calendar, first);
  const length = monthDays(era, year, month - 1);
  const offset = weekdayOf(calendar, first) ?? 0;
  const weekdays = era.weekdays;
  const byDay = new Map<number, Entry[]>();
  for (const e of entries) {
    const [day] = splitMoment(e.at);
    if (day >= first && day < first + length) byDay.set(day, [...(byDay.get(day) ?? []), e]);
  }

  return (
    <div className="month-view">
      <div className="month-head">
        <button className="compact ghost" title="Month before" onClick={() => setShown(monthStart(calendar, first - 1))}>
          ‹
        </button>
        <strong>
          {era.months[month - 1]?.name} {yearText(era, year)}
        </strong>
        <button className="compact ghost" title="Month after" onClick={() => setShown(first + length)}>
          ›
        </button>
      </div>
      <div className="month-grid" style={{ gridTemplateColumns: `repeat(${Math.max(1, weekdays.length || 7)}, minmax(0, 1fr))` }}>
        {weekdays.map((w, i) => (
          <span key={`w${i}`} className="month-weekday" title={w}>
            {w.slice(0, 2)}
          </span>
        ))}
        {Array.from({ length: offset }, (_, i) => (
          <span key={`o${i}`} className="month-blank" />
        ))}
        {Array.from({ length }, (_, i) => {
          const day = first + i;
          const here = byDay.get(day) ?? [];
          return (
            <div key={day} className={`month-day${here.length ? " busy" : ""}`}>
              <span className="month-day-head">
                <span className="month-day-n">{i + 1}</span>
                <MoonsOf moons={moonsAt(calendar, day)} day={day} size={10} />
              </span>
              {here.map((e) => (
                <button
                  key={`${e.article.id}${e.at}`}
                  className="month-entry"
                  style={{ ["--hue" as string]: hueOf(templates, e.type?.id ?? "") }}
                  title={`${formatMoment(calendar, e.at)}${e.label ? ` · ${e.label}` : ""}`}
                  onClick={() => onOpen?.(e.article.id)}
                >
                  {e.type?.icon} {e.article.name}
                </button>
              ))}
            </div>
          );
        })}
      </div>
    </div>
  );
}

/**
 * `<calendar type=…>` — a month of the world, opened where the story stands:
 * at the latest of what it shows.
 */
export function MonthView({ node }: { node: CalendarNode }) {
  const data = useWorldData();
  const current = useContext(CurrentDocument);
  if (!data) return <p className="listing-placeholder small">Calendar{node.of.length ? ` of ${node.of.join(", ")}` : ""}</p>;
  const entries = datedEntries(data, current, { of: node.of, show: [], around: "", until: "", text: ["none"] });
  if (typeof entries === "string") return <p className="dead-link">{entries}</p>;
  const latest = entries.length ? entries[entries.length - 1].at : data.world.calendar.eras[data.world.calendar.eras.length - 1].start;
  return (
    <MonthGrid
      calendar={data.world.calendar}
      start={splitMoment(latest)[0]}
      entries={entries}
      templates={data.templates}
      onOpen={(id) => data.onPlace({ kind: "article", id })}
    />
  );
}
