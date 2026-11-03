//! Pluggable calendars over an explicitly advanced fictional clock.
use crate::GameState;
use dnd_protocol::*;
fn label(ru: &str, en: &str) -> Label {
    Label::Localized(LocalizedText {
        ru: Some(ru.into()),
        en: Some(en.into()),
    })
}
pub(crate) fn earth() -> CalendarDef {
    CalendarDef {
        epoch_year: 1,
        day_seconds: 86400,
        round_seconds: 6,
        months: [
            ("Январь", "January", 31),
            ("Февраль", "February", 28),
            ("Март", "March", 31),
            ("Апрель", "April", 30),
            ("Май", "May", 31),
            ("Июнь", "June", 30),
            ("Июль", "July", 31),
            ("Август", "August", 31),
            ("Сентябрь", "September", 30),
            ("Октябрь", "October", 31),
            ("Ноябрь", "November", 30),
            ("Декабрь", "December", 31),
        ]
        .into_iter()
        .map(|(ru, en, days)| CalendarMonth {
            label: label(ru, en),
            days,
        })
        .collect(),
        weekdays: [
            ("Понедельник", "Monday"),
            ("Вторник", "Tuesday"),
            ("Среда", "Wednesday"),
            ("Четверг", "Thursday"),
            ("Пятница", "Friday"),
            ("Суббота", "Saturday"),
            ("Воскресенье", "Sunday"),
        ]
        .into_iter()
        .map(|(ru, en)| label(ru, en))
        .collect(),
        periods: [
            (0, "Ночь", "Night"),
            (21600, "Рассвет", "Dawn"),
            (28800, "День", "Day"),
            (64800, "Закат", "Sunset"),
            (72000, "Ночь", "Night"),
        ]
        .into_iter()
        .map(|(start_second, ru, en)| DayPeriod {
            start_second,
            label: label(ru, en),
        })
        .collect(),
        leap_interval: 4,
        leap_except: 100,
        leap_include: 400,
        leap_month: 1,
    }
}
pub(crate) fn definition(state: &GameState) -> CalendarDef {
    state
        .rules()
        .and_then(|r| r.mechanics.calendar.clone())
        .unwrap_or_else(earth)
}
pub(crate) fn valid(c: &CalendarDef) -> bool {
    c.epoch_year > 0
        && (60..=604800).contains(&c.day_seconds)
        && (1..=86400).contains(&c.round_seconds)
        && !c.months.is_empty()
        && c.months.len() <= 64
        && c.months.iter().all(|m| (1..=1000).contains(&m.days))
        && !c.weekdays.is_empty()
        && c.weekdays.len() <= 64
        && !c.periods.is_empty()
        && c.periods.len() <= 32
        && c.periods[0].start_second == 0
        && c.periods.iter().all(|p| p.start_second < c.day_seconds)
        && c.periods
            .windows(2)
            .all(|p| p[0].start_second < p[1].start_second)
        && usize::from(c.leap_month) < c.months.len()
        && c.leap_interval <= 10000
        && c.leap_except <= 10000
        && c.leap_include <= 10000
        && (c.leap_except == 0
            || (c.leap_interval != 0 && c.leap_except.is_multiple_of(c.leap_interval)))
        && (c.leap_include == 0
            || (c.leap_except != 0 && c.leap_include.is_multiple_of(c.leap_except)))
        && (c.leap_interval != 0 || (c.leap_except == 0 && c.leap_include == 0))
}
fn leap(c: &CalendarDef, y: i64) -> bool {
    c.leap_interval != 0
        && y % i64::from(c.leap_interval) == 0
        && (c.leap_except == 0
            || y % i64::from(c.leap_except) != 0
            || (c.leap_include != 0 && y % i64::from(c.leap_include) == 0))
}
fn before_year(c: &CalendarDef, y: i64) -> i128 {
    let n = i128::from(y - 1);
    let days: i128 = c.months.iter().map(|m| i128::from(m.days)).sum();
    n * days
        + if c.leap_interval == 0 {
            0
        } else {
            n / i128::from(c.leap_interval)
                - if c.leap_except == 0 {
                    0
                } else {
                    n / i128::from(c.leap_except)
                }
                + if c.leap_include == 0 {
                    0
                } else {
                    n / i128::from(c.leap_include)
                }
        }
}
fn month_days(c: &CalendarDef, y: i64, m: usize) -> i64 {
    i64::from(c.months[m].days) + i64::from(m == usize::from(c.leap_month) && leap(c, y))
}
fn date(c: &CalendarDef, seconds: i64) -> (i64, usize, i64) {
    let target =
        before_year(c, i64::from(c.epoch_year)) + i128::from(seconds / i64::from(c.day_seconds));
    let mut lo = i64::from(c.epoch_year);
    let mut hi = lo + seconds / i64::from(c.day_seconds) + 2;
    while lo + 1 < hi {
        let mid = lo + (hi - lo) / 2;
        if before_year(c, mid) <= target {
            lo = mid
        } else {
            hi = mid
        }
    }
    let mut days = target - before_year(c, lo);
    let mut month = 0;
    while month + 1 < c.months.len() && days >= i128::from(month_days(c, lo, month)) {
        days -= i128::from(month_days(c, lo, month));
        month += 1;
    }
    (lo, month, i64::try_from(days).unwrap_or(0) + 1)
}
pub(crate) fn view(state: &GameState) -> CalendarView {
    let c = definition(state);
    let seconds = state.campaign.game_clock.seconds.max(0);
    let (year, month, day) = date(&c, seconds);
    let in_day = seconds % i64::from(c.day_seconds);
    let day_index = seconds / i64::from(c.day_seconds);
    let period = c
        .periods
        .iter()
        .rev()
        .find(|p| i64::from(p.start_second) <= in_day)
        .map(|p| p.label.clone())
        .unwrap_or_else(|| Label::Text(String::new()));
    CalendarView {
        year,
        month: c.months[month].label.clone(),
        month_index: u16::try_from(month).unwrap_or(0),
        day: u16::try_from(day).unwrap_or(1),
        weekday: c.weekdays[usize::try_from(
            day_index % i64::try_from(c.weekdays.len()).unwrap_or(1),
        )
        .unwrap_or(0)]
        .clone(),
        hour: u16::try_from(in_day / 3600).unwrap_or(0),
        minute: u8::try_from(in_day / 60 % 60).unwrap_or(0),
        second: u8::try_from(in_day % 60).unwrap_or(0),
        period,
        elapsed_days: day_index,
    }
}
pub(crate) fn delta(
    state: &GameState,
    amount: u16,
    unit: CalendarUnit,
) -> Result<i64, RejectReason> {
    if !(1..=10000).contains(&amount) {
        return Err(RejectReason::invalid("amount"));
    }
    let c = definition(state);
    let n = i64::from(amount);
    let day = i64::from(c.day_seconds);
    let now = state.campaign.game_clock.seconds;
    let delta = match unit {
        CalendarUnit::Second => n,
        CalendarUnit::Minute => n * 60,
        CalendarUnit::Hour => n * 3600,
        CalendarUnit::Day => n * day,
        CalendarUnit::Week => n * day * i64::try_from(c.weekdays.len()).unwrap_or(1),
        CalendarUnit::Round => n * i64::from(c.round_seconds),
        CalendarUnit::Month | CalendarUnit::Year => {
            let (y, m, d) = date(&c, now);
            let count = i64::try_from(c.months.len()).unwrap_or(1);
            let target = y * count
                + i64::try_from(m).unwrap_or(0)
                + if unit == CalendarUnit::Year {
                    n * count
                } else {
                    n
                };
            let year = target / count;
            let month = usize::try_from(target % count).unwrap_or(0);
            let prefix: i128 = (0..month)
                .map(|m| i128::from(month_days(&c, year, m)))
                .sum();
            let days = before_year(&c, year) - before_year(&c, i64::from(c.epoch_year))
                + prefix
                + i128::from(d.min(month_days(&c, year, month)) - 1);
            i64::try_from(days * i128::from(day) + i128::from(now % day) - i128::from(now))
                .map_err(|_| RejectReason::invalid("amount"))?
        }
    };
    if !(1..=10_000_000_000).contains(&delta) {
        return Err(RejectReason::invalid("amount"));
    }
    Ok(delta)
}
