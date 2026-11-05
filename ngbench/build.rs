//! On AROS, compile `csrc/availmem.c` with the cross compiler the `aros`
//! crate uses (`AROS_GCC`, `AROS_SDK` from `rust-aros/aros-env.toml`); on the
//! host there is nothing to build. Both record the commit in the results.
use std::{env, path::Path, process::Command};

fn main() {
    println!("cargo::rustc-check-cfg=cfg(target_os, values(\"aros\"))");
    println!("cargo:rerun-if-changed=csrc/availmem.c");
    let commit = Command::new("git").args(["rev-parse", "--short", "HEAD"]).output().ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|| String::from("unknown"));
    let dirty = Command::new("git").args(["status", "--porcelain", "--", "../ngbench", "../aros-game/src/gfx.rs", "../burngfx/src"])
        .output().ok().is_some_and(|o| !o.stdout.is_empty());
    println!("cargo:rustc-env=NGBENCH_COMMIT={commit}{}", if dirty { "+dirty" } else { "" });
    // Run again on every commit and on any change to what is measured, so
    // the commit written into the results is the one built.
    if let Some(dir) = Command::new("git").args(["rev-parse", "--absolute-git-dir"]).output().ok()
        .filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()) {
        println!("cargo:rerun-if-changed={dir}/logs/HEAD");
    }
    for f in ["src", "../aros-game/src/gfx.rs", "../burngfx/src"] {
        println!("cargo:rerun-if-changed={f}");
    }

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("aros") {
        return;
    }
    println!("cargo:rerun-if-env-changed=AROS_GCC");
    println!("cargo:rerun-if-env-changed=AROS_SDK");
    let gcc = env::var("AROS_GCC").expect("AROS_GCC: the AROS cross compiler (rust-aros/aros-env.toml)");
    let sdk = env::var("AROS_SDK").expect("AROS_SDK: the AROS SDK (rust-aros/aros-env.toml)");
    let ar = env::var("AROS_AR").unwrap_or_else(|_| format!("{}ar", gcc.strip_suffix("gcc").expect("set AROS_AR")));
    let out = env::var("OUT_DIR").unwrap();
    let src = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("csrc/availmem.c");
    let obj = format!("{out}/availmem.o");
    let ok = Command::new(&gcc)
        .args([&format!("--sysroot={sdk}"), &format!("-I{sdk}/include"), "-O2", "-c"])
        .arg(&src).args(["-o", &obj]).status().map(|s| s.success()).unwrap_or(false);
    assert!(ok, "availmem.c did not compile with {gcc}");
    let ok = Command::new(&ar).args(["rcs", &format!("{out}/libngbenchglue.a"), &obj]).status()
        .map(|s| s.success()).unwrap_or(false);
    assert!(ok, "could not archive availmem.o");
    println!("cargo:rustc-link-search=native={out}");
    println!("cargo:rustc-link-lib=static=ngbenchglue");
}
