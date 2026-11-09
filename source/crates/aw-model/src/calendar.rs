//! World calendar and world dates.
//!
//! Campaigns do not count in Gregorian. A world date is therefore stored
//! internally as an integer day count since the epoch and only formatted for
//! display. Sorting, timelines and "three months ago" then work by
//! themselves.
//!
//! This belongs in the first data model and not in a later phase: changing
//! every date field afterwards would mean migrating all devices at once.

use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CalendarError {
    #[error("the world needs at least one era")]
    NoEras,

    #[error("the first era begins on day 0")]
    FirstEraMoved,

    #[error("each era begins after the one before")]
    ErasOutOfOrder,

    #[error("with several eras each needs a mark of its own")]
    EraMarks,

    #[error("every month needs a name")]
    UnnamedMonth,

    #[error("moon '{0}' needs a length of more than 0 days")]
    BadMoon(String),

    #[error("a calendar needs at least one month")]
    NoMonths,

    #[error("month '{name}' has {days} days — that makes no calendar")]
    BadMonthLength { name: String, days: u32 },

    #[error("month {0} does not exist in this calendar")]
    NoSuchMonth(u32),

    #[error("day {day} lies outside month '{month}' ({len} days)")]
    DayOutOfRange {
        day: u32,
        month: String,
        len: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Month {
    pub name: String,
    pub days: u32,
}

/// Leap rule: every `every` years, `month_index` gets one extra day.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeapRule {
    pub every: u32,
    pub month_index: usize,
}

/// One era of the world's time: a calendar of its own, from a day on.
///
/// Its years count from 1 at `start`; an era ends where the next begins.
/// Its moons are its own: an era may know more of them, or fewer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Era {
    pub name: String,
    /// After the year, e.g. "AR" — how a written date names its era.
    pub abbr: String,
    /// The first era only: after a year before its year 1, counted back —
    /// "BR". Empty: those years are 0, -1, … of the era.
    pub before: String,
    pub months: Vec<Month>,
    pub weekdays: Vec<String>,
    pub leap: Option<LeapRule>,
    /// The day of the world's count its year 1 begins on — 0 for the first.
    pub start: i64,
    pub moons: Vec<Moon>,
}

/// A moon — or any body above the world that comes back: a planet, a
/// comet. Its phase follows from how long it takes and one day it stood full.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Moon {
    pub name: String,
    /// A CSS colour, `#c8d4ff`.
    pub color: String,
    /// Days from one full moon to the next; need not be whole.
    pub period: f64,
    /// A day of the world's count it stood full.
    pub full: f64,
}

/// How the world counts its days: eras one after another.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldCalendar {
    /// In the order they follow each other; the first reaches back without end.
    pub eras: Vec<Era>,
}

/// Minutes in a day: a world's day has the hours of ours.
pub const DAY_MINUTES: f64 = 1440.0;

/// A day in the world, counted from the first day of year 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct WorldDate(pub i64);

impl WorldDate {

    pub fn plus_days(&self, days: i64) -> Self {
        WorldDate(self.0 + days)
    }

    pub fn days_until(&self, other: WorldDate) -> i64 {
        other.0 - self.0
    }
}

impl Era {
    pub fn validate(&self) -> Result<(), CalendarError> {
        if self.months.is_empty() {
            return Err(CalendarError::NoMonths);
        }
        for month in &self.months {
            if month.days == 0 {
                return Err(CalendarError::BadMonthLength {
                    name: month.name.clone(),
                    days: month.days,
                });
            }
        }
        Ok(())
    }

    /// Days of an ordinary year, without a leap day.
    pub fn base_year_days(&self) -> i64 {
        self.months.iter().map(|m| m.days as i64).sum()
    }

    pub fn is_leap_year(&self, year: i64) -> bool {
        match &self.leap {
            Some(rule) if rule.every > 0 => year % rule.every as i64 == 0,
            _ => false,
        }
    }

    pub fn year_days(&self, year: i64) -> i64 {
        self.base_year_days() + if self.is_leap_year(year) { 1 } else { 0 }
    }

    pub fn month_days(&self, year: i64, month_index: usize) -> i64 {
        let base = self.months[month_index].days as i64;
        match &self.leap {
            Some(rule) if rule.month_index == month_index && self.is_leap_year(year) => base + 1,
            _ => base,
        }
    }

    /// Days from the era's first day to the first day of `year`.
    ///
    /// Works for years before year 1 too: `div_euclid` rounds in the same
    /// direction there, so the count stays linear throughout.
    fn days_before_year(&self, year: i64) -> i64 {
        let years = year - 1;
        let leaps = match &self.leap {
            Some(rule) if rule.every > 0 => years.div_euclid(rule.every as i64),
            _ => 0,
        };
        years * self.base_year_days() + leaps
    }

    /// A day of this era's calendar as the world's day count. Month and day start at 1.
    pub fn to_date(&self, year: i64, month: u32, day: u32) -> Result<WorldDate, CalendarError> {
        self.validate()?;
        let index = (month as usize)
            .checked_sub(1)
            .filter(|i| *i < self.months.len())
            .ok_or(CalendarError::NoSuchMonth(month))?;

        let len = self.month_days(year, index);
        if day == 0 || day as i64 > len {
            return Err(CalendarError::DayOutOfRange {
                day,
                month: self.months[index].name.clone(),
                len: len as u32,
            });
        }

        let mut total = self.days_before_year(year);
        for i in 0..index {
            total += self.month_days(year, i);
        }
        total += day as i64 - 1;
        Ok(WorldDate(self.start + total))
    }

    /// A day of the world's count as year/month/day of this era's calendar.
    pub fn to_ymd(&self, date: WorldDate) -> (i64, u32, u32) {
        let base = self.base_year_days();
        let (cycle_years, cycle_days) = match &self.leap {
            Some(rule) if rule.every > 0 => (rule.every as i64, base * rule.every as i64 + 1),
            _ => (1, base),
        };

        let mut remaining = date.0 - self.start;
        // Whole cycles in one step — also for dates before year 1.
        let cycles = remaining.div_euclid(cycle_days);
        remaining = remaining.rem_euclid(cycle_days);
        let mut year = 1 + cycles * cycle_years;

        // After that at most `cycle_years` steps, so only a few.
        loop {
            let len = self.year_days(year);
            if remaining < len {
                break;
            }
            remaining -= len;
            year += 1;
        }

        let mut month_index = 0usize;
        loop {
            let len = self.month_days(year, month_index);
            if remaining < len || month_index + 1 == self.months.len() {
                break;
            }
            remaining -= len;
            month_index += 1;
        }

        (year, month_index as u32 + 1, remaining as u32 + 1)
    }

    /// A year as written: `812 AR` — or, before year 1, `12 BR` counted back.
    fn year_text(&self, year: i64) -> String {
        let (n, mark) = if year < 1 && !self.before.is_empty() { (1 - year, &self.before) } else { (year, &self.abbr) };
        if mark.is_empty() { n.to_string() } else { format!("{n} {mark}") }
    }
}

impl WorldCalendar {
    pub fn validate(&self) -> Result<(), CalendarError> {
        let first = self.eras.first().ok_or(CalendarError::NoEras)?;
        if first.start != 0 {
            return Err(CalendarError::FirstEraMoved);
        }
        for era in &self.eras {
            era.validate()?;
            if era.months.iter().any(|m| m.name.trim().is_empty()) {
                return Err(CalendarError::UnnamedMonth);
            }
        }
        if self.eras.windows(2).any(|w| w[1].start <= w[0].start) {
            return Err(CalendarError::ErasOutOfOrder);
        }
        if self.eras.len() > 1 {
            let mut marks: Vec<&str> = self.eras.iter().map(|e| e.abbr.trim()).collect();
            marks.extend(Some(first.before.trim()).filter(|b| !b.is_empty()));
            let count = marks.len();
            marks.sort_unstable();
            marks.dedup();
            if marks.contains(&"") || marks.len() < count {
                return Err(CalendarError::EraMarks);
            }
        }
        if let Some(moon) = self.eras.iter().flat_map(|e| &e.moons).find(|m| !(m.period.is_finite() && m.period > 0.0 && m.full.is_finite())) {
            return Err(CalendarError::BadMoon(moon.name.clone()));
        }
        Ok(())
    }

    /// The era a day falls into.
    pub fn era_at(&self, date: WorldDate) -> &Era {
        self.eras.iter().rev().find(|e| e.start <= date.0).unwrap_or(&self.eras[0])
    }

    /// Year/month/day of a day, in the calendar of its era.
    pub fn to_ymd(&self, date: WorldDate) -> (&Era, i64, u32, u32) {
        let era = self.era_at(date);
        let (y, m, d) = era.to_ymd(date);
        (era, y, m, d)
    }

    pub fn weekday(&self, date: WorldDate) -> Option<&str> {
        let era = self.era_at(date);
        if era.weekdays.is_empty() {
            return None;
        }
        let index = date.0.rem_euclid(era.weekdays.len() as i64) as usize;
        Some(&era.weekdays[index])
    }

    /// Formats a world date for display: `15 Hay Moon 812 AR`.
    pub fn format(&self, date: WorldDate) -> String {
        let (era, year, month, day) = self.to_ymd(date);
        format!("{day} {} {}", era.months[(month - 1) as usize].name, era.year_text(year))
    }

    /// A moment as written: its day — and its time where it has one.
    ///
    /// A moment is stored as a day count; the time of day is the fraction.
    pub fn format_moment(&self, value: f64) -> String {
        let (day, minutes) = split_moment(value);
        let date = self.format(WorldDate(day));
        match minutes {
            0 if value.fract() == 0.0 => date,
            m => format!("{date}, {:02}:{:02}", m / 60, m % 60),
        }
    }

    /// Reads a moment as written: `15 Hay Moon 812 AR`, `15. Hay Moon 812`,
    /// with `, 18:30` or ` 18:30` after it — or a bare day count.
    pub fn parse_moment(&self, text: &str) -> Option<f64> {
        let text = text.trim();
        if let Ok(n) = text.parse::<f64>() {
            return n.is_finite().then_some(n);
        }
        let (date, time) = split_time(text);
        let day = self.parse_date(date)?;
        Some(day.0 as f64 + time.map(|m| m as f64 / DAY_MINUTES).unwrap_or(0.0))
    }

    /// `15 Hay Moon 812 AR` — the day, a month's name, the year, the era's
    /// mark. Without a mark the date is of the last era.
    fn parse_date(&self, text: &str) -> Option<WorldDate> {
        let text = text.trim();
        // Which era: the longest mark the text ends with.
        let mut marks: Vec<(&Era, &str, bool)> = Vec::new();
        for era in &self.eras {
            marks.push((era, era.abbr.trim(), false));
        }
        if let Some(first) = self.eras.first() {
            marks.push((first, first.before.trim(), true));
        }
        marks.retain(|(_, m, _)| !m.is_empty());
        marks.sort_by_key(|(_, m, _)| std::cmp::Reverse(m.chars().count()));
        let found = marks.into_iter().find_map(|(era, mark, back)| {
            let rest = text.strip_suffix(mark)?;
            rest.ends_with(|c: char| c.is_whitespace() || c.is_ascii_digit()).then_some((era, rest.trim_end(), back))
        });
        let (era, text, back) = found.unwrap_or((self.eras.last()?, text, false));

        let digits = text.find(|c: char| !c.is_ascii_digit()).unwrap_or(text.len());
        let day: u32 = text[..digits].parse().ok()?;
        let rest = text[digits..].trim_start_matches('.').trim_start();
        // The longest name first: “Moon” must not win over “Moon of Ash”.
        let mut months: Vec<(usize, &Month)> = era.months.iter().enumerate().collect();
        months.sort_by_key(|(_, m)| std::cmp::Reverse(m.name.chars().count()));
        let (index, month) = months.into_iter().find(|(_, m)| {
            rest.get(..m.name.len()).is_some_and(|p| p.eq_ignore_ascii_case(&m.name))
                && rest[m.name.len()..].starts_with(|c: char| c.is_whitespace())
        })?;
        let year: i64 = rest[month.name.len()..].trim().parse().ok()?;
        let year = if back { 1 - year } else { year };
        era.to_date(year, index as u32 + 1, day).ok()
    }
}

/// A stored moment: the day, and the minutes into it.
fn split_moment(value: f64) -> (i64, i64) {
    let day = value.floor();
    let minutes = ((value - day) * DAY_MINUTES).round() as i64;
    // 23:59.99 rounds to the next day, not to 24:00.
    if minutes >= DAY_MINUTES as i64 { (day as i64 + 1, 0) } else { (day as i64, minutes) }
}

/// Splits `…, 18:30` or `… 18:30` into the date and the minutes of the day.
fn split_time(text: &str) -> (&str, Option<i64>) {
    let Some(at) = text.rfind([',', ' ']) else { return (text, None) };
    let (h, m) = match text[at + 1..].trim().split_once(':') {
        Some(hm) => hm,
        None => return (text, None),
    };
    match (h.parse::<i64>(), m.parse::<i64>()) {
        (Ok(h), Ok(m)) if (0..24).contains(&h) && (0..60).contains(&m) => (text[..at].trim_end_matches(',').trim(), Some(h * 60 + m)),
        _ => (text, None),
    }
}

impl Default for Era {
    /// A simple calendar: twelve months of 30 days, a six-day week.
    fn default() -> Self {
        let month_names = [
            "First Moon",
            "Thaw Moon",
            "Seed Moon",
            "Blossom Moon",
            "Pasture Moon",
            "Sun Moon",
            "Hay Moon",
            "Harvest Moon",
            "Mist Moon",
            "Fall Moon",
            "Frost Moon",
            "Dark Moon",
        ];
        Era {
            name: "Standard calendar".into(),
            abbr: "AR".into(),
            before: String::new(),
            months: month_names
                .iter()
                .map(|n| Month {
                    name: (*n).into(),
                    days: 30,
                })
                .collect(),
            weekdays: ["Firstday", "Secondday", "Thirdday", "Fourthday", "Fifthday", "Restday"]
                .iter()
                .map(|s| (*s).into())
                .collect(),
            leap: None,
            start: 0,
            moons: Vec::new(),
        }
    }
}

impl Default for WorldCalendar {
    fn default() -> Self {
        WorldCalendar { eras: vec![Era::default()] }
    }
}

#[cfg(test)]
#[path = "calendar_tests.rs"]
mod tests;
