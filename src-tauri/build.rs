//! What the wallet is compiled to believe about the platform.
//!
//! **The one platform this wallet answers to is decided here**, and it is
//! decided in a build script rather than anywhere cargo has to be asked for it.
//!
//! It used to live in `.cargo/config.toml` under `[env]`, and that was wrong in
//! a way nothing caught: **cargo looks for its configuration by walking up from
//! the directory it was invoked in, not from the manifest it was pointed at.**
//! `tauri ios build` runs cargo from the wallet root, one level above this one,
//! so `src-tauri/.cargo/config.toml` was never read on the path that mattered.
//! The desktop build read it and the phone build did not, and the difference
//! was invisible: `option_env!` simply answered `None`, the code took its
//! development fallback, and a wallet installed on a phone refused every code
//! the real platform showed it — while saying only that the request pointed
//! somewhere that is not Almena ID.
//!
//! A build script cannot be lost that way. It belongs to the crate, so it runs
//! wherever and however cargo was started, and what it prints is handed to the
//! compilation directly.

/// The platform a build answers to when nobody says otherwise.
///
/// Not a localhost address. A wallet that quietly falls back to a development
/// server is a wallet that fails on somebody's phone for a reason nobody can
/// see, which is exactly what happened. Pointing a build somewhere else is done
/// on purpose, by setting `ALMENA_PLATFORM_URL`, and never by omission.
const PLATFORM: &str = "https://api.almena.id";

fn main() {
    // An exported value still wins, so a platform running on this machine stays
    // one variable away. Cargo does not track what `env!` reads in the crate's
    // own source, which is why this is declared rather than assumed.
    println!("cargo::rerun-if-env-changed=ALMENA_PLATFORM_URL");

    let platform = match std::env::var("ALMENA_PLATFORM_URL") {
        Ok(url) if !url.trim().is_empty() => url,
        _ => PLATFORM.to_string(),
    };

    // A release build that answers a development address would be handed to
    // somebody as if it were the real thing. It is worth refusing to compile.
    let release = std::env::var("PROFILE").is_ok_and(|profile| profile == "release");
    if release && (platform.contains("localhost") || platform.contains("127.0.0.1")) {
        panic!(
            "this release build would answer {platform}, which is a development address. \
             Set ALMENA_PLATFORM_URL, or leave it unset to answer {PLATFORM}."
        );
    }

    println!("cargo::rustc-env=ALMENA_PLATFORM_URL={platform}");

    tauri_build::build()
}
