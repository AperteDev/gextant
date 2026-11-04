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
    run(
        Command::new("blueprint-compiler").args(["batch-compile", &out, "ui", "ui/window.blp"]),
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
