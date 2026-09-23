use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    // Enable rpath for $ORIGIN and $ORIGIN/../lib and user's Steam runtime path
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/../lib");
    if let Ok(home) = env::var("HOME") {
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,{}/.local/share/Steam/steamrt64",
            home
        );
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,{}/.local/share/Steam/ubuntu12_64",
            home
        );
    }

    // Try finding libsteam_api.so and copying it next to the output binary
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    // target/{debug|release}/build/steam-idled-.../out -> target/{debug|release}
    let target_dir = out_dir.ancestors().nth(3).map(Path::to_path_buf);

    if let Some(target) = target_dir {
        // Search in target directory for built libsteam_api.so
        let build_dir = target.join("build");
        if let Ok(entries) = fs::read_dir(&build_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().starts_with("steamworks-sys-") {
                    let so_path = entry.path().join("out").join("libsteam_api.so");
                    if so_path.exists() {
                        let dest = target.join("libsteam_api.so");
                        let _ = fs::copy(&so_path, &dest);
                        break;
                    }
                }
            }
        }
    }
}
