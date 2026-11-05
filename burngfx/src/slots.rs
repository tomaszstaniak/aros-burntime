//! The Saves page's slots (`OptionsSavesPage`): the `.sav` files in `saves/`,
//! the first ten of them, each shown upper case.
//!
//! v0.9.1 lists the directory (`FileSystem.GetFileNames("saves/", ".sav")`)
//! and so does this: whatever put a save there — the page, the `s` key, a
//! copy made by hand — it is on the page. The files are this port's own save
//! format; only the page is v0.9.1's.

use alloc::string::String;
use alloc::vec::Vec;

/// `MaxNameLength` of `SavegameInputWindow`.
pub const MAX_NAME: usize = 8;
/// Slots on the page.
pub const SLOTS: usize = 10;

/// From a directory's entry names, the slots: the `.sav` files, by name
/// (the order a Windows directory listing gives v0.9.1), lower case and
/// without the extension, at most ten.
pub fn from_listing<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = names
        .into_iter()
        .filter_map(|n| {
            let lower = n.to_lowercase();
            let stem = lower.strip_suffix(".sav")?;
            (!stem.is_empty()).then(|| String::from(stem))
        })
        .collect();
    out.sort();
    out.dedup();
    out.truncate(SLOTS);
    out
}

/// Names compare and are stored in lower case; the page shows them upper.
/// A typed name is at most eight characters; a file found in the directory
/// keeps the name it has.
pub fn normal(name: &str) -> String {
    name.to_lowercase()
}

/// `saves/<name>.sav` under `dir`, which ends in a separator.
pub fn file_of(dir: &str, name: &str) -> String {
    let mut p = String::from(dir);
    p.push_str(&normal(name));
    p.push_str(".sav");
    p
}

// -- v1.2's Saves page (`OptionsSavesPage` at v1.2) --------------------------

use crate::save::Stamp;

/// Every `.sav` in the listing, lower case and without the extension — v1.2
/// lists them all and scrolls; it does not stop at ten.
pub fn every_save<'a>(names: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = names
        .into_iter()
        .filter_map(|n| {
            let lower = n.to_lowercase();
            let stem = lower.strip_suffix(".sav")?;
            (!stem.is_empty()).then(|| String::from(stem))
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// `GetFileNameSlug`: the name's letters and digits, lower case, a `-` for
/// each run of anything else between them, `player` when nothing is left.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut last_was_separator = false;
    for c in name.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
            last_was_separator = false;
        } else if !last_was_separator && !out.is_empty() {
            out.push('-');
            last_was_separator = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() { String::from("player") } else { out }
}

/// `GetNextSaveFileName`: `<slug>-saveNNN`, one past the highest number the
/// listing has for that player, never one that exists.
pub fn next_save_name(player: &str, existing: &[String]) -> String {
    let mut prefix = slug(player);
    prefix.push_str("-save");
    let mut highest = existing
        .iter()
        .filter_map(|n| n.strip_prefix(prefix.as_str())?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    loop {
        highest += 1;
        let mut name = prefix.clone();
        let n = highest;
        if n < 100 {
            name.push('0');
        }
        if n < 10 {
            name.push('0');
        }
        let _ = core::fmt::Write::write_fmt(&mut name, format_args!("{n}"));
        if !existing.iter().any(|e| *e == name) {
            return name;
        }
    }
}

/// `GetDisplayName`: the file's name without a `-saveNNN` it was given.
pub fn display_name(stem: &str) -> &str {
    match stem.rfind("-save") {
        Some(at) if at > 0 && !stem[at + 5..].is_empty() && stem[at + 5..].bytes().all(|b| b.is_ascii_digit()) => &stem[..at],
        _ => stem,
    }
}

/// `ToString("MMM-dd HH:mm")` of a [`Stamp`].
pub fn stamp_text(t: Stamp) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (_, month, day) = civil_from_days(t.days as i64 + DAYS_1970_TO_1978);
    let two = |n: i64, s: &mut String| {
        s.push((b'0' + (n / 10 % 10) as u8) as char);
        s.push((b'0' + (n % 10) as u8) as char);
    };
    let mut s = String::from(MONTHS[(month - 1) as usize]);
    s.push('-');
    two(day, &mut s);
    s.push(' ');
    two((t.minute / 60) as i64, &mut s);
    s.push(':');
    two((t.minute % 60) as i64, &mut s);
    s
}

/// Days from 1 January 1970 to 1 January 1978, dos.library's epoch.
pub const DAYS_1970_TO_1978: i64 = 2922;

/// A Unix time as a [`Stamp`] (in UTC: the host has no time zone here).
pub fn stamp_of_unix(seconds: i64) -> Stamp {
    let days = seconds.div_euclid(86_400);
    Stamp { days: (days - DAYS_1970_TO_1978) as i32, minute: (seconds.rem_euclid(86_400) / 60) as i32 }
}

/// Howard Hinnant's `civil_from_days`: days since 1970-01-01 to a date.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}
