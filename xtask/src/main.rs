use std::{env, fs, path::PathBuf, process::Command};

const TARGET: &str = "xtensa-esp32s3-none-elf";
const EXAMPLES: &[&str] = &["hello_world", "dirty_regions", "gateway_h2"];

fn main() {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("firmware") => firmware(args.any(|arg| arg == "--release")),
        _ => {
            eprintln!("usage: cargo xtask firmware [--release]");
            std::process::exit(2);
        }
    }
}

fn firmware(release: bool) {
    let profile = if release { "release" } else { "debug" };
    let out_dir = PathBuf::from("target/firmware");
    fs::create_dir_all(&out_dir).expect("create target/firmware");

    for package in EXAMPLES {
        let mut build = Command::new("cargo");
        build.args(["build", "-p", package, "--target", TARGET]);
        if release {
            build.arg("--release");
        }
        if *package == "gateway_h2" {
            build.args(["--features", "gateway-h2"]);
        }
        run(build);

        let elf = PathBuf::from("target")
            .join(TARGET)
            .join(profile)
            .join(package);
        let elf_out = out_dir.join(format!("core-s3-{package}.elf"));
        fs::copy(&elf, &elf_out).unwrap_or_else(|err| panic!("copy {}: {err}", elf.display()));
    }
}

fn run(mut command: Command) {
    let status = command
        .status()
        .unwrap_or_else(|err| panic!("failed to spawn {command:?}: {err}"));
    if !status.success() {
        panic!("{command:?} exited with {status}");
    }
}
