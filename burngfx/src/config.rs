//! v1.2's `ConfigFile`, as `newgfx.txt` and the `.txt` font descriptions are
//! read (`Burntime.Platform/IO/ConfigFile.cs`, `ConfigSection.cs`,
//! `ConfigLineTemplate.cs`).
//!
//! What matters for those files and is easy to get wrong:
//!
//! * a comment starts at `'`, `;` or `//` outside a quoted string
//!   (`kerning1="T; P; F;"` keeps its semicolons);
//! * a value loses one leading and one trailing `"`, and `""` becomes `"`;
//! * keys and section names compare without case;
//! * a key written twice is **one** value, the lines joined by `\n` — the
//!   fonts' several `kerning1=` lines are all read;
//! * lines before the first `[section]` belong to a section named `""`, and
//!   the sections keep the file's order (`GetAllSections`).
//!
//! A file naming one section twice is refused, as `Open` returns false.

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Section {
    /// Lower case.
    pub name: String,
    /// `(key in lower case, value)`, in first-written order.
    values: Vec<(String, String)>,
}

impl Section {
    pub fn get(&self, key: &str) -> Option<&str> {
        let key = key.to_ascii_lowercase();
        self.values.iter().find(|(k, _)| *k == key).map(|(_, v)| v.as_str())
    }

    pub fn contains(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// `(key, value)` pairs in order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.values.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Config {
    pub sections: Vec<Section>,
}

enum Line {
    Empty,
    Section(String),
    Key(String, String),
    Invalid,
}

/// `ConfigLineTemplate`'s reading of one line.
fn read_line(line: &str) -> Line {
    let chars: Vec<char> = line.chars().collect();
    let mut inside = false;
    let mut has_string = false;
    let mut end = chars.len();
    let mut key: Option<(String, usize)> = None;
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if inside {
            has_string = true;
            if c == '"' {
                if chars.get(i + 1) == Some(&'"') {
                    i += 1;
                } else {
                    inside = false;
                }
            }
        } else if c == '"' {
            inside = true;
        } else if c == '\'' || c == ';' || (c == '/' && chars.get(i + 1) == Some(&'/')) {
            end = i;
            break;
        } else if c == '[' {
            if key.is_some() {
                return Line::Invalid;
            }
            let rest: String = chars[i + 1..].iter().collect();
            return match rest.find(']') {
                Some(0) | None => Line::Invalid,
                Some(at) => Line::Section(String::from(&rest[..at])),
            };
        } else if c == '=' {
            // Every `=` outside a string moves the split, as the reader's
            // loop does: `a=b=c` is key `a=b`.
            if has_string {
                return Line::Invalid;
            }
            let k: String = chars[..i].iter().collect();
            let k = String::from(k.trim());
            if k.is_empty() {
                return Line::Invalid;
            }
            key = Some((k, i + 1));
        }
        i += 1;
    }
    if inside {
        return Line::Invalid;
    }
    match key {
        None => Line::Empty,
        Some((k, from)) => {
            let raw: String = chars[from.min(end)..end].iter().collect();
            let mut v = raw.trim();
            v = v.strip_prefix('"').unwrap_or(v);
            v = v.strip_suffix('"').unwrap_or(v);
            Line::Key(k, v.replace("\"\"", "\""))
        }
    }
}

impl Config {
    /// `None` where `ConfigFile.Open` returns false: a section named twice.
    pub fn parse(text: &str) -> Option<Config> {
        let mut sections = alloc::vec![Section::default()];
        for line in text.trim_start_matches('\u{feff}').lines() {
            match read_line(line) {
                Line::Section(name) => {
                    let name = name.to_ascii_lowercase();
                    if sections.iter().any(|s| s.name == name) {
                        return None;
                    }
                    sections.push(Section { name, values: Vec::new() });
                }
                Line::Key(k, v) => {
                    let k = k.to_ascii_lowercase();
                    let s = sections.last_mut().expect("there is always a section");
                    match s.values.iter_mut().find(|(key, _)| *key == k) {
                        Some((_, old)) => {
                            old.push('\n');
                            old.push_str(&v);
                        }
                        None => s.values.push((k, v)),
                    }
                }
                Line::Empty | Line::Invalid => {}
            }
        }
        Some(Config { sections })
    }

    /// `config[name]`: the section, or an empty one.
    pub fn section(&self, name: &str) -> Option<&Section> {
        let name = name.to_ascii_lowercase();
        self.sections.iter().find(|s| s.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_quotes_and_repeats_read_as_v12_reads_them() {
        let text = "a=1 ' note\n[One]\nKey = \"x; y\" ; gone\nk=1\nk=2\n// all comment\nq=\"say \"\"hi\"\"\"\n[two]\nb=2";
        let c = Config::parse(text).unwrap();
        assert_eq!(c.sections.len(), 3);
        assert_eq!(c.section("").unwrap().get("A"), Some("1"));
        let one = c.section("ONE").unwrap();
        assert_eq!(one.get("key"), Some("x; y"));
        assert_eq!(one.get("k"), Some("1\n2"));
        assert_eq!(one.get("q"), Some("say \"hi\""));
        assert_eq!(c.section("two").unwrap().get("b"), Some("2"));
    }

    #[test]
    fn a_section_named_twice_refuses_the_file() {
        assert_eq!(Config::parse("[a]\nx=1\n[A]\ny=2"), None);
    }
}
