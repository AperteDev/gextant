//! Compiles the Blueprint views in `ui/` and bundles them with the icon into a resource file.

use std::process::Command;

fn run(cmd: &mut Command, tool: &str) {
    let ok = cmd
        .status()
        .unwrap_or_else(|_| panic!("{tool} not found"))
        .success();
    assert!(ok, "{tool} failed");
}

fn main() {
    let out = std::env::var("OUT_DIR").unwrap();
    println!("cargo::rerun-if-changed=ui");
    println!("cargo::rerun-if-changed=data");
    let blueprints: Vec<_> = std::fs::read_dir("ui")
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "blp"))
        .collect();
    run(
        Command::new("blueprint-compiler")
            .args(["batch-compile", &out, "ui"])
            .args(&blueprints),
        "blueprint-compiler",
    );
    run(
        Command::new("glib-compile-resources")
            .args([
                "--sourcedir",
                &out,
                "--sourcedir",
                "ui",
                "--sourcedir",
                "data",
                "--target",
            ])
            .arg(format!("{out}/gextant.gresource"))
            .arg("ui/gextant.gresource.xml"),
        "glib-compile-resources",
    );
}
