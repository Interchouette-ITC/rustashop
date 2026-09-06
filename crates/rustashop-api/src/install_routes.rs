//! HTTP surface for `/install` when `install/dist` exists on disk.

use std::path::Path;

use actix_files::Files;
use actix_web::web;
use serde::{Deserialize, Serialize};
use serenade_http::Response;

use crate::error::json_response;
use crate::install_env::{
    InstallEnvError, InstallWriteOptions, existing_prefix_needs_wipe, run_install_write,
};
use crate::install_fs::{
    INSTALL_DIR_NAME, INSTALL_OFF_DIR_NAME, install_artefacts_present, install_dir, shop_root,
};

#[derive(Debug, Serialize)]
struct StatusResponse {
    available: bool,
    wipe_required: bool,
    rename_after_success: String,
}

#[derive(Debug, Deserialize)]
struct CompleteBody {
    /// Optional explicit admin folder segment.
    admin_folder: Option<String>,
    /// Required when overwriting an existing non-default prefix.
    #[serde(default)]
    wipe_confirmed: bool,
}

#[derive(Debug, Serialize)]
struct CompleteResponse {
    admin_prefix: String,
    admin_token: String,
    env_path: String,
    next_step: String,
}

/// `GET /install/api/status` as a Serenade [`Response`].
///
/// Returns 404 when `root` is missing or install artefacts are not on disk.
#[must_use]
pub fn install_status_response(root: Option<&Path>) -> Response {
    let Some(root) = root else {
        return Response::new(404);
    };
    if !install_artefacts_present(root) {
        return Response::new(404);
    }
    json_response(
        200,
        &StatusResponse {
            available: true,
            wipe_required: existing_prefix_needs_wipe(root),
            rename_after_success: format!("mv {INSTALL_DIR_NAME} {INSTALL_OFF_DIR_NAME}"),
        },
    )
}

/// `POST /install/api/complete` as a Serenade [`Response`].
#[must_use]
pub fn install_complete_response(root: Option<&Path>, body: &[u8]) -> Response {
    let Some(root) = root else {
        return Response::new(404);
    };
    if !install_artefacts_present(root) {
        return Response::new(404);
    }
    let request = match serde_json::from_slice::<CompleteBody>(body) {
        Ok(request) => request,
        Err(error) => {
            return json_response(
                400,
                &serde_json::json!({
                    "error": "invalid_body",
                    "message": error.to_string(),
                }),
            );
        }
    };
    match run_install_write(&InstallWriteOptions {
        admin_folder: request.admin_folder,
        wipe_confirmed: request.wipe_confirmed,
    }) {
        Ok(result) => json_response(
            200,
            &CompleteResponse {
                admin_prefix: result.admin_prefix,
                admin_token: result.admin_token,
                env_path: result.env_path.display().to_string(),
                next_step: format!(
                    "Run `mv {INSTALL_DIR_NAME} {INSTALL_OFF_DIR_NAME}` so /install stops being served."
                ),
            },
        ),
        Err(InstallEnvError::WipeRequired) => json_response(
            409,
            &serde_json::json!({
                "error": "wipe_required",
                "message": "I understand this will wipe my shop files and database."
            }),
        ),
        Err(InstallEnvError::InvalidPrefix(message)) => json_response(
            400,
            &serde_json::json!({
                "error": "invalid_admin_folder",
                "message": message
            }),
        ),
        Err(InstallEnvError::Io(_)) => Response::new(500),
    }
}

/// Registers install static files when artefacts exist (JSON API is on the Serenade kernel).
pub fn configure_install(cfg: &mut web::ServiceConfig, root: &Path) {
    if !install_artefacts_present(root) {
        return;
    }
    let dist = install_dir(root).join("dist");
    cfg.service(Files::new("/install", dist).index_file("index.html"));
}

/// Convenience for [`shop_root`] at configure time.
pub fn configure_install_from_env(cfg: &mut web::ServiceConfig) {
    configure_install(cfg, &shop_root());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::install_fs::install_dist_index;
    use std::fs;

    #[test]
    fn status_ok_when_dist_present() {
        let _guard = crate::install_env::INSTALL_PROCESS_ENV_LOCK
            .lock()
            .expect("lock");
        unsafe {
            std::env::remove_var(crate::install_env::ENV_FILE_ENV);
        }
        let dir = tempfile_dir("status-ok");
        let index = install_dist_index(&dir);
        fs::create_dir_all(index.parent().expect("parent")).expect("mkdir");
        fs::write(&index, "<!doctype html>").expect("write");

        let resp = install_status_response(Some(&dir));
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = serde_json::from_slice(resp.body()).expect("json");
        assert_eq!(body["available"], true);
        assert_eq!(body["wipe_required"], false);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn status_absent_without_dist() {
        let dir = tempfile_dir("status-absent");
        assert_eq!(install_status_response(Some(&dir)).status(), 404);
        assert_eq!(install_status_response(None).status(), 404);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn complete_returns_not_found_without_root_or_dist() {
        let dir = tempfile_dir("complete-absent");
        assert_eq!(
            install_complete_response(Some(&dir), br#"{"wipe_confirmed":true}"#).status(),
            404
        );
        assert_eq!(
            install_complete_response(None, br#"{"wipe_confirmed":true}"#).status(),
            404
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn complete_rejects_invalid_json_body() {
        let dir = tempfile_dir("complete-bad-json");
        let index = install_dist_index(&dir);
        fs::create_dir_all(index.parent().expect("parent")).expect("mkdir");
        fs::write(&index, "<!doctype html>").expect("write");
        let resp = install_complete_response(Some(&dir), b"not-json");
        assert_eq!(resp.status(), 400);
        let body: serde_json::Value = serde_json::from_slice(resp.body()).expect("json");
        assert_eq!(body["error"], "invalid_body");
        let _ = fs::remove_dir_all(&dir);
    }

    #[actix_web::test]
    async fn configure_install_skips_without_dist() {
        use actix_web::{App, test};

        let dir = tempfile_dir("cfg-skip");
        let app =
            test::init_service(App::new().configure(|cfg| configure_install(cfg, &dir))).await;
        let req = test::TestRequest::get().uri("/install/").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), 404);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn complete_success_wipe_and_invalid_prefix() {
        let _guard = crate::install_env::INSTALL_PROCESS_ENV_LOCK
            .lock()
            .expect("lock");
        let dir = tempfile_dir("complete-ok");
        let index = install_dist_index(&dir);
        fs::create_dir_all(index.parent().expect("parent")).expect("mkdir");
        fs::write(&index, "<!doctype html>").expect("write");
        let env_path = dir.join(".env");
        fs::write(&env_path, "RUSTASHOP_ADMIN_API_PREFIX=alreadypfx1\n").expect("env");
        unsafe {
            std::env::set_var(crate::install_fs::ROOT_ENV, &dir);
            std::env::set_var(crate::install_env::ENV_FILE_ENV, &env_path);
        }

        let conflict = install_complete_response(
            Some(&dir),
            br#"{"admin_folder":"newfolderok1","wipe_confirmed":false}"#,
        );
        assert_eq!(conflict.status(), 409);
        let conflict_body: serde_json::Value =
            serde_json::from_slice(conflict.body()).expect("json");
        assert_eq!(conflict_body["error"], "wipe_required");

        let bad = install_complete_response(
            Some(&dir),
            br#"{"admin_folder":"carts","wipe_confirmed":true}"#,
        );
        assert_eq!(bad.status(), 400);
        let bad_body: serde_json::Value = serde_json::from_slice(bad.body()).expect("json");
        assert_eq!(bad_body["error"], "invalid_admin_folder");

        let ok = install_complete_response(
            Some(&dir),
            br#"{"admin_folder":"newfolderok1","wipe_confirmed":true}"#,
        );
        assert_eq!(ok.status(), 200);
        let body: serde_json::Value = serde_json::from_slice(ok.body()).expect("json");
        assert_eq!(body["admin_prefix"], "newfolderok1");
        assert!(body["admin_token"].as_str().unwrap().len() >= 16);
        assert!(
            body["next_step"]
                .as_str()
                .unwrap()
                .contains(INSTALL_OFF_DIR_NAME)
        );

        let status = install_status_response(Some(&dir));
        let status_body: serde_json::Value = serde_json::from_slice(status.body()).expect("json");
        assert_eq!(status_body["wipe_required"], true);

        unsafe {
            std::env::remove_var(crate::install_fs::ROOT_ENV);
            std::env::remove_var(crate::install_env::ENV_FILE_ENV);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn complete_maps_io_error_to_500() {
        let _guard = crate::install_env::INSTALL_PROCESS_ENV_LOCK
            .lock()
            .expect("lock");
        let dir = tempfile_dir("complete-io");
        let index = install_dist_index(&dir);
        fs::create_dir_all(index.parent().expect("parent")).expect("mkdir");
        fs::write(&index, "<!doctype html>").expect("write");
        let blocker = dir.join("env-blocker");
        fs::write(&blocker, "x").expect("blocker file");
        let env_path = blocker.join(".env");
        unsafe {
            std::env::set_var(crate::install_fs::ROOT_ENV, &dir);
            std::env::set_var(crate::install_env::ENV_FILE_ENV, &env_path);
        }
        let resp = install_complete_response(
            Some(&dir),
            br#"{"admin_folder":"newfolderok1","wipe_confirmed":true}"#,
        );
        assert_eq!(resp.status(), 500);
        unsafe {
            std::env::remove_var(crate::install_fs::ROOT_ENV);
            std::env::remove_var(crate::install_env::ENV_FILE_ENV);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    // Process env must stay stable across Actix test awaits; hold the lock for the whole test.
    #[actix_web::test]
    #[allow(clippy::await_holding_lock)]
    async fn configure_install_from_env_registers_static_when_dist_present() {
        use actix_web::{App, test};

        let _guard = crate::install_env::INSTALL_PROCESS_ENV_LOCK
            .lock()
            .expect("lock");
        let dir = tempfile_dir("from-env");
        let index = install_dist_index(&dir);
        fs::create_dir_all(index.parent().expect("parent")).expect("mkdir");
        fs::write(&index, "<!doctype html><title>i</title>").expect("write");
        unsafe {
            std::env::set_var(crate::install_fs::ROOT_ENV, &dir);
            std::env::remove_var(crate::install_env::ENV_FILE_ENV);
        }
        let app = test::init_service(App::new().configure(configure_install_from_env)).await;
        let req = test::TestRequest::get().uri("/install/").to_request();
        let resp = test::call_service(&app, req).await;
        assert!(resp.status().is_success());
        unsafe {
            std::env::remove_var(crate::install_fs::ROOT_ENV);
        }
        let _ = fs::remove_dir_all(&dir);
    }

    fn tempfile_dir(label: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "rustashop-install-routes-{label}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("tmpdir");
        dir
    }
}
