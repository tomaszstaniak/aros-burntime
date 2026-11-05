//! Test inputs that are not in the repository (game data and the
//! remaster's files, burngfx/testdata/README.md).
//!
//! **A missing input is a failure, not a pass.** A test that needs one
//! panics naming it; set `BURNTIME_ALLOW_MISSING_TESTDATA=1` to skip such
//! tests instead, which each then prints `SKIPPED` with the file it lacked.
#![allow(dead_code)]

pub fn path(rel: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata").join(rel)
}

/// The bytes of `testdata/<rel>`, or `None` after printing `SKIPPED` when
/// missing inputs are allowed; otherwise a panic.
pub fn read(rel: &str) -> Option<Vec<u8>> {
    match std::fs::read(path(rel)) {
        Ok(b) => Some(b),
        Err(e) => missing(rel, &e.to_string()),
    }
}

pub fn read_text(rel: &str) -> Option<String> {
    read(rel).map(|b| String::from_utf8(b).unwrap_or_else(|_| panic!("testdata/{rel} is not UTF-8")))
}

pub fn missing<T>(rel: &str, why: &str) -> Option<T> {
    if std::env::var_os("BURNTIME_ALLOW_MISSING_TESTDATA").is_some() {
        eprintln!("SKIPPED: testdata/{rel} missing ({why})");
        None
    } else {
        panic!("testdata/{rel} missing ({why}); see burngfx/testdata/README.md, or set BURNTIME_ALLOW_MISSING_TESTDATA=1 to skip");
    }
}
