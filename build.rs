//! Embeds the icon and version information into the Windows executable using the
//! Windows SDK resource compiler (rc.exe). Does nothing on other platforms, and only
//! warns if rc.exe cannot be found.

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=packaging/icons/meshcore.ico");
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || env::var("CARGO_CFG_TARGET_ENV").as_deref() != Ok("msvc") {
        return;
    }
    let Some(rc) = find_rc() else {
        println!("cargo:warning=rc.exe not found; the executable will have no icon");
        return;
    };
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    let ico = Path::new(&env::var("CARGO_MANIFEST_DIR").unwrap()).join("packaging/icons/meshcore.ico");
    let ver = env::var("CARGO_PKG_VERSION").unwrap();
    let mut v: Vec<u32> = ver.split(['.', '-']).filter_map(|p| p.parse().ok()).collect();
    v.resize(4, 0);
    let commas = format!("{},{},{},{}", v[0], v[1], v[2], v[3]);
    let script = format!(
        r#"1 ICON "{ico}"
1 VERSIONINFO
FILEVERSION {commas}
PRODUCTVERSION {commas}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "080904b0"
    BEGIN
      VALUE "CompanyName", "Simon Blandford"
      VALUE "FileDescription", "Rust Desktop Client for MeshCore"
      VALUE "FileVersion", "{ver}"
      VALUE "InternalName", "meshcore-rust-client"
      VALUE "LegalCopyright", "Copyright (C) Simon Blandford. GNU GPL v3."
      VALUE "OriginalFilename", "meshcore-rust-client.exe"
      VALUE "ProductName", "Rust Desktop Client for MeshCore"
      VALUE "ProductVersion", "{ver}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0809, 1200
  END
END
"#,
        ico = ico.display().to_string().replace('\\', "\\\\"),
    );
    let rc_file = out.join("meshcore.rc");
    let res_file = out.join("meshcore.res");
    std::fs::write(&rc_file, script).unwrap();
    let ok = Command::new(&rc).arg("/nologo").arg("/fo").arg(&res_file).arg(&rc_file).status().map(|s| s.success()).unwrap_or(false);
    if ok {
        // link.exe accepts compiled .res files as ordinary inputs.
        println!("cargo:rustc-link-arg-bins={}", res_file.display());
    } else {
        println!("cargo:warning=rc.exe failed; the executable will have no icon");
    }
}

/// rc.exe from $RC, PATH, or the newest installed Windows 10/11 SDK.
fn find_rc() -> Option<PathBuf> {
    if let Ok(p) = env::var("RC") {
        return Some(p.into());
    }
    if Command::new("rc.exe").arg("/?").output().is_ok() {
        return Some("rc.exe".into());
    }
    let arch = if env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("aarch64") { "arm64" } else { "x64" };
    let base = PathBuf::from(env::var("ProgramFiles(x86)").unwrap_or_else(|_| r"C:\Program Files (x86)".into())).join(r"Windows Kits\10\bin");
    let mut vers: Vec<PathBuf> = std::fs::read_dir(&base).ok()?.flatten().map(|e| e.path()).filter(|p| p.join(arch).join("rc.exe").exists()).collect();
    vers.sort();
    vers.pop().map(|p| p.join(arch).join("rc.exe"))
}
