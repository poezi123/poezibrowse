fn main() {
    if std::env::var_os("CARGO_FEATURE_CEF").is_some() {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib/poezibrowse");
        println!("cargo:rustc-link-arg=-Wl,--disable-new-dtags");
    }
}
