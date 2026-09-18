use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn main() {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let template_root = rustashop_template_admin_default::root();
    std::fs::create_dir_all(manifest.join("generated")).expect("create generated");
    let css_out = manifest.join("generated/components.css");

    println!("cargo:rerun-if-changed={}", template_root.display());

    let mut css = String::from("/* Generated from templates/admin/default - do not edit. */\n");
    append_scss(&mut css, "tokens", &template_root.join("tokens.scss"));
    append_scss(&mut css, "chrome", &template_root.join("chrome.scss"));
    // `admin.scss` / `bootstrap.scss` need Angular Sass load paths (Node bootstrap).
    // Chrome already defines `.admin` layout used by Leptos pages.
    for panel in ["admin_shell", "orders_page", "products_page"] {
        append_scss(
            &mut css,
            panel,
            &rustashop_template_admin_default::component_file(panel, "scss"),
        );
    }

    std::fs::write(&css_out, css).unwrap_or_else(|err| {
        panic!("write {}: {err}", css_out.display());
    });
}

fn append_scss(css: &mut String, label: &str, scss_path: &Path) {
    let scss = std::fs::read_to_string(scss_path).unwrap_or_else(|err| {
        panic!("read {}: {err}", scss_path.display());
    });
    let result = rangular_css::compile_scss(&scss);
    assert!(result.ok(), "{label}.scss: {:?}", result.issues);
    let _ = write!(css, "\n/* --- {label} --- */\n");
    css.push_str(&result.css);
    css.push('\n');
}
