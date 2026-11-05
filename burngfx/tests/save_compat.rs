//! The first release is the baseline of save compatibility: a save from
//! an earlier test build (here `0.1.0-aros6`'s, v1.2 rules, another pack)
//! is refused, and the refusal says so in words. The owner's decision of
//! 2026-11-03 (docs/compliance-v1.3.md, "Saves").
mod common;

use burngfx::pack::Pack;
use burngfx::save::{self, SaveError};

const AROS6_PACK: u64 = 0xa905_3410_8c23_7dd7;

#[test]
fn a_test_builds_save_is_refused_in_words() {
    let Some(bytes) = common::read("saves-aros6/modern-c.sav") else { return };
    let Some(pack_bytes) = common::read("burntime.pack") else { return };
    let text = String::from_utf8(bytes).expect("a save is text");
    let pack = Pack::open(&pack_bytes).expect("the developer's pack");
    let now = pack.rules_fingerprint();
    let h = save::header(&text).expect("its header reads");
    assert_eq!(h.pack, AROS6_PACK);
    let e = save::parse(&text, now, h.identity, None).unwrap_err();
    assert_eq!(e, SaveError::WrongPack { found: AROS6_PACK, want: now });
    let words = e.explain();
    assert!(words.contains("test build before the first release"), "{words}");
    assert!(words.contains("a90534108c237dd7"), "{words}");
}

#[test]
fn every_refusal_has_words() {
    for e in [
        SaveError::NotASave,
        SaveError::WrongVersion { found: 25, want: save::VERSION },
        SaveError::WrongPack { found: 1, want: 2 },
        SaveError::WrongRules,
        SaveError::Truncated,
        SaveError::BadLine("x".into()),
    ] {
        let w = e.explain();
        assert!(!w.is_empty() && !w.starts_with("WrongPack"), "{w}");
    }
}
