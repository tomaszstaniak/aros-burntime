//! The remaster's `burntime.pak`, which carries the original 1993 game data.
//!
//! Not to be confused with [`crate::pack`]: that one is ours, written by our
//! packer and compiled into the AROS build. This one is upstream's, it holds
//! the classic files, and nothing on AROS ever opens it.
//!
//! It is worth reading because everything we have been working around is
//! inside it — `burn_gfx/gam.dat` with the real entry points and road graph,
//! `zei_014.raw` and `zei_015.raw` with the tiles no rendered PNG covers, and
//! `raum_0.pac` upward with the room interiors.
//!
//! ```text
//! count    i32
//! version  i32
//! type     i32
//! entries  count * { position i32, length i32, name (BinaryWriter string) }
//! data                     ... at `position` relative to the end of the table
//! ```
//!
//! Ported from `Burntime.Platform/IO/PackagePak.cs`. Two details from there
//! matter: names are lower-cased on read, and `position` is relative to where
//! the table ends rather than to the start of the file.

use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, PartialEq, Eq)]
pub enum PakError {
    /// Too short, or an entry reaches outside the file.
    Malformed,
    /// A count or offset that cannot be right.
    Implausible,
}

pub struct Entry {
    /// Lower-cased, as the original stores it, e.g. `burn_gfx/gam.dat`.
    pub name: String,
    offset: usize,
    length: usize,
}

/// `Debug` prints the shape, not the megabytes it points at.
impl core::fmt::Debug for ClassicPak<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "ClassicPak {{ entries: {}, version: {}, bytes: {} }}",
            self.entries.len(),
            self.version,
            self.bytes.len()
        )
    }
}

pub struct ClassicPak<'a> {
    bytes: &'a [u8],
    pub version: i32,
    pub kind: i32,
    entries: Vec<Entry>,
}

impl<'a> ClassicPak<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<ClassicPak<'a>, PakError> {
        let mut r = Reader { b: bytes, at: 0 };

        let count = r.i32()?;
        let version = r.i32()?;
        let kind = r.i32()?;
        // The real archive holds a few hundred files; a count in the millions
        // is a corrupt header, not an ambitious one.
        if !(0..=100_000).contains(&count) {
            return Err(PakError::Implausible);
        }

        let mut raw = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let position = r.i32()?;
            let length = r.i32()?;
            let name = r.string()?;
            if position < 0 || length < 0 {
                return Err(PakError::Implausible);
            }
            raw.push((position as usize, length as usize, name));
        }

        // Offsets are relative to the end of the table, which is wherever the
        // last name finished.
        let base = r.at;

        let mut entries = Vec::with_capacity(raw.len());
        for (position, length, name) in raw {
            let offset = base.checked_add(position).ok_or(PakError::Malformed)?;
            let end = offset.checked_add(length).ok_or(PakError::Malformed)?;
            if end > bytes.len() {
                return Err(PakError::Malformed);
            }
            entries.push(Entry { name, offset, length });
        }

        Ok(ClassicPak { bytes, version, kind, entries })
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    pub fn data(&self, entry: &Entry) -> &'a [u8] {
        &self.bytes[entry.offset..entry.offset + entry.length]
    }

    /// Look a file up by its path inside the archive. Matching is
    /// case-insensitive, because the archive lower-cases what it stores and
    /// callers write names the way the game does.
    pub fn get(&self, name: &str) -> Option<&'a [u8]> {
        self.entries
            .iter()
            .find(|e| e.name.len() == name.len() && e.name.eq_ignore_ascii_case(name))
            .map(|e| self.data(e))
    }

    pub fn contains(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn i32(&mut self) -> Result<i32, PakError> {
        let end = self.at.checked_add(4).ok_or(PakError::Malformed)?;
        let s = self.b.get(self.at..end).ok_or(PakError::Malformed)?;
        self.at = end;
        Ok(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    /// A .NET `BinaryWriter` string: 7-bit-encoded byte length, then UTF-8.
    fn string(&mut self) -> Result<String, PakError> {
        let mut len = 0usize;
        let mut shift = 0;
        loop {
            let byte = *self.b.get(self.at).ok_or(PakError::Malformed)?;
            self.at += 1;
            len |= ((byte & 0x7f) as usize) << shift;
            if byte & 0x80 == 0 {
                break;
            }
            shift += 7;
            if shift > 28 {
                return Err(PakError::Implausible);
            }
        }
        let end = self.at.checked_add(len).ok_or(PakError::Malformed)?;
        let s = self.b.get(self.at..end).ok_or(PakError::Malformed)?;
        self.at = end;
        // The archive is ASCII paths; anything else is a corrupt table.
        core::str::from_utf8(s).map(String::from).map_err(|_| PakError::Malformed)
    }
}
