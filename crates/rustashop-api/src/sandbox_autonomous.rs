//! Autonomous sandbox jobs: guest proposal + host-mediated commit.

use rustashop_persist::CatalogRepository;
use rustashop_sandbox::{
    CART_UPDATE_QUANTITY_HOOK, DomainEventDraft, LegacyHookInput, accept_validated_domain_event,
    invoke_php_migration_hook, php_migration_hook_source,
};
use serenade_http::Response;
use utoipa::ToSchema;

use crate::admin_auth::AdminAuthConfig;
use crate::carts::CartResponse;
use crate::error::{ApiError, ErrorBody, api_error_json_response, json_response};
use crate::realtime::{CartHub, CartRealtimeEvent};
use crate::sandbox_jobs::{
    CreateSandboxJobRequest, JOB_TYPE_CART_QUANTITY, SandboxJobRegistry, SandboxJobResponse,
    SandboxJobStatus, SandboxProposalResponse, source_hash,
};
use crate::sandbox_realtime::{SandboxJobEvent, SandboxJobHub, SandboxProposalEventBody};

/// Creates a `cart_quantity` job, spawns the PHP migration guest, returns 202.
pub fn create_cart_quantity_job(
    registry: &SandboxJobRegistry,
    hub: &SandboxJobHub,
    request: &CreateSandboxJobRequest,
) -> Response {
    let (cart_id, variant_id, quantity, operator) = match (
        request.cart_id.as_deref(),
        request.variant_id.as_deref(),
        request.quantity,
        request.operator.as_deref(),
    ) {
        (Some(cart_id), Some(variant_id), Some(quantity), Some(operator))
            if !cart_id.trim().is_empty()
                && !variant_id.trim().is_empty()
                && matches!(operator, "up" | "down" | "set") =>
        {
            (
                cart_id.to_owned(),
                variant_id.to_owned(),
                quantity,
                operator.to_owned(),
            )
        }
        _ => {
            return api_error_json_response(&ApiError::Unprocessable(
                "cart_quantity requires cart_id, variant_id, quantity, and operator (up|down|set)"
                    .into(),
            ));
        }
    };

    let source = php_migration_hook_source();
    let hash = source_hash(&source);
    let job = registry.start_job(JOB_TYPE_CART_QUANTITY, &hash, "admin-bearer");
    let job_id = job.id.clone();
    let input = LegacyHookInput {
        hook: CART_UPDATE_QUANTITY_HOOK.into(),
        cart_id,
        id_product: variant_id,
        quantity,
        operator,
    };
    let registry_runner = registry.clone();
    let hub_runner = hub.clone();

    tokio::spawn(async move {
        run_cart_quantity_job(&registry_runner, &hub_runner, &job_id, &input, &source).await;
    });

    json_response(202, &job)
}

async fn run_cart_quantity_job(
    registry: &SandboxJobRegistry,
    hub: &SandboxJobHub,
    job_id: &str,
    input: &LegacyHookInput,
    source: &str,
) {
    hub.publish(&SandboxJobEvent::log(
        job_id,
        "starting Wasmer PHP cart_quantity guest",
    ));
    match invoke_php_migration_hook(input, source).await {
        Ok(raw) => match accept_validated_domain_event(raw) {
            Ok(draft) => {
                let proposal = proposal_from_draft(&draft);
                hub.publish(&SandboxJobEvent::log(
                    job_id,
                    format!(
                        "validated proposal {} on cart {}",
                        proposal.event_type, proposal.cart_id
                    ),
                ));
                registry.set_awaiting_commit(job_id, proposal.clone());
                hub.publish(&SandboxJobEvent::proposal(
                    job_id,
                    SandboxProposalEventBody {
                        event_type: proposal.event_type,
                        cart_id: proposal.cart_id,
                        product_id: proposal.product_id,
                        quantity: proposal.quantity,
                        operator: proposal.operator,
                    },
                ));
            }
            Err(error) => {
                let message = format!("validation failed: {error:#}");
                hub.publish(&SandboxJobEvent::log(job_id, &message));
                registry.finish_job(job_id, SandboxJobStatus::Failed, None, Some(message));
                hub.publish(&SandboxJobEvent::finished(job_id, "error"));
            }
        },
        Err(error) => {
            let message = format!("guest failed: {error:#}");
            hub.publish(&SandboxJobEvent::log(job_id, &message));
            registry.finish_job(job_id, SandboxJobStatus::Failed, None, Some(message));
            hub.publish(&SandboxJobEvent::finished(job_id, "error"));
        }
    }
}

fn proposal_from_draft(draft: &DomainEventDraft) -> SandboxProposalResponse {
    SandboxProposalResponse {
        event_type: draft.event_type.clone(),
        cart_id: draft.cart_id.clone(),
        product_id: draft.product_id.clone(),
        quantity: draft.quantity,
        operator: draft.operator.clone(),
    }
}

/// Inputs for [`commit_sandbox_job_response`].
pub struct CommitSandboxJobContext<'a> {
    /// Admin bearer gate.
    pub auth: &'a AdminAuthConfig,
    /// Bearer token from the request.
    pub bearer: Option<&'a str>,
    /// Job registry.
    pub registry: &'a SandboxJobRegistry,
    /// Sandbox job push hub.
    pub hub: &'a SandboxJobHub,
    /// Optional cart push hub.
    pub cart_hub: Option<&'a CartHub>,
    /// Catalog for cart mutation.
    pub catalog: &'a CatalogRepository,
    /// Job id to commit.
    pub job_id: &'a str,
}

/// Host commit: apply a validated `cart_quantity` proposal to the live cart.
pub async fn commit_sandbox_job_response(ctx: CommitSandboxJobContext<'_>) -> Response {
    if let Err(error) = ctx.auth.authorize_bearer(ctx.bearer) {
        return api_error_json_response(&error);
    }
    let Some(job) = ctx.registry.get(ctx.job_id) else {
        return api_error_json_response(&ApiError::NotFound);
    };
    if job.status != SandboxJobStatus::AwaitingCommit {
        return api_error_json_response(&ApiError::Unprocessable(
            "job is not awaiting commit".into(),
        ));
    }
    let Some(proposal) = job.proposal.clone() else {
        return api_error_json_response(&ApiError::Unprocessable(
            "job has no proposal to commit".into(),
        ));
    };

    match apply_cart_quantity_proposal(ctx.catalog, ctx.cart_hub, &proposal).await {
        Ok(cart) => {
            ctx.registry
                .finalize_proposal(ctx.job_id, SandboxJobStatus::Committed);
            ctx.hub.publish(&SandboxJobEvent::log(
                ctx.job_id,
                format!("host committed quantity on cart {}", cart.id),
            ));
            ctx.hub
                .publish(&SandboxJobEvent::finished(ctx.job_id, "committed"));
            let updated = ctx.registry.get(ctx.job_id).unwrap_or(job);
            json_response(200, &CommitSandboxJobResponse { job: updated, cart })
        }
        Err(error) => api_error_json_response(&error),
    }
}

/// Discard a proposal without mutating commerce state.
pub fn discard_sandbox_job_response(
    auth: &AdminAuthConfig,
    bearer: Option<&str>,
    registry: &SandboxJobRegistry,
    hub: &SandboxJobHub,
    job_id: &str,
) -> Response {
    if let Err(error) = auth.authorize_bearer(bearer) {
        return api_error_json_response(&error);
    }
    let Some(job) = registry.get(job_id) else {
        return api_error_json_response(&ApiError::NotFound);
    };
    if job.status != SandboxJobStatus::AwaitingCommit {
        return api_error_json_response(&ApiError::Unprocessable(
            "job is not awaiting commit".into(),
        ));
    }
    registry.finalize_proposal(job_id, SandboxJobStatus::Discarded);
    hub.publish(&SandboxJobEvent::log(job_id, "operator discarded proposal"));
    hub.publish(&SandboxJobEvent::finished(job_id, "discarded"));
    registry.get(job_id).map_or_else(
        || json_response(200, &job),
        |updated| json_response(200, &updated),
    )
}

async fn apply_cart_quantity_proposal(
    catalog: &CatalogRepository,
    cart_hub: Option<&CartHub>,
    proposal: &SandboxProposalResponse,
) -> Result<CartResponse, ApiError> {
    let mut cart = match catalog.find_cart_by_id(&proposal.cart_id).await {
        Ok(Some(cart)) => cart,
        Ok(None) => return Err(ApiError::NotFound),
        Err(error) => return Err(ApiError::from_persist(&error)),
    };
    let line = cart
        .lines
        .iter()
        .find(|line| line.variant_id == proposal.product_id)
        .ok_or(ApiError::NotFound)?;
    let line_id = line.id.clone();
    let current = line.quantity;
    let next = resolve_quantity(current, proposal.quantity, &proposal.operator)?;
    cart.update_line_quantity(&line_id, next)
        .map_err(|error| ApiError::from_domain(&error))?;
    let saved = match catalog.save_cart(&cart).await {
        Ok(()) => match catalog.find_cart_by_id(&proposal.cart_id).await {
            Ok(Some(cart)) => cart,
            Ok(None) => return Err(ApiError::NotFound),
            Err(error) => return Err(ApiError::from_persist(&error)),
        },
        Err(error) => return Err(ApiError::from_persist(&error)),
    };
    let response = CartResponse::try_from_cart(saved)?;
    if let Some(hub) = cart_hub {
        hub.publish(&CartRealtimeEvent::updated(response.clone()));
    }
    Ok(response)
}

fn resolve_quantity(current: i32, operand: u32, operator: &str) -> Result<i32, ApiError> {
    let operand = i32::try_from(operand).map_err(|_| {
        ApiError::Unprocessable("quantity operand does not fit signed 32-bit".into())
    })?;
    let next = match operator {
        "set" => operand,
        "up" => current.saturating_add(operand),
        "down" => current.saturating_sub(operand),
        other => {
            return Err(ApiError::Unprocessable(format!(
                "unsupported operator `{other}`"
            )));
        }
    };
    if next < 1 {
        return Err(ApiError::Unprocessable(
            "resolved quantity must be at least 1".into(),
        ));
    }
    Ok(next)
}

/// Commit response: updated job + cart snapshot after host apply.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct CommitSandboxJobResponse {
    /// Job after commit.
    pub job: SandboxJobResponse,
    /// Cart after host-mediated mutation.
    pub cart: CartResponse,
}

/// `POST /v1/{admin_api_prefix}/sandbox/jobs/{id}/commit` `OpenAPI` stub.
#[utoipa::path(
    post,
    path = "/v1/{admin_api_prefix}/sandbox/jobs/{id}/commit",
    params(("id" = String, Path, description = "Job id")),
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Committed", body = CommitSandboxJobResponse),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody),
        (status = 404, description = "Unknown job or cart line", body = ErrorBody),
        (status = 422, description = "Not awaiting commit", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn commit_sandbox_job() {}

/// `POST /v1/{admin_api_prefix}/sandbox/jobs/{id}/discard` `OpenAPI` stub.
#[utoipa::path(
    post,
    path = "/v1/{admin_api_prefix}/sandbox/jobs/{id}/discard",
    params(("id" = String, Path, description = "Job id")),
    security(("admin_bearer" = [])),
    responses(
        (status = 200, description = "Discarded", body = SandboxJobResponse),
        (status = 401, description = "Missing or invalid bearer", body = ErrorBody),
        (status = 404, description = "Unknown job", body = ErrorBody),
        (status = 422, description = "Not awaiting commit", body = ErrorBody)
    )
)]
#[allow(clippy::missing_const_for_fn)]
pub fn discard_sandbox_job() {}

#[cfg(test)]
mod tests {
    use rustashop_sandbox::php_migration_hook_source;

    use super::*;
    use crate::sandbox_jobs::CreateSandboxJobRequest;

    fn sample_proposal() -> SandboxProposalResponse {
        SandboxProposalResponse {
            event_type: "cart.line_quantity_proposed".into(),
            cart_id: "cart-1".into(),
            product_id: "variant-1".into(),
            quantity: 3,
            operator: "set".into(),
        }
    }

    fn create_request(
        cart_id: Option<&str>,
        variant_id: Option<&str>,
        quantity: Option<u32>,
        operator: Option<&str>,
    ) -> CreateSandboxJobRequest {
        CreateSandboxJobRequest {
            job_type: JOB_TYPE_CART_QUANTITY.into(),
            currency: None,
            lines: None,
            cart_id: cart_id.map(str::to_owned),
            variant_id: variant_id.map(str::to_owned),
            quantity,
            operator: operator.map(str::to_owned),
        }
    }

    #[test]
    fn resolve_quantity_operators() {
        assert_eq!(resolve_quantity(2, 5, "set").unwrap(), 5);
        assert_eq!(resolve_quantity(2, 3, "up").unwrap(), 5);
        assert_eq!(resolve_quantity(5, 2, "down").unwrap(), 3);
        assert!(matches!(
            resolve_quantity(1, 1, "down"),
            Err(ApiError::Unprocessable(_))
        ));
        assert!(matches!(
            resolve_quantity(1, 1, "noop"),
            Err(ApiError::Unprocessable(_))
        ));
        assert!(matches!(
            resolve_quantity(1, u32::MAX, "set"),
            Err(ApiError::Unprocessable(_))
        ));
    }

    #[test]
    fn proposal_from_draft_maps_fields() {
        let draft = DomainEventDraft {
            event_type: "cart.line_quantity_proposed".into(),
            cart_id: "c1".into(),
            product_id: "p1".into(),
            quantity: 4,
            operator: "up".into(),
        };
        let proposal = proposal_from_draft(&draft);
        assert_eq!(proposal.cart_id, "c1");
        assert_eq!(proposal.product_id, "p1");
        assert_eq!(proposal.quantity, 4);
        assert_eq!(proposal.operator, "up");
    }

    #[test]
    fn create_cart_quantity_job_rejects_incomplete_request() {
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let response = create_cart_quantity_job(
            &registry,
            &hub,
            &create_request(Some("cart"), Some("v"), None, Some("set")),
        );
        assert_eq!(response.status(), 422);
    }

    #[test]
    fn create_cart_quantity_job_rejects_bad_operator() {
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let response = create_cart_quantity_job(
            &registry,
            &hub,
            &create_request(Some("cart"), Some("v"), Some(1), Some("noop")),
        );
        assert_eq!(response.status(), 422);
    }

    #[tokio::test]
    async fn run_cart_quantity_job_reaches_awaiting_commit() {
        let _wasmer = rustashop_sandbox::WASMER_TEST_GATE.lock().await;
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let job = registry.start_job(JOB_TYPE_CART_QUANTITY, "hash", "admin-bearer");
        let input = LegacyHookInput {
            hook: CART_UPDATE_QUANTITY_HOOK.into(),
            cart_id: "cart-spawn".into(),
            id_product: "variant-1".into(),
            quantity: 3,
            operator: "set".into(),
        };
        run_cart_quantity_job(
            &registry,
            &hub,
            &job.id,
            &input,
            &php_migration_hook_source(),
        )
        .await;
        let done = registry.get(&job.id).expect("job");
        assert_eq!(
            done.status,
            SandboxJobStatus::AwaitingCommit,
            "error={:?}",
            done.error
        );
        let proposal = done.proposal.expect("proposal");
        assert_eq!(proposal.operator, "set");
        assert_eq!(proposal.quantity, 3);
    }

    #[test]
    fn discard_sandbox_job_response_branches() {
        let auth = AdminAuthConfig::from_token("secret");
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();

        assert_eq!(
            discard_sandbox_job_response(&auth, None, &registry, &hub, "missing").status(),
            401
        );
        assert_eq!(
            discard_sandbox_job_response(&auth, Some("secret"), &registry, &hub, "missing")
                .status(),
            404
        );

        let job = registry.start_job(JOB_TYPE_CART_QUANTITY, "hash", "admin-bearer");
        assert_eq!(
            discard_sandbox_job_response(&auth, Some("secret"), &registry, &hub, &job.id).status(),
            422
        );

        registry.set_awaiting_commit(&job.id, sample_proposal());
        let discarded =
            discard_sandbox_job_response(&auth, Some("secret"), &registry, &hub, &job.id);
        assert_eq!(discarded.status(), 200);
        let body: SandboxJobResponse = serde_json::from_slice(discarded.body()).unwrap();
        assert_eq!(body.status, SandboxJobStatus::Discarded);
    }

    #[cfg(feature = "persist-sqlx")]
    #[tokio::test]
    async fn commit_sandbox_job_response_early_returns() {
        use rustashop_persist_sqlx::SqlxCatalogRepository;
        use sqlx::postgres::PgPoolOptions;

        let auth = AdminAuthConfig::from_token("secret");
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://rustashop:rustashop@127.0.0.1:5432/rustashop".into());
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect");
        let catalog = SqlxCatalogRepository::new(pool);

        assert_eq!(
            commit_sandbox_job_response(CommitSandboxJobContext {
                auth: &auth,
                bearer: None,
                registry: &registry,
                hub: &hub,
                cart_hub: None,
                catalog: &catalog,
                job_id: "missing",
            })
            .await
            .status(),
            401
        );
        assert_eq!(
            commit_sandbox_job_response(CommitSandboxJobContext {
                auth: &auth,
                bearer: Some("secret"),
                registry: &registry,
                hub: &hub,
                cart_hub: None,
                catalog: &catalog,
                job_id: "missing",
            })
            .await
            .status(),
            404
        );

        let job = registry.start_job(JOB_TYPE_CART_QUANTITY, "hash", "admin-bearer");
        assert_eq!(
            commit_sandbox_job_response(CommitSandboxJobContext {
                auth: &auth,
                bearer: Some("secret"),
                registry: &registry,
                hub: &hub,
                cart_hub: None,
                catalog: &catalog,
                job_id: &job.id,
            })
            .await
            .status(),
            422
        );

        registry.set_awaiting_commit_without_proposal(&job.id);
        assert_eq!(
            commit_sandbox_job_response(CommitSandboxJobContext {
                auth: &auth,
                bearer: Some("secret"),
                registry: &registry,
                hub: &hub,
                cart_hub: None,
                catalog: &catalog,
                job_id: &job.id,
            })
            .await
            .status(),
            422
        );
    }

    #[tokio::test]
    async fn run_cart_quantity_job_validation_failure_finishes_failed() {
        let _wasmer = rustashop_sandbox::WASMER_TEST_GATE.lock().await;
        let registry = SandboxJobRegistry::new();
        let hub = SandboxJobHub::new();
        let job = registry.start_job(JOB_TYPE_CART_QUANTITY, "hash", "admin-bearer");
        let input = LegacyHookInput {
            hook: CART_UPDATE_QUANTITY_HOOK.into(),
            cart_id: "cart-1".into(),
            id_product: "variant-1".into(),
            quantity: 2,
            operator: "noop".into(),
        };
        run_cart_quantity_job(
            &registry,
            &hub,
            &job.id,
            &input,
            &php_migration_hook_source(),
        )
        .await;
        let finished = registry.get(&job.id).expect("job");
        assert_eq!(finished.status, SandboxJobStatus::Failed);
        assert!(finished.error.is_some());
    }

    #[test]
    fn openapi_stubs_are_callable() {
        commit_sandbox_job();
        discard_sandbox_job();
    }

    #[cfg(feature = "persist-sqlx")]
    async fn seeded_catalog() -> (CatalogRepository, tokio::sync::MutexGuard<'static, ()>) {
        use std::sync::LazyLock;

        use rustashop_persist_sqlx::{SqlxCatalogRepository, migrate, seed_catalog};
        use sqlx::postgres::PgPoolOptions;
        use tokio::sync::Mutex;

        static DB_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
        // Same key as carts::cart_response_tests so schema resets serialize in Postgres.
        const SCHEMA_LOCK: i64 = 874_521;

        let guard = DB_LOCK.lock().await;
        let url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://rustashop:rustashop@127.0.0.1:5432/rustashop".into());
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .connect(&url)
            .await
            .expect("connect");
        sqlx::query("SELECT pg_advisory_lock($1)")
            .bind(SCHEMA_LOCK)
            .execute(&pool)
            .await
            .expect("lock");
        sqlx::query("DROP SCHEMA IF EXISTS public CASCADE")
            .execute(&pool)
            .await
            .expect("drop");
        sqlx::query("CREATE SCHEMA public")
            .execute(&pool)
            .await
            .expect("create");
        sqlx::query("GRANT ALL ON SCHEMA public TO PUBLIC")
            .execute(&pool)
            .await
            .ok();
        migrate(&pool).await.expect("migrate");
        seed_catalog(&pool).await.expect("seed");
        (SqlxCatalogRepository::new(pool), guard)
    }

    #[cfg(feature = "persist-sqlx")]
    mod persist_tests {
        use super::*;
        use crate::carts::{add_cart_line_response, create_cart_response};

        const HOODIE_VARIANT: &str = "33333333-3333-3333-3333-333333333331";

        async fn cart_with_hoodie(catalog: &CatalogRepository, quantity: i32) -> CartResponse {
            let created = create_cart_response(catalog, None, br#"{"currency":"EUR"}"#).await;
            assert_eq!(created.status(), 201);
            let cart: CartResponse = serde_json::from_slice(created.body()).unwrap();
            let body = format!(r#"{{"variant_id":"{HOODIE_VARIANT}","quantity":{quantity}}}"#);
            let added = add_cart_line_response(catalog, None, &cart.id, body.as_bytes()).await;
            assert_eq!(added.status(), 200);
            serde_json::from_slice(added.body()).unwrap()
        }

        #[tokio::test]
        async fn commit_applies_set_up_down_and_publishes_cart() {
            let (catalog, _guard) = seeded_catalog().await;
            let auth = AdminAuthConfig::from_token("secret");
            let registry = SandboxJobRegistry::new();
            let hub = SandboxJobHub::new();
            let cart_hub = CartHub::new();
            let cart = cart_with_hoodie(&catalog, 2).await;
            let mut rx = cart_hub.subscribe(&cart.id);

            let job = registry.start_job(JOB_TYPE_CART_QUANTITY, "hash", "admin-bearer");
            registry.set_awaiting_commit(
                &job.id,
                SandboxProposalResponse {
                    event_type: "cart.line_quantity_proposed".into(),
                    cart_id: cart.id.clone(),
                    product_id: HOODIE_VARIANT.into(),
                    quantity: 5,
                    operator: "set".into(),
                },
            );
            let response = commit_sandbox_job_response(CommitSandboxJobContext {
                auth: &auth,
                bearer: Some("secret"),
                registry: &registry,
                hub: &hub,
                cart_hub: Some(&cart_hub),
                catalog: &catalog,
                job_id: &job.id,
            })
            .await;
            assert_eq!(response.status(), 200);
            let body: CommitSandboxJobResponse = serde_json::from_slice(response.body()).unwrap();
            assert_eq!(body.job.status, SandboxJobStatus::Committed);
            assert_eq!(body.cart.lines[0].quantity, 5);
            let event = rx.recv().await.expect("cart event");
            assert!(event.contains("cart.updated"));

            let up_job = registry.start_job(JOB_TYPE_CART_QUANTITY, "hash", "admin-bearer");
            registry.set_awaiting_commit(
                &up_job.id,
                SandboxProposalResponse {
                    event_type: "cart.line_quantity_proposed".into(),
                    cart_id: cart.id.clone(),
                    product_id: HOODIE_VARIANT.into(),
                    quantity: 2,
                    operator: "up".into(),
                },
            );
            let up = commit_sandbox_job_response(CommitSandboxJobContext {
                auth: &auth,
                bearer: Some("secret"),
                registry: &registry,
                hub: &hub,
                cart_hub: None,
                catalog: &catalog,
                job_id: &up_job.id,
            })
            .await;
            assert_eq!(up.status(), 200);
            let up_body: CommitSandboxJobResponse = serde_json::from_slice(up.body()).unwrap();
            assert_eq!(up_body.cart.lines[0].quantity, 7);

            let down_job = registry.start_job(JOB_TYPE_CART_QUANTITY, "hash", "admin-bearer");
            registry.set_awaiting_commit(
                &down_job.id,
                SandboxProposalResponse {
                    event_type: "cart.line_quantity_proposed".into(),
                    cart_id: cart.id.clone(),
                    product_id: HOODIE_VARIANT.into(),
                    quantity: 1,
                    operator: "down".into(),
                },
            );
            let down = commit_sandbox_job_response(CommitSandboxJobContext {
                auth: &auth,
                bearer: Some("secret"),
                registry: &registry,
                hub: &hub,
                cart_hub: None,
                catalog: &catalog,
                job_id: &down_job.id,
            })
            .await;
            assert_eq!(down.status(), 200);
            let down_body: CommitSandboxJobResponse = serde_json::from_slice(down.body()).unwrap();
            assert_eq!(down_body.cart.lines[0].quantity, 6);
        }

        #[tokio::test]
        async fn apply_cart_quantity_proposal_errors() {
            let (catalog, _guard) = seeded_catalog().await;
            let missing_cart = apply_cart_quantity_proposal(
                &catalog,
                None,
                &SandboxProposalResponse {
                    event_type: "cart.line_quantity_proposed".into(),
                    cart_id: "11111111-1111-1111-1111-111111111111".into(),
                    product_id: HOODIE_VARIANT.into(),
                    quantity: 1,
                    operator: "set".into(),
                },
            )
            .await;
            assert!(matches!(missing_cart, Err(ApiError::NotFound)));

            let cart = cart_with_hoodie(&catalog, 2).await;
            let missing_line = apply_cart_quantity_proposal(
                &catalog,
                None,
                &SandboxProposalResponse {
                    event_type: "cart.line_quantity_proposed".into(),
                    cart_id: cart.id.clone(),
                    product_id: "33333333-3333-3333-3333-333333333399".into(),
                    quantity: 1,
                    operator: "set".into(),
                },
            )
            .await;
            assert!(matches!(missing_line, Err(ApiError::NotFound)));

            let bad_qty = apply_cart_quantity_proposal(
                &catalog,
                None,
                &SandboxProposalResponse {
                    event_type: "cart.line_quantity_proposed".into(),
                    cart_id: cart.id,
                    product_id: HOODIE_VARIANT.into(),
                    quantity: 10,
                    operator: "down".into(),
                },
            )
            .await;
            assert!(matches!(bad_qty, Err(ApiError::Unprocessable(_))));
        }
    }
}
