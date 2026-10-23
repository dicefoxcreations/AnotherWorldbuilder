# Calendar, timeline and lists from properties

As of October 2026. Code: `crates/aw-model/src/calendar.rs` (calendar, reading and writing dates), `markdown.rs` (`<timeline>`, `<calendar>`, `<list from=…>`), `names.rs` (names ↔ IDs), `document.rs` in the app (dates in the document code), `lib.rs` (`calendar_set`); interface: `shared/Calendar.ts`, `shared/ui/DateInputs.tsx`, `features/settings/CalendarArea.tsx`, `shared/ui/MoonPhase.tsx`, `features/wiki/Timeline.tsx`, `features/wiki/MonthView.tsx`, `features/wiki/ListCard.tsx` (settings by click), `features/wiki/Listing.tsx`.

## Calendar, eras and moons

Every world has a reckoning of time (a control entry of the world, like its name and symbol). The GM sets it under **Options → Calendar**.

- **Eras** follow one another. Each has a calendar of its own: months with days, weekdays, optionally a leap day every *n* years in one month. Added to that are a name and an **abbreviation** after the year (`A.R.`). Their years count from 1. An era begins on a day of the previous one ("Begins: 1 Firstmonth 412 A.C.") and ends where the next begins. The first reaches back without limit. With **Before** (`B.C.`) it counts years before its year 1 backwards, without a year 0.
- With several eras each needs its own abbreviation, because a written date recognises its era by the abbreviation. A date without an abbreviation belongs to the last era. "＋ Era" takes over the calendar of the last era. "Copy from" takes over, in one era, the months, weekdays and leap day of another; then only what differs is changed. In the options every era is a section that folds open, with months, weekdays and leap day inside.
- **Moons** belong to an era: one can know more or fewer of them than another (in the test world the blood moon only appears with the second era). "Copy from" takes them over too. A moon (or planet, comet) has a name, colour, orbit length in days (fractions too, `29.5`) and a day on which it was full. Its phase on every day follows from that (`moonAge`: 0 new, ½ full). It is drawn in its colour: in the month calendar on every day, in the timeline at the date of every entry.
- A world date is stored as the world's **day number**, a time of day as a fraction of the day (`448804.75` = 18:00). Day 0 is the first day of year 1 of the first era. Sorting, timeline and month view calculate with it, across eras.
- In the document code it stands readable, in the calendar of its era: `Born: 3 Seedmoon 179 A.R.`, `Date: 17 Sixthmonth 288 A.C.`, `In-game date: 14 Fogmoon 203 A.R., 21:00`. On saving, Rust reads it back; a bare day number is accepted too.
- If the calendar changes, the days stay the same – they are only written anew.
- In the editor one picks day, month, year, era and optionally the time. An empty date starts at the latest date of the world, that is where the story stands.
- There is no "today in the world". Where the campaign stands is told by the last session log.
- The field kind `datetime` is a date with time of **our** calendar – when a session was played: `Played on: datetime`, stored as `2026-09-12T19:00`, shown in the system's format.

## Timeline

`<timeline type="Event, Session log" until=End text="Summary, Course">` can stand in any text. In the editor it is drawn as when reading; ⚙ opens its settings by click: types, dates, end, whose life and text. Each click writes the tag; ‹› shows it for changing by hand.

Types are picked by search (`SearchPick`): the chosen ones stand as chips with ×, a field beside them finds more while typing. Only types with a date are offered. A subtype carries the hint "in Character". Whoever picks Character gets player characters and NPCs as well, because they inherit its dates.

- Every world date of a document is an entry: birth, death, session, event. `show=` limits to certain date properties. `until=` makes one of them the end of a span instead of an entry of its own.
- `around=self` shows only what this document itself is or picks in a property (participants, those involved). Relations do not count – a friend's birth does not belong in one's own life story.
- `text=` sets which part of a document supplies an entry's text, by title. The first of the named parts that the document has applies (`Summary` for sessions, `Course` for events). If it has none of them, the entry stays without text. `text=none` means never text. Without `text=` the first part with text applies.
- Display: axis in the middle, years on it, entries alternating left and right, with date, moons, type colour, picture and the first words. If the entries reach across several eras, their name stands where they begin. At the top, a click on a type switches its entries off and on. In narrow columns the axis stands on the left.
- What a reader cannot decrypt does not appear – as with lists.

The model is the familiar timeline layout: entries as cards with date, title, text, colour and link, divided into periods. Parallel tracks do not exist yet.

## Month calendar

`<calendar type="Session log, Event">` shows one month: the days below their weekdays, on each day the moons, in it the documents of the types with their date. It opens at the latest entry; ‹ › pages across years and eras. Options → Calendar shows the same month as a preview of the moons.

## Lists from properties

In the editor every list has ⚙: what it lists (documents of a type, picked by search, or what a property of this document picks), which properties it shows, columns, conditions (property *is* value, for references from a choice) and "Below" for a subtree. Each click writes the tag. Graph, timeline, calendar, images and infobox have that already. All of this exists only in parts one may change.

`<list from=Equipment style=table show="Kind, Rarity">` lists what a property of *this* document picks (`Equipment: @@Item@@ multiple`), in its order. Whoever may change the document adds with ＋ and removes with × – saved directly in the read view (`references_set`), written into the property's text in the editor. The infobox offers the same for multiple choices: the chosen ones with ×, a ＋ for more.

## Family trees

A family tree is a relation graph over a relation property of its own (`Kinship: @@Character@@ as @@Relation@@`), so that friendships and feuds do not crowd it: `<graph via=Kinship show="portrait, type">`. People are placed by hand (`<person who=… at=x,y sub=note note="120 – 190">`), the marriage line straight (`<line between="Tomas, Hedda" sides="right, left" shape=straight>`), children hang from it (`<line between="Tomas & Hedda, Odile" shape=corner>`): a line hanging from a line leaves it at a right angle. A secret descent stands in a relation property with `view=gm`. Someone a reader cannot open is drawn as `???` with a stand-in picture, their role still beside the line; where the GM put them by hand, the `???` stands there too — the stored card names them by id only. The cards show the person's first image field. Wider than the column, the graph is fitted in; ＋ zooms in.
