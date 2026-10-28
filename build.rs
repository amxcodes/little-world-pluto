use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=island-ui/dist/index.html");
    println!("cargo:rerun-if-env-changed=CARGO_PKG_VERSION");
    println!("cargo:rerun-if-env-changed=PATH");
    println!("cargo:rerun-if-env-changed=ProgramFiles(x86)");
    println!("cargo:rerun-if-env-changed=PLUTO_REQUIRE_WINDOWS_VERSION_RESOURCE");
    println!("cargo:rerun-if-env-changed=PLUTO_SIGNING_COMPANY_NAME");
    println!("cargo:rerun-if-env-changed=PLUTO_SIGNING_PUBLISHER");
    println!("cargo:rerun-if-env-changed=PLUTO_SIGNING_CERT_THUMBPRINT");

    if env::var("CARGO_CFG_TARGET_OS").ok().as_deref() != Some("windows")
        || env::var("CARGO_CFG_TARGET_ENV").ok().as_deref() != Some("msvc")
        || env::var_os("CARGO_FEATURE_PRODUCT_APP").is_none()
    {
        return;
    }

    let required = env::var_os("PLUTO_REQUIRE_WINDOWS_VERSION_RESOURCE").is_some();
    if let Err(error) = embed_windows_version_resource() {
        if required {
            panic!("Could not embed Pluto's Windows version resource: {error}");
        }
        println!("cargo:warning=Windows version information was not embedded: {error}");
    }
}

fn embed_windows_version_resource() -> Result<(), String> {
    let version = env::var("CARGO_PKG_VERSION").map_err(|error| error.to_string())?;
    let parts = version
        .split('.')
        .map(|part| part.parse::<u16>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| format!("package version {version:?} must have numeric components"))?;
    if parts.len() != 3 {
        return Err(format!(
            "package version {version:?} must be major.minor.patch"
        ));
    }

    let numeric_version = format!("{},{},{},0", parts[0], parts[1], parts[2]);
    let string_version = format!("{}.{}.{}.0", parts[0], parts[1], parts[2]);
    let company_name = env::var("PLUTO_SIGNING_COMPANY_NAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Littleworld Pluto".to_owned());
    let company_name = company_name
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace(['\r', '\n'], " ");
    let resource = format!(
        r#"
1 VERSIONINFO
 FILEVERSION {numeric_version}
 PRODUCTVERSION {numeric_version}
 FILEFLAGSMASK 0x3fL
 FILEFLAGS 0x0L
 FILEOS 0x00040004L
 FILETYPE 0x00000001L
 FILESUBTYPE 0x00000000L
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "{company_name}\0"
      VALUE "FileDescription", "A compact Windows desktop workspace for everyday tools and AI conversations\0"
      VALUE "FileVersion", "{string_version}\0"
      VALUE "InternalName", "pluto\0"
      VALUE "OriginalFilename", "pluto.exe\0"
      VALUE "ProductName", "Littleworld Pluto\0"
      VALUE "ProductVersion", "{string_version}\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#
    );

    let out_dir = PathBuf::from(
        env::var_os("OUT_DIR").ok_or_else(|| "Cargo did not provide OUT_DIR".to_owned())?,
    );
    let rc_file = out_dir.join("pluto-version.rc");
    let res_file = out_dir.join("pluto-version.res");
    fs::write(&rc_file, resource).map_err(|error| error.to_string())?;

    let rc = find_resource_compiler()?;
    let output = Command::new(&rc)
        .arg("/nologo")
        .arg("/fo")
        .arg(&res_file)
        .arg(&rc_file)
        .output()
        .map_err(|error| format!("could not start {}: {error}", rc.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed: {}{}",
            rc.display(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    println!("cargo:rustc-link-arg-bin=pluto={}", res_file.display());
    Ok(())
}

fn find_resource_compiler() -> Result<PathBuf, String> {
    for directory in env::split_paths(&env::var_os("PATH").unwrap_or_default()) {
        let candidate = directory.join("rc.exe");
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let program_files = env::var_os("ProgramFiles(x86)")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files (x86)"));
    let kit_bin = program_files.join(r"Windows Kits\10\bin");
    let architecture = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
        Ok("x86_64") => "x64",
        Ok("aarch64") => "arm64",
        Ok("x86") => "x86",
        other => return Err(format!("unsupported MSVC target architecture: {other:?}")),
    };

    let entries = fs::read_dir(&kit_bin).map_err(|error| {
        format!(
            "Windows SDK resource compiler not found under {}: {error}",
            kit_bin.display()
        )
    })?;
    let mut versions = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect::<Vec<_>>();
    versions.sort_by_key(|entry| std::cmp::Reverse(sdk_version(entry)));

    versions
        .into_iter()
        .map(|version| version.join(architecture).join("rc.exe"))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| {
            format!(
                "rc.exe for {architecture} was not found in {}",
                kit_bin.display()
            )
        })
}

fn sdk_version(path: &Path) -> Vec<u32> {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .split('.')
        .map(str::parse::<u32>)
        .collect::<Result<Vec<_>, _>>()
        .unwrap_or_default()
}
