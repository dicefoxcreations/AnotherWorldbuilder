import { Fragment, type ReactNode } from "react";
import { Icon } from "./Icons";
import type { DiceFoxIconName } from "./dicefox-icons/DiceFoxIcon";

/** One area of a window with areas — the options, AW Server's window. */
export type Area<Id extends string = string> = { id: Id; icon: DiceFoxIconName; name: string; group?: string; badge?: number };

/**
 * The areas of such a window, in groups: a side bar, on a phone a strip of
 * tabs on top. What follows the areas (Switch world, Close) comes as children.
 */
export function AreaNav<Id extends string>({
  head,
  areas,
  on,
  onPick,
  children,
}: {
  head: ReactNode;
  areas: Area<Id>[];
  on: Id;
  onPick: (id: Id) => void;
  children?: ReactNode;
}) {
  return (
    <nav className="options-nav">
      <div className="nav-head">{head}</div>
      {areas.map((b, i) => (
        <Fragment key={b.id}>
          {b.group && (i === 0 || areas[i - 1].group !== b.group) && <h5 className="options-group">{b.group}</h5>}
          <button className={b.id === on ? "ghost on" : "ghost"} onClick={() => onPick(b.id)}>
            <span className="nav-icon">
              <Icon name={b.icon} />
            </span>
            {b.name}
            {!!b.badge && <span className="badge">{b.badge}</span>}
          </button>
        </Fragment>
      ))}
      {children}
    </nav>
  );
}
