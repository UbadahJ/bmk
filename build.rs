//! Records whether this build targets the host CPU, so `bmk -V` can report it.

fn main() {
    let flags = std::env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    // rustc uses the last -C target-cpu=..., so report that one.
    let native = flags
        .split('\x1f')
        .filter_map(|f| f.split_once("target-cpu=").map(|(_, cpu)| cpu))
        .next_back()
        == Some("native");
    println!(
        "cargo:rustc-env=BMK_BUILD={}",
        if native { "native" } else { "generic" }
    );
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
}
