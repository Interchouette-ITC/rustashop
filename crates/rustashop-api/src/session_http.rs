//! Browser session bridge + CSRF HTML dogfood (Serenade session / security).

use std::sync::Arc;

use serenade_http::{Request, Response};
use serenade_security::{
    CSRF_FIELD_NAME, CsrfToken, CsrfTokenManager, HmacCsrfTokenManager, InMemoryUser,
    TokenInterface, UsernamePasswordToken, login, logout, request_token, token_from_session,
};
use serenade_session::request_session_mut;

use crate::admin_auth::{ADMIN_TOKEN_ENV, AdminAuthConfig};
use crate::install_fs::install_artefacts_present;

/// Env for the HMAC CSRF secret (`HmacCsrfTokenManager`).
pub const CSRF_SECRET_ENV: &str = "RUSTASHOP_CSRF_SECRET";

/// Intention id for the install HTML form CSRF token.
pub const INSTALL_FORM_CSRF_ID: &str = "install.form";

/// Intention id for session login / logout forms.
pub const SESSION_FORM_CSRF_ID: &str = "session.form";

/// Builds the process CSRF manager (stable secret from env or local default).
#[must_use]
pub fn csrf_manager_from_env() -> Arc<HmacCsrfTokenManager> {
    Arc::new(HmacCsrfTokenManager::new(csrf_secret_bytes()))
}

fn csrf_secret_bytes() -> Vec<u8> {
    std::env::var(CSRF_SECRET_ENV)
        .or_else(|_| std::env::var(ADMIN_TOKEN_ENV))
        .unwrap_or_else(|_| "rustashop-dev-csrf-secret!!!!!!!!!!".to_owned())
        .into_bytes()
}

/// Sync browser routes that need the request session (login / logout / install form).
#[must_use]
pub fn try_browser_security_route(
    route_name: &str,
    request: &mut Request,
    admin_auth: &AdminAuthConfig,
    csrf: &dyn CsrfTokenManager,
    install_root: Option<&std::path::Path>,
) -> Option<Response> {
    match route_name {
        "session_status" => Some(session_status_response(request)),
        "session_login_get" => Some(session_login_form_response(csrf)),
        "session_login_post" => Some(session_login_post_response(request, admin_auth, csrf)),
        "session_logout_get" => Some(session_logout_form_response(csrf)),
        "session_logout_post" => Some(session_logout_post_response(request, csrf)),
        "install_form_get" => Some(install_form_get_response(install_root, csrf)),
        "install_form_post" => Some(install_form_post_response(install_root, request, csrf)),
        _ => None,
    }
}

fn session_status_response(request: &Request) -> Response {
    let token = request_token(request);
    let session_token = serenade_session::request_session(request).and_then(token_from_session);
    let body = format!(
        "firewall_authenticated={}\nsession_authenticated={}\n",
        token.is_some_and(TokenInterface::is_authenticated),
        session_token
            .as_ref()
            .is_some_and(TokenInterface::is_authenticated),
    );
    Response::text(200, body)
}

fn session_login_form_response(csrf: &dyn CsrfTokenManager) -> Response {
    csrf.get_token(SESSION_FORM_CSRF_ID).map_or_else(
        |_| Response::text(500, "csrf generation failed"),
        |token| {
            html_form_response(
                "Session login",
                "/session/login",
                &token,
                r#"<label>Admin bearer <input name="token" type="password" autocomplete="off"></label>"#,
            )
        },
    )
}

fn session_logout_form_response(csrf: &dyn CsrfTokenManager) -> Response {
    csrf.get_token(SESSION_FORM_CSRF_ID).map_or_else(
        |_| Response::text(500, "csrf generation failed"),
        |token| html_form_response("Session logout", "/session/logout", &token, ""),
    )
}

fn session_login_post_response(
    request: &mut Request,
    admin_auth: &AdminAuthConfig,
    csrf: &dyn CsrfTokenManager,
) -> Response {
    let fields = parse_form_body(request.body());
    if !csrf_ok(
        csrf,
        SESSION_FORM_CSRF_ID,
        fields.get(CSRF_FIELD_NAME).map(String::as_str),
    ) {
        return Response::text(403, "invalid csrf");
    }
    let presented = fields.get("token").map(String::as_str);
    if admin_auth.authorize_bearer(presented).is_err() {
        return Response::text(401, "unauthorized");
    }
    let Some(session) = request_session_mut(request) else {
        return Response::text(500, "session missing");
    };
    let security = UsernamePasswordToken::authenticated(
        InMemoryUser::new("admin", vec!["ROLE_ADMIN".to_owned()]),
        presented.unwrap_or_default(),
    );
    login(session, &security);
    Response::text(200, "logged in")
}

fn session_logout_post_response(request: &mut Request, csrf: &dyn CsrfTokenManager) -> Response {
    let fields = parse_form_body(request.body());
    if !csrf_ok(
        csrf,
        SESSION_FORM_CSRF_ID,
        fields.get(CSRF_FIELD_NAME).map(String::as_str),
    ) {
        return Response::text(403, "invalid csrf");
    }
    let Some(session) = request_session_mut(request) else {
        return Response::text(500, "session missing");
    };
    logout(session);
    Response::text(200, "logged out")
}

fn install_form_get_response(
    install_root: Option<&std::path::Path>,
    csrf: &dyn CsrfTokenManager,
) -> Response {
    let Some(root) = install_root else {
        return Response::new(404);
    };
    if !install_artefacts_present(root) {
        return Response::new(404);
    }
    csrf.get_token(INSTALL_FORM_CSRF_ID).map_or_else(
        |_| Response::text(500, "csrf generation failed"),
        |token| {
            html_form_response(
                "Install confirm",
                "/install/form",
                &token,
                r#"<label><input name="wipe_confirmed" type="checkbox" value="1"> Wipe confirmed</label>"#,
            )
        },
    )
}

fn install_form_post_response(
    install_root: Option<&std::path::Path>,
    request: &Request,
    csrf: &dyn CsrfTokenManager,
) -> Response {
    let Some(root) = install_root else {
        return Response::new(404);
    };
    if !install_artefacts_present(root) {
        return Response::new(404);
    }
    let fields = parse_form_body(request.body());
    if !csrf_ok(
        csrf,
        INSTALL_FORM_CSRF_ID,
        fields.get(CSRF_FIELD_NAME).map(String::as_str),
    ) {
        return Response::text(403, "invalid csrf");
    }
    // Dogfood only: CSRF gate for the HTML form. JSON `/install/api/complete` stays separate.
    Response::text(200, "csrf ok")
}

fn html_form_response(
    title: &str,
    action: &str,
    token: &CsrfToken,
    extra_fields: &str,
) -> Response {
    let html = format!(
        "<!doctype html><html><head><title>{title}</title></head><body>\
         <h1>{title}</h1>\
         <form method=\"post\" action=\"{action}\">\
         <input type=\"hidden\" name=\"{csrf_name}\" value=\"{csrf_value}\">\
         {extra_fields}\
         <button type=\"submit\">Submit</button>\
         </form></body></html>",
        csrf_name = CSRF_FIELD_NAME,
        csrf_value = token.value(),
    );
    let mut response = Response::text(200, html);
    response
        .headers_mut()
        .insert("content-type", "text/html; charset=utf-8");
    response
}

fn csrf_ok(csrf: &dyn CsrfTokenManager, intention: &str, submitted: Option<&str>) -> bool {
    let Some(value) = submitted.filter(|v| !v.is_empty()) else {
        return false;
    };
    csrf.is_token_valid(&CsrfToken::new(intention, value))
}

fn parse_form_body(body: &[u8]) -> std::collections::HashMap<String, String> {
    let raw = String::from_utf8_lossy(body);
    let mut map = std::collections::HashMap::new();
    for pair in raw.split('&') {
        let mut parts = pair.splitn(2, '=');
        let key = parts.next().unwrap_or_default();
        let value = parts.next().unwrap_or_default();
        if key.is_empty() {
            continue;
        }
        map.insert(form_decode(key), form_decode(value));
    }
    map
}

fn form_decode(input: &str) -> String {
    let plus = input.replace('+', " ");
    percent_decode(&plus)
}

fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let (Some(hi), Some(lo)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                    out.push((hi << 4) | lo);
                    i += 3;
                    continue;
                }
                out.push(bytes[i]);
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

const fn from_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Whether `Content-Type` is a browser form post.
#[must_use]
#[cfg(test)]
pub fn is_form_urlencoded(headers: &serenade_http::Headers) -> bool {
    headers
        .get("content-type")
        .is_some_and(|value| value.starts_with("application/x-www-form-urlencoded"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serenade_http::{Headers, Method};
    use serenade_session::{CookieSession, MemorySessionStore, SESSION_ATTRIBUTE, SessionStore};

    fn test_csrf() -> HmacCsrfTokenManager {
        HmacCsrfTokenManager::new(b"test-secret-key-32bytes-minimum!!")
    }

    fn request_with_session(method: Method, path: &str, body: Vec<u8>) -> Request {
        let store = Arc::new(MemorySessionStore::new()) as Arc<dyn SessionStore>;
        let cookies = CookieSession::new(store);
        let mut request = Request::new(method, path).with_body(body);
        let session = cookies
            .open(request.headers().get("cookie"))
            .expect("open session");
        request.attributes_mut().insert(SESSION_ATTRIBUTE, session);
        request
    }

    #[test]
    fn parse_form_body_decodes_fields() {
        let map = parse_form_body(b"token=abc&_token=x%2Fy&wipe_confirmed=1");
        assert_eq!(map.get("token").map(String::as_str), Some("abc"));
        assert_eq!(map.get("_token").map(String::as_str), Some("x/y"));
        assert_eq!(map.get("wipe_confirmed").map(String::as_str), Some("1"));
    }

    #[test]
    fn csrf_rejects_missing_and_accepts_issued() {
        let csrf = test_csrf();
        assert!(!csrf_ok(&csrf, INSTALL_FORM_CSRF_ID, None));
        assert!(!csrf_ok(&csrf, INSTALL_FORM_CSRF_ID, Some("bad")));
        let token = csrf.get_token(INSTALL_FORM_CSRF_ID).expect("token");
        assert!(csrf_ok(&csrf, INSTALL_FORM_CSRF_ID, Some(token.value())));
    }

    #[test]
    fn install_form_get_404_without_artefacts() {
        let csrf = test_csrf();
        assert_eq!(install_form_get_response(None, &csrf).status(), 404);
        let dir = std::env::temp_dir().join(format!("rs-session-install-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("mkdir");
        assert_eq!(install_form_get_response(Some(&dir), &csrf).status(), 404);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_login_logout_roundtrip_with_csrf() {
        let csrf = test_csrf();
        let auth = AdminAuthConfig::from_token("secret");
        let token = csrf.get_token(SESSION_FORM_CSRF_ID).expect("csrf");
        let body = format!("token=secret&{CSRF_FIELD_NAME}={}", token.value());
        let mut request = request_with_session(Method::Post, "/session/login", body.into_bytes());
        let response = session_login_post_response(&mut request, &auth, &csrf);
        assert_eq!(response.status(), 200);
        let session = request
            .attributes()
            .get::<serenade_session::Session>(SESSION_ATTRIBUTE)
            .expect("session");
        assert!(token_from_session(session).is_some_and(|t| t.is_authenticated()));

        let logout_token = csrf.get_token(SESSION_FORM_CSRF_ID).expect("csrf");
        let logout_body = format!("{CSRF_FIELD_NAME}={}", logout_token.value());
        // Keep the same session bag; replace body by rebuilding attributes.
        let session = request
            .attributes_mut()
            .remove::<serenade_session::Session>(SESSION_ATTRIBUTE)
            .expect("session");
        let mut logout_request =
            Request::new(Method::Post, "/session/logout").with_body(logout_body.into_bytes());
        logout_request
            .attributes_mut()
            .insert(SESSION_ATTRIBUTE, session);
        let response = session_logout_post_response(&mut logout_request, &csrf);
        assert_eq!(response.status(), 200);
        let session = logout_request
            .attributes()
            .get::<serenade_session::Session>(SESSION_ATTRIBUTE)
            .expect("session");
        assert!(token_from_session(session).is_none());
    }

    #[test]
    fn session_login_rejects_bad_csrf() {
        let csrf = test_csrf();
        let auth = AdminAuthConfig::from_token("secret");
        let mut request = request_with_session(
            Method::Post,
            "/session/login",
            b"token=secret&_token=nope".to_vec(),
        );
        assert_eq!(
            session_login_post_response(&mut request, &auth, &csrf).status(),
            403
        );
    }

    #[test]
    fn session_login_form_is_html() {
        let csrf = test_csrf();
        let response = session_login_form_response(&csrf);
        assert_eq!(response.status(), 200);
        assert!(
            response
                .headers()
                .get("content-type")
                .unwrap()
                .contains("text/html")
        );
        let body = String::from_utf8_lossy(response.body());
        assert!(body.contains(CSRF_FIELD_NAME));
        assert!(body.contains("/session/login"));
    }

    #[test]
    fn is_form_urlencoded_detects_header() {
        let mut headers = Headers::new();
        headers.insert("content-type", "application/x-www-form-urlencoded");
        assert!(is_form_urlencoded(&headers));
        assert!(!is_form_urlencoded(&Headers::new()));
    }

    struct FailCsrf;

    impl CsrfTokenManager for FailCsrf {
        fn get_token(
            &self,
            _token_id: &str,
        ) -> Result<CsrfToken, serenade_security::SecurityError> {
            Err(serenade_security::SecurityError::CsrfGeneration)
        }

        fn is_token_valid(&self, _token: &CsrfToken) -> bool {
            false
        }
    }

    #[test]
    fn csrf_manager_from_env_builds() {
        let _guard = crate::install_env::INSTALL_PROCESS_ENV_LOCK
            .lock()
            .expect("lock");
        unsafe {
            std::env::remove_var(CSRF_SECRET_ENV);
            std::env::remove_var(ADMIN_TOKEN_ENV);
        }
        let mgr = csrf_manager_from_env();
        assert!(mgr.get_token(SESSION_FORM_CSRF_ID).is_ok());
        unsafe {
            std::env::set_var(ADMIN_TOKEN_ENV, "admin-token-as-csrf-fallback!!!!!!");
        }
        let mgr = csrf_manager_from_env();
        assert!(mgr.get_token(SESSION_FORM_CSRF_ID).is_ok());
        unsafe {
            std::env::set_var(CSRF_SECRET_ENV, "csrf-secret-from-env-32bytes!!!!");
        }
        let mgr = csrf_manager_from_env();
        assert!(mgr.get_token(SESSION_FORM_CSRF_ID).is_ok());
        unsafe {
            std::env::remove_var(CSRF_SECRET_ENV);
            std::env::remove_var(ADMIN_TOKEN_ENV);
        }
    }

    #[test]
    fn forms_return_500_when_csrf_generation_fails() {
        let fail = FailCsrf;
        assert!(!fail.is_token_valid(&CsrfToken::new("x", "y")));
        assert_eq!(session_login_form_response(&fail).status(), 500);
        assert_eq!(session_logout_form_response(&fail).status(), 500);
        let dir = std::env::temp_dir().join(format!("rs-session-fail-csrf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let index = crate::install_fs::install_dist_index(&dir);
        std::fs::create_dir_all(index.parent().expect("parent")).expect("mkdir");
        std::fs::write(&index, "ok").expect("write");
        assert_eq!(install_form_get_response(Some(&dir), &fail).status(), 500);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn try_browser_security_route_dispatches_and_unknown() {
        let csrf = test_csrf();
        let auth = AdminAuthConfig::from_token("secret");
        let mut request = Request::new(Method::Get, "/session");
        assert_eq!(
            try_browser_security_route("session_status", &mut request, &auth, &csrf, None)
                .expect("status")
                .status(),
            200
        );
        assert_eq!(
            try_browser_security_route("session_login_get", &mut request, &auth, &csrf, None)
                .expect("login get")
                .status(),
            200
        );
        assert_eq!(
            try_browser_security_route("session_logout_get", &mut request, &auth, &csrf, None)
                .expect("logout get")
                .status(),
            200
        );
        let token = csrf.get_token(SESSION_FORM_CSRF_ID).expect("csrf");
        let body = format!("token=secret&{CSRF_FIELD_NAME}={}", token.value());
        let mut login = request_with_session(Method::Post, "/session/login", body.into_bytes());
        assert_eq!(
            try_browser_security_route("session_login_post", &mut login, &auth, &csrf, None)
                .expect("login post")
                .status(),
            200
        );
        let logout_token = csrf.get_token(SESSION_FORM_CSRF_ID).expect("csrf");
        let logout_body = format!("{CSRF_FIELD_NAME}={}", logout_token.value());
        let mut logout =
            request_with_session(Method::Post, "/session/logout", logout_body.into_bytes());
        assert_eq!(
            try_browser_security_route("session_logout_post", &mut logout, &auth, &csrf, None)
                .expect("logout post")
                .status(),
            200
        );
        assert!(try_browser_security_route("nope", &mut request, &auth, &csrf, None).is_none());
    }

    #[test]
    fn session_login_unauthorized_and_missing_session() {
        let csrf = test_csrf();
        let auth = AdminAuthConfig::from_token("secret");
        let token = csrf.get_token(SESSION_FORM_CSRF_ID).expect("csrf");
        let body = format!("token=wrong&{CSRF_FIELD_NAME}={}", token.value());
        let mut request = request_with_session(Method::Post, "/session/login", body.into_bytes());
        assert_eq!(
            session_login_post_response(&mut request, &auth, &csrf).status(),
            401
        );

        let token = csrf.get_token(SESSION_FORM_CSRF_ID).expect("csrf");
        let body = format!("token=secret&{CSRF_FIELD_NAME}={}", token.value());
        let mut bare = Request::new(Method::Post, "/session/login").with_body(body.into_bytes());
        assert_eq!(
            session_login_post_response(&mut bare, &auth, &csrf).status(),
            500
        );
    }

    #[test]
    fn session_logout_rejects_bad_csrf_and_missing_session() {
        let csrf = test_csrf();
        let mut request =
            request_with_session(Method::Post, "/session/logout", b"_token=bad".to_vec());
        assert_eq!(
            session_logout_post_response(&mut request, &csrf).status(),
            403
        );

        let token = csrf.get_token(SESSION_FORM_CSRF_ID).expect("csrf");
        let body = format!("{CSRF_FIELD_NAME}={}", token.value());
        let mut bare = Request::new(Method::Post, "/session/logout").with_body(body.into_bytes());
        assert_eq!(session_logout_post_response(&mut bare, &csrf).status(), 500);
    }

    #[test]
    fn install_form_get_and_post_with_artefacts() {
        let csrf = test_csrf();
        let dir = std::env::temp_dir().join(format!(
            "rs-session-install-ok-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let index = crate::install_fs::install_dist_index(&dir);
        std::fs::create_dir_all(index.parent().expect("parent")).expect("mkdir");
        std::fs::write(&index, "<!doctype html>").expect("write");

        let get = install_form_get_response(Some(&dir), &csrf);
        assert_eq!(get.status(), 200);
        assert!(String::from_utf8_lossy(get.body()).contains("/install/form"));

        let token = csrf.get_token(INSTALL_FORM_CSRF_ID).expect("csrf");
        let body = format!("{CSRF_FIELD_NAME}={}", token.value());
        let request =
            Request::new(Method::Post, "/install/form").with_body(body.clone().into_bytes());
        assert_eq!(
            install_form_post_response(Some(&dir), &request, &csrf).status(),
            200
        );
        assert_eq!(
            install_form_post_response(None, &request, &csrf).status(),
            404
        );
        let bad = Request::new(Method::Post, "/install/form").with_body(b"_token=nope".to_vec());
        assert_eq!(
            install_form_post_response(Some(&dir), &bad, &csrf).status(),
            403
        );
        let empty = std::env::temp_dir().join(format!("rs-session-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&empty);
        std::fs::create_dir_all(&empty).expect("mkdir");
        assert_eq!(
            install_form_post_response(Some(&empty), &request, &csrf).status(),
            404
        );

        assert_eq!(
            try_browser_security_route(
                "install_form_get",
                &mut Request::new(Method::Get, "/install/form"),
                &AdminAuthConfig::from_token(""),
                &csrf,
                Some(&dir),
            )
            .expect("dispatch")
            .status(),
            200
        );
        assert_eq!(
            try_browser_security_route(
                "install_form_post",
                &mut Request::new(Method::Post, "/install/form").with_body(body.into_bytes()),
                &AdminAuthConfig::from_token(""),
                &csrf,
                Some(&dir),
            )
            .expect("dispatch post")
            .status(),
            200
        );

        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&empty);
    }

    #[test]
    fn parse_form_body_skips_empty_keys_and_decodes_plus() {
        let map = parse_form_body(b"&=skip&name=a+b&bare&x=%ZZ&y=a%&z=%2f");
        assert!(!map.contains_key(""));
        assert_eq!(map.get("name").map(String::as_str), Some("a b"));
        assert!(map.contains_key("bare"));
        assert_eq!(map.get("x").map(String::as_str), Some("%ZZ"));
        assert_eq!(map.get("y").map(String::as_str), Some("a%"));
        assert_eq!(map.get("z").map(String::as_str), Some("/"));
    }

    #[tokio::test]
    async fn commerce_kernel_serves_session_login_html() {
        use crate::http_front::{CommerceFrontConfig, commerce_http_kernel};
        use serenade_http::AsyncHttpKernel;

        let kernel: AsyncHttpKernel = commerce_http_kernel(CommerceFrontConfig::test_default());
        let response = kernel
            .handle(Request::new(Method::Get, "/session/login"))
            .await;
        assert_eq!(response.status(), 200);
        let body = String::from_utf8_lossy(response.body());
        assert!(body.contains("text/html") || response.headers().get("content-type").is_some());
        assert!(body.contains(CSRF_FIELD_NAME));
        assert!(body.contains("/session/login"));

        let status = kernel.handle(Request::new(Method::Get, "/session")).await;
        assert_eq!(status.status(), 200);
        assert!(String::from_utf8_lossy(status.body()).contains("firewall_authenticated="));

        let logout = kernel
            .handle(Request::new(Method::Get, "/session/logout"))
            .await;
        assert_eq!(logout.status(), 200);
        assert!(String::from_utf8_lossy(logout.body()).contains("/session/logout"));

        let health = kernel.handle(Request::new(Method::Get, "/healthz")).await;
        assert_eq!(health.status(), 200);
    }
}
