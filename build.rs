fn main() {
    // DuckDB's bundled C++ uses the Windows Restart Manager API (Rm* symbols)
    // for nicer "file in use" diagnostics. Those live in rstrtmgr.lib which
    // isn't pulled in automatically on the MSVC target.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        println!("cargo:rustc-link-lib=rstrtmgr");
    }
}
