use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    for variable in [
        "QUIRE_VERSION",
        "QUIRE_GETTEXT_PACKAGE",
        "QUIRE_LOCALEDIR",
        "QUIRE_PKGDATADIR",
    ] {
        println!("cargo:rerun-if-env-changed={variable}");
    }

    let version = env::var("QUIRE_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into());
    let gettext_package = env::var("QUIRE_GETTEXT_PACKAGE").unwrap_or_else(|_| "quire".into());
    let locale_dir =
        env::var("QUIRE_LOCALEDIR").unwrap_or_else(|_| "/usr/local/share/locale".into());
    let pkgdata_dir =
        env::var("QUIRE_PKGDATADIR").unwrap_or_else(|_| "/usr/local/share/quire".into());

    let config = format!(
        "pub static VERSION: &str = {version:?};\n\
         pub static GETTEXT_PACKAGE: &str = {gettext_package:?};\n\
         pub static LOCALEDIR: &str = {locale_dir:?};\n\
         pub static PKGDATADIR: &str = {pkgdata_dir:?};\n"
    );

    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR should be set by Cargo"));
    fs::write(out_dir.join("config.rs"), config).expect("config.rs should be written");
}
