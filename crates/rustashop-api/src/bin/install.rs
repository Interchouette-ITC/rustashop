//! CLI: write opaque admin segment + token into `.env` (same as `/install` API).

use rustashop_api::{
    INSTALL_DIR_NAME, INSTALL_OFF_DIR_NAME, InstallWriteOptions, run_install_write,
};

fn main() {
    let mut wipe = false;
    let mut admin_folder: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--i-understand-wipe-files-and-db" => wipe = true,
            "--admin-folder" => {
                admin_folder = args.next();
            }
            "-h" | "--help" => {
                print_help();
                return;
            }
            other => {
                eprintln!("unknown argument: {other}");
                print_help();
                std::process::exit(2);
            }
        }
    }

    match run_install_write(&InstallWriteOptions {
        admin_folder,
        wipe_confirmed: wipe,
    }) {
        Ok(result) => {
            println!("Wrote {}", result.env_path.display());
            println!("RUSTASHOP_ADMIN_API_PREFIX={}", result.admin_prefix);
            println!("RUSTASHOP_ADMIN_API_TOKEN={}", result.admin_token);
            println!("Next: mv {INSTALL_DIR_NAME} {INSTALL_OFF_DIR_NAME}");
        }
        Err(error) => {
            eprintln!("{error}");
            if error.to_string().contains("wipe") {
                eprintln!("Re-run with --i-understand-wipe-files-and-db");
            }
            std::process::exit(1);
        }
    }
}

fn print_help() {
    eprintln!(
        "rustashop-install [--admin-folder SEG] [--i-understand-wipe-files-and-db]\n\
         Writes RUSTASHOP_ADMIN_API_PREFIX and RUSTASHOP_ADMIN_API_TOKEN into .env\n\
         After success: mv {INSTALL_DIR_NAME} {INSTALL_OFF_DIR_NAME}"
    );
}
