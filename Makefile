# rustashop developer targets

SHELL := /bin/bash
ROOT := $(abspath .)
CARGO ?= cargo
CLIPPY_FLAGS := -D warnings -D clippy::all -D clippy::pedantic -D clippy::nursery
RUSTDOCFLAGS ?= -D warnings
API_BIND ?= 127.0.0.1:8080
DATABASE_URL ?= postgres://rustashop:rustashop@127.0.0.1:5432/rustashop
OPENAPI_OUT ?= openapi/openapi.json
SHOP_ANGULAR_DIR := shops/angular
SHOP_ANGULAR_PORT ?= 4242
ADMIN_ANGULAR_DIR := admin/angular
ADMIN_ANGULAR_PORT ?= 4250
SHOP_LEPTOS_DIR := shops/leptos-rangular
SHOP_LEPTOS_PORT ?= 4181
SHOP_LEPTOS_ADDR ?= 127.0.0.1
TRUNK_BIN ?= $(HOME)/.cargo/bin/trunk
TRUNK ?= env -u NO_COLOR $(TRUNK_BIN)
# Compose file lives under docker/; project name keeps container names stable.
COMPOSE := docker compose -f docker/compose.yml --project-directory $(ROOT)
# Set FORCE=1 to re-run npm install even when node_modules exists.
FORCE ?= 0
CVE_LITE_CLI := cve-lite-cli@1.33.0

# Published API image (local compose still tags rustashop-api:local).
HUB_IMAGE ?= interchouette/rustashop
GHCR_PERSONAL_IMAGE ?= ghcr.io/groussac/rustashop
GHCR_WORKER_IMAGE ?= ghcr.io/interchouette/rustashop
GHCR_ORG_IMAGE ?= ghcr.io/interchouette-itc/rustashop
DOCKERFILE ?= docker/Dockerfile
DOCKER_BUILDKIT ?= 1
TAG ?= latest
APP_VERSION ?= $(shell awk '/^version = /{gsub(/"/, "", $$3); print $$3; exit}' Cargo.toml)
CI ?= 0

.DEFAULT_GOAL := help

.PHONY: help check test lint lint-shop-angular lint-admin-angular lint-install format format-check check-sql-safety doc doc-open doc-clean openapi openapi-check run-api clean db-up db-down db-psql db-wait db-migrate db-migrate-seaorm db-seed db-reset stack-up shop-angular admin-angular shop-leptos-rangular install-ui install-dev install-cli audit deny audit-npm audit-all coverage coverage-js ci \
	docker-build docker-build-no-cache docker-build-dev docker-push-dev \
	docker-push-dev-hub docker-push-dev-ghcr-personal docker-push-dev-ghcr-itc \
	docker-push-release docker-push-release-hub \
	docker-push-release-ghcr-personal docker-push-release-ghcr-itc

SEAORM_PACKAGES := -p rustashop-persist -p rustashop-persist-seaorm -p rustashop-api
SEAORM_FEATURES := --no-default-features --features persist-seaorm

help:
	@echo "rustashop targets"
	@echo ""
	@echo "  make check      cargo check --workspace, then SeaORM features"
	@echo "  make test       cargo test --workspace, then SeaORM feature tests"
	@echo "  make coverage   cargo llvm-cov → coverage/lcov.info (needs DATABASE_URL for integration)"
	@echo "  make coverage-js Vitest coverage for shop, admin, and install → coverage/*-lcov.info"
	@echo "  make lint       fmt check + SQL safety + clippy + Angular shop/admin lint (when node_modules present)"
	@echo "  make ci         lint + test + doc + audit + deny (local mirror of core CI jobs before opening a PR)"
	@echo "  make check-sql-safety  cargo test: deny format!-built SQL in persist crates"
	@echo "  make doc        rustdoc → docs/api-rust/ (-D warnings)"
	@echo "  make doc-open   build docs and open docs/api-rust/index.html"
	@echo "  make openapi    write $(OPENAPI_OUT) from utoipa"
	@echo "  make openapi-check  regenerate OpenAPI and fail if $(OPENAPI_OUT) drifts"
	@echo "  make shop-angular  serve Angular shop ($(SHOP_ANGULAR_DIR), port $(SHOP_ANGULAR_PORT); FORCE=1 reinstalls; RUSTASHOP_BASE_HREF=/)"
	@echo "  make admin-angular serve Angular admin ($(ADMIN_ANGULAR_DIR), port $(ADMIN_ANGULAR_PORT); FORCE=1 reinstalls)"
	@echo "  make shop-leptos-rangular  serve Leptos+rangular shop ($(SHOP_LEPTOS_DIR), port $(SHOP_LEPTOS_PORT))"
	@echo "  make install-ui  build Vite+Vue install funnel into install/dist (API serves /install when present)"
	@echo "  make install-dev Vite dev server for install UI (proxies /install/api via RUSTASHOP_API_PROXY / RUSTASHOP_BIND)"
	@echo "  make install-cli run rustashop-install (writes .env; then mv install install.off)"
	@echo "  make format     cargo fmt"
	@echo "  make audit      cargo audit"
	@echo "  make deny       cargo deny check"
	@echo "  make audit-npm  npm audit + cve-lite + malware IoC (shop/admin/install)"
	@echo "  make audit-all  audit + deny + audit-npm"
	@echo "  make run-api    start Actix API on the host (RUSTASHOP_BIND, default $(API_BIND))"
	@echo "  make db-up      start Postgres via docker compose"
	@echo "  make db-down    stop the compose project (Postgres and API if started)"
	@echo "  make stack-up   build and start Postgres + migrate + API"
	@echo "  make db-psql    psql shell (needs db-up)"
	@echo "  make db-migrate run SQLx migrations (needs db-up, DATABASE_URL)"
	@echo "  make db-migrate-seaorm run SeaORM migrations (needs db-up, DATABASE_URL)"
	@echo "  make db-seed    load catalog seed (idempotent; never drops data)"
	@echo "  make db-reset   DROP SCHEMA public + migrate (requires CONFIRM=YES)"
	@echo "  make docker-build       Build $(HUB_IMAGE):$(TAG) (+ :$(APP_VERSION))"
	@echo "  make docker-build-dev   Build and tag :dev (Hub + GHCR names)"
	@echo "  make docker-push-dev    Push :dev (local interactive logins)"
	@echo "  make docker-push-release  Push :$(APP_VERSION) + :latest (split targets for CI)"
	@echo "  make clean      cargo clean"
	@echo ""
	@echo "Overrides: API_BIND=$(API_BIND) SHOP_ANGULAR_PORT=$(SHOP_ANGULAR_PORT) ADMIN_ANGULAR_PORT=$(ADMIN_ANGULAR_PORT) SHOP_LEPTOS_PORT=$(SHOP_LEPTOS_PORT) RUSTASHOP_API_PROXY FORCE=$(FORCE) CONFIRM= HUB_IMAGE=$(HUB_IMAGE) APP_VERSION=$(APP_VERSION)"

check:
	cd $(ROOT) && $(CARGO) check --workspace
	cd $(ROOT) && $(CARGO) check $(SEAORM_PACKAGES) $(SEAORM_FEATURES)

test:
	cd $(ROOT) && $(CARGO) test --workspace
	cd $(ROOT) && $(CARGO) test $(SEAORM_PACKAGES) $(SEAORM_FEATURES)

## Local mirror of core CI jobs. Run before every PR create/update (see .cursor/rules/rustashop-ci-before-pr.mdc).
ci: lint test doc openapi-check audit deny

## Requires `cargo install cargo-llvm-cov`. Writes `coverage/lcov.info`.
## Uses the stable toolchain so llvm-cov finds instrumented objects.
## Integration tests need DATABASE_URL (make db-up).
## Bins match Codecov ignore paths so local lcov stays aligned.
coverage:
	cd $(ROOT) && mkdir -p coverage && RUSTUP_TOOLCHAIN=stable $(CARGO) llvm-cov --workspace \
		--exclude rustashop-persist-seaorm --lcov \
		--ignore-filename-regex '/src/bin/|/src/main\.rs$$' \
		--output-path coverage/lcov.info
	cd $(ROOT) && RUSTUP_TOOLCHAIN=stable $(CARGO) llvm-cov $(SEAORM_PACKAGES) $(SEAORM_FEATURES) --lcov \
		--ignore-filename-regex '/src/bin/|/src/main\.rs$$' \
		--output-path coverage/lcov-seaorm.info

## Vitest coverage for shop, admin, and install. Writes coverage/*-lcov.info.
coverage-js:
	cd $(ROOT) && mkdir -p coverage
	@set -e; \
	cd $(ROOT)/$(SHOP_ANGULAR_DIR) && \
		if [ "$(FORCE)" = "1" ] || [ ! -d node_modules ]; then npm ci; fi && \
		npm run test:coverage && \
		cp -f coverage/shop-angular/lcov.info $(ROOT)/coverage/shop-lcov.info
	@set -e; \
	cd $(ROOT)/$(ADMIN_ANGULAR_DIR) && \
		if [ "$(FORCE)" = "1" ] || [ ! -d node_modules ]; then npm ci; fi && \
		npm run test:coverage && \
		cp -f coverage/admin-angular/lcov.info $(ROOT)/coverage/admin-lcov.info
	@set -e; \
	cd $(ROOT)/install && \
		if [ "$(FORCE)" = "1" ] || [ ! -d node_modules ]; then npm ci; fi && \
		npm run test:coverage && \
		cp -f coverage/lcov.info $(ROOT)/coverage/install-lcov.info

format:
	cd $(ROOT) && $(CARGO) fmt

format-check:
	cd $(ROOT) && $(CARGO) fmt --check

check-sql-safety:
	cd $(ROOT) && $(CARGO) test -p rustashop-persist-sqlx --test sql_safety

# Angular shop eslint. Skips only when node_modules is missing (fresh clone without npm install).
lint-shop-angular:
	@if [ ! -d "$(ROOT)/$(SHOP_ANGULAR_DIR)/node_modules" ]; then \
		echo "skip lint-shop-angular: $(SHOP_ANGULAR_DIR)/node_modules missing (npm install or make shop-angular)"; \
	else \
		cd $(ROOT)/$(SHOP_ANGULAR_DIR) && npm run lint; \
	fi

# Angular admin eslint. Same skip rule as shop.
lint-admin-angular:
	@if [ ! -d "$(ROOT)/$(ADMIN_ANGULAR_DIR)/node_modules" ]; then \
		echo "skip lint-admin-angular: $(ADMIN_ANGULAR_DIR)/node_modules missing (npm install or make admin-angular)"; \
	else \
		cd $(ROOT)/$(ADMIN_ANGULAR_DIR) && npm run lint; \
	fi

lint-install:
	@if [ ! -d "$(ROOT)/install/node_modules" ]; then \
		echo "skip lint-install: install/node_modules missing (make install-ui)"; \
	else \
		cd $(ROOT)/install && npm run build; \
	fi

lint: format-check check-sql-safety lint-shop-angular lint-admin-angular lint-install
	cd $(ROOT) && $(CARGO) clippy --workspace --all-targets -- $(CLIPPY_FLAGS)
	cd $(ROOT) && $(CARGO) clippy $(SEAORM_PACKAGES) --all-targets $(SEAORM_FEATURES) -- $(CLIPPY_FLAGS)

## Requires `cargo install cargo-audit`.
## RUSTSEC-2026-0258: actix-http 3.x pins h2 0.3; fix is only on h2 >= 0.4.16.
audit:
	cd $(ROOT) && $(CARGO) audit --ignore RUSTSEC-2026-0258

## Requires `cargo install cargo-deny`.
deny:
	cd $(ROOT) && $(CARGO) deny check

audit-npm:
	@set -e; \
	for d in $(SHOP_ANGULAR_DIR) $(ADMIN_ANGULAR_DIR) install; do \
		echo "==> $$d"; \
		cd $(ROOT)/$$d; \
		if [ ! -d node_modules ]; then npm ci; fi; \
		npm audit --audit-level=high --omit=dev; \
		NPM_CONFIG_MIN_RELEASE_AGE=0 npx --yes --package=$(CVE_LITE_CLI) cve-lite . --fail-on high; \
	done; \
	cd $(ROOT) && node scripts/npm-malware-scan.mjs .

audit-all: audit deny audit-npm

install-ui:
	cd $(ROOT)/install && \
		if [ "$(FORCE)" = "1" ] || [ ! -d node_modules ]; then npm ci; fi && \
		npm run build

install-dev:
	@raw="$${RUSTASHOP_API_PROXY:-$${RUSTASHOP_BIND:-$(API_BIND)}}"; \
	case "$$raw" in \
		http://*|https://*) API_PROXY="$${raw%/}" ;; \
		*) API_PROXY="http://$${raw%/}" ;; \
	esac; \
	cd $(ROOT)/install && \
		if [ "$(FORCE)" = "1" ] || [ ! -d node_modules ]; then npm ci; fi && \
		RUSTASHOP_API_PROXY="$$API_PROXY" npm run dev

install-cli:
	cd $(ROOT) && $(CARGO) run -p rustashop-api --bin rustashop-install --

## rustdoc → `docs/api-rust/` (GitHub Pages publish folder).
DOC_OUT ?= $(ROOT)/target/doc
DOC_CRATE ?= rustashop

doc:
	cd $(ROOT) && RUSTDOCFLAGS="$(RUSTDOCFLAGS)" $(CARGO) doc --workspace --no-deps
	@test -d "$(DOC_OUT)" || (echo "missing $(DOC_OUT)"; exit 1)
	@rm -rf $(ROOT)/docs/api-rust
	@mkdir -p $(ROOT)/docs/api-rust
	@cp -a "$(DOC_OUT)/." $(ROOT)/docs/api-rust/
	@printf '%s\n' \
		'<!DOCTYPE html>' \
		'<html lang="en">' \
		'<head>' \
		'<meta charset="utf-8">' \
		'<meta http-equiv="refresh" content="0; url=$(DOC_CRATE)/index.html">' \
		'<title>rustashop - Rust API docs</title>' \
		'<link rel="canonical" href="$(DOC_CRATE)/index.html">' \
		'<script>location.replace("$(DOC_CRATE)/index.html");</script>' \
		'</head>' \
		'<body><p><a href="$(DOC_CRATE)/index.html">rustashop API documentation</a></p></body>' \
		'</html>' \
		> $(ROOT)/docs/api-rust/index.html
	@touch $(ROOT)/docs/api-rust/.nojekyll
	@printf '%s\n' \
		'# Rust API documentation (rustdoc)' \
		'' \
		'Open [`$(DOC_CRATE)/index.html`]($(DOC_CRATE)/index.html) (root [`index.html`](index.html) redirects there).' \
		> $(ROOT)/docs/api-rust/README.md
	@echo "docs/api-rust/ updated - open docs/api-rust/index.html"

doc-open: doc
	@xdg-open $(ROOT)/docs/api-rust/index.html 2>/dev/null \
		|| open $(ROOT)/docs/api-rust/index.html 2>/dev/null \
		|| echo "Open docs/api-rust/index.html in a browser"

doc-clean:
	rm -rf $(ROOT)/docs/api-rust
	mkdir -p $(ROOT)/docs/api-rust
	@printf '%s\n' \
		'# Rust API documentation (rustdoc)' \
		'' \
		'Generated by `make doc`. Open [`index.html`](index.html) after building.' \
		> $(ROOT)/docs/api-rust/README.md

openapi:
	cd $(ROOT) && $(CARGO) run -p rustashop-api --bin rustashop-openapi -- $(OPENAPI_OUT)

## Regenerate OpenAPI into a temp file and fail if it differs from the committed dump.
openapi-check:
	@tmpdir=$$(mktemp -d) && \
	trap 'rm -rf "$$tmpdir"' EXIT && \
	cd $(ROOT) && $(CARGO) run -p rustashop-api --bin rustashop-openapi -- "$$tmpdir/openapi.json" && \
	if ! diff -u $(OPENAPI_OUT) "$$tmpdir/openapi.json"; then \
		echo "OpenAPI dump drifted. Run \`make openapi\` and commit $(OPENAPI_OUT)."; \
		exit 1; \
	fi && \
	echo "OpenAPI dump matches utoipa ($(OPENAPI_OUT))."

shop-angular:
	@if ss -tln | grep -qE ':$(SHOP_ANGULAR_PORT)\\b'; then \
		printf '\033[1;31m✗\033[0m  port \033[1m$(SHOP_ANGULAR_PORT)\033[0m busy - stop the other process or set SHOP_ANGULAR_PORT=\n'; \
		exit 1; \
	fi
	@raw="$${RUSTASHOP_API_PROXY:-$(API_BIND)}"; \
	case "$$raw" in \
		http://*|https://*) API_PROXY="$${raw%/}" ;; \
		*) API_PROXY="http://$${raw%/}" ;; \
	esac; \
	BASE_HREF=$${RUSTASHOP_BASE_HREF:-/}; \
	case "$$BASE_HREF" in /|*/) ;; *) BASE_HREF="$$BASE_HREF/";; esac; \
	if curl -sf "$${API_PROXY}/healthz" >/dev/null; then \
		API_STATE=ready; API_COLOR='\033[32m'; \
	else \
		API_STATE=down; API_COLOR='\033[33m'; \
	fi; \
	printf '\n'; \
	printf '  🦀 \033[97m\033[1mrusta\033[0m\033[38;2;235;65;1m\033[1mshop\033[0m 🛒\n'; \
	printf '  \033[2m─────────────────────────────────────\033[0m\n'; \
	printf '  \033[37mapp\033[0m    shop-angular\n'; \
	printf '  \033[37mshop\033[0m   http://127.0.0.1:$(SHOP_ANGULAR_PORT)/\n'; \
	printf '  \033[37mapi\033[0m    %s  %b%s\033[0m\n' "$$API_PROXY" "$$API_COLOR" "$$API_STATE"; \
	printf '  \033[37mbase\033[0m   %s\n' "$$BASE_HREF"; \
	printf '\n'; \
	if [ "$$API_STATE" = down ]; then \
		printf '  \033[38;2;235;65;1m!\033[0m  API not reachable - \033[1mmake run-api\033[0m (or RUSTASHOP_API_PROXY=…)\n\n'; \
	fi; \
	cd $(ROOT)/$(SHOP_ANGULAR_DIR) && \
		if [ "$(FORCE)" = "1" ] || [ ! -d node_modules ]; then npm ci; fi && \
		npm run generate:api && \
		SERVE_EXTRA=; \
		if [ "$$BASE_HREF" != "/" ]; then SERVE_EXTRA="--serve-path $$BASE_HREF"; fi; \
		printf '  \033[2mstarting ng serve…\033[0m\n\n'; \
		RUSTASHOP_API_PROXY="$$API_PROXY" npm start -- --port $(SHOP_ANGULAR_PORT) $$SERVE_EXTRA

admin-angular:
	@if ss -tln | grep -qE ':$(ADMIN_ANGULAR_PORT)\\b'; then \
		printf '\033[1;31m✗\033[0m  port \033[1m$(ADMIN_ANGULAR_PORT)\033[0m busy - stop the other process or set ADMIN_ANGULAR_PORT=\n'; \
		exit 1; \
	fi
	@raw="$${RUSTASHOP_API_PROXY:-$(API_BIND)}"; \
	case "$$raw" in \
		http://*|https://*) API_PROXY="$${raw%/}" ;; \
		*) API_PROXY="http://$${raw%/}" ;; \
	esac; \
	if curl -sf "$${API_PROXY}/healthz" >/dev/null; then \
		API_STATE=ready; API_COLOR='\033[32m'; \
	else \
		API_STATE=down; API_COLOR='\033[33m'; \
	fi; \
	printf '\n'; \
	printf '  🦀 \033[97m\033[1mrusta\033[0m\033[38;2;235;65;1m\033[1mshop\033[0m admin\n'; \
	printf '  \033[2m─────────────────────────────────────\033[0m\n'; \
	printf '  \033[37mapp\033[0m    admin-angular\n'; \
	printf '  \033[37madmin\033[0m  http://127.0.0.1:$(ADMIN_ANGULAR_PORT)/\n'; \
	printf '  \033[37mapi\033[0m    %s  %b%s\033[0m\n' "$$API_PROXY" "$$API_COLOR" "$$API_STATE"; \
	printf '\n'; \
	if [ "$$API_STATE" = down ]; then \
		printf '  \033[38;2;235;65;1m!\033[0m  API not reachable - \033[1mmake run-api\033[0m (or RUSTASHOP_API_PROXY=…)\n\n'; \
	fi; \
	cd $(ROOT)/$(ADMIN_ANGULAR_DIR) && \
		if [ "$(FORCE)" = "1" ] || [ ! -d node_modules ]; then npm ci; fi && \
		printf '  \033[2mstarting ng serve…\033[0m\n\n'; \
		RUSTASHOP_API_PROXY="$$API_PROXY" npm start -- --port $(ADMIN_ANGULAR_PORT)

shop-leptos-rangular:
	@if ss -tlnp 2>/dev/null | grep -q ':$(SHOP_LEPTOS_PORT) '; then \
		echo "Port $(SHOP_LEPTOS_PORT) already in use - reuse that server or set SHOP_LEPTOS_PORT"; \
		exit 1; \
	fi
	@test -x $(TRUNK_BIN) || { echo "trunk not found at $(TRUNK_BIN) (install: cargo install trunk)"; exit 1; }
	cd $(ROOT)/$(SHOP_LEPTOS_DIR) && env -u NO_COLOR $(TRUNK) serve --release --port $(SHOP_LEPTOS_PORT) --address $(SHOP_LEPTOS_ADDR)

run-api:
	cd $(ROOT) && DATABASE_URL=$(DATABASE_URL) RUSTASHOP_BIND=$${RUSTASHOP_BIND:-$(API_BIND)} $(CARGO) run -p rustashop-api --bin rustashop-api

stack-up:
	cd $(ROOT) && $(COMPOSE) up --build -d

docker-build:
	cd $(ROOT) && DOCKER_BUILDKIT=$(DOCKER_BUILDKIT) docker build \
		--network=host \
		-t $(HUB_IMAGE):$(TAG) \
		-t $(HUB_IMAGE):$(APP_VERSION) \
		-f $(DOCKERFILE) \
		.

docker-build-no-cache:
	cd $(ROOT) && DOCKER_BUILDKIT=$(DOCKER_BUILDKIT) docker build \
		--no-cache \
		--network=host \
		-t $(HUB_IMAGE):$(TAG) \
		-t $(HUB_IMAGE):$(APP_VERSION) \
		-f $(DOCKERFILE) \
		.

docker-build-dev:
	cd $(ROOT) && DOCKER_BUILDKIT=$(DOCKER_BUILDKIT) docker build \
		--network=host \
		-t rustashop:dev \
		-t $(HUB_IMAGE):dev \
		-t $(GHCR_PERSONAL_IMAGE):dev \
		-t $(GHCR_WORKER_IMAGE):dev \
		-t $(GHCR_ORG_IMAGE):dev \
		-f $(DOCKERFILE) \
		.

docker-push-dev-hub:
	docker push $(HUB_IMAGE):dev

docker-push-dev-ghcr-personal:
	docker push $(GHCR_PERSONAL_IMAGE):dev

docker-push-dev-ghcr-itc:
	docker push $(GHCR_WORKER_IMAGE):dev
	docker push $(GHCR_ORG_IMAGE):dev

docker-push-dev:
	@if [ "$(CI)" = "1" ]; then \
		echo "Use docker-push-dev-hub / docker-push-dev-ghcr-personal / docker-push-dev-ghcr-itc in CI"; \
		exit 1; \
	fi
	@echo "Logging in to Docker Hub..."; \
	docker login || { echo "Docker Hub login failed"; exit 1; }
	$(MAKE) docker-push-dev-hub
	@echo "Logging in to GHCR (personal)..."; \
	docker login ghcr.io || { echo "Skipping personal GHCR"; exit 0; }
	$(MAKE) docker-push-dev-ghcr-personal
	@echo "Logging in to GHCR (Interchouette / ITC)..."; \
	docker login ghcr.io || { echo "Skipping ITC GHCR"; exit 0; }
	$(MAKE) docker-push-dev-ghcr-itc

docker-push-release-hub:
	docker push $(HUB_IMAGE):$(APP_VERSION)
	docker push $(HUB_IMAGE):latest

docker-push-release-ghcr-personal:
	docker tag $(HUB_IMAGE):$(APP_VERSION) $(GHCR_PERSONAL_IMAGE):$(APP_VERSION)
	docker tag $(HUB_IMAGE):latest $(GHCR_PERSONAL_IMAGE):latest
	docker push $(GHCR_PERSONAL_IMAGE):$(APP_VERSION)
	docker push $(GHCR_PERSONAL_IMAGE):latest

docker-push-release-ghcr-itc:
	docker tag $(HUB_IMAGE):$(APP_VERSION) $(GHCR_WORKER_IMAGE):$(APP_VERSION)
	docker tag $(HUB_IMAGE):latest $(GHCR_WORKER_IMAGE):latest
	docker tag $(HUB_IMAGE):$(APP_VERSION) $(GHCR_ORG_IMAGE):$(APP_VERSION)
	docker tag $(HUB_IMAGE):latest $(GHCR_ORG_IMAGE):latest
	docker push $(GHCR_WORKER_IMAGE):$(APP_VERSION)
	docker push $(GHCR_WORKER_IMAGE):latest
	docker push $(GHCR_ORG_IMAGE):$(APP_VERSION)
	docker push $(GHCR_ORG_IMAGE):latest

docker-push-release: docker-push-release-hub docker-push-release-ghcr-personal docker-push-release-ghcr-itc

db-up:
	cd $(ROOT) && $(COMPOSE) up -d postgres
	@$(MAKE) db-wait

db-down:
	cd $(ROOT) && $(COMPOSE) down

db-wait:
	@cd $(ROOT) && $(COMPOSE) exec -T postgres sh -c 'until pg_isready -U rustashop -d rustashop; do sleep 1; done'

db-psql:
	cd $(ROOT) && $(COMPOSE) exec postgres psql -U rustashop -d rustashop

db-migrate:
	cd $(ROOT) && DATABASE_URL=$(DATABASE_URL) $(CARGO) run -p rustashop-persist-sqlx --bin rustashop-migrate

db-migrate-seaorm:
	cd $(ROOT) && DATABASE_URL=$(DATABASE_URL) $(CARGO) run -p rustashop-persist-seaorm --bin rustashop-seaorm-migrate

db-seed:
	@echo "Seeding catalog (idempotent; does not wipe the database)"
	@echo "Expect INSERT 0 0 / UPDATE N on re-run when rows already exist."
	cd $(ROOT) && $(COMPOSE) exec -T postgres psql -U rustashop -d rustashop < db/seeds/catalog.sql

# Destroys ALL tables and data in the rustashop database, then re-migrates.
# Refuses unless CONFIRM=YES (example: CONFIRM=YES make db-reset).
db-reset:
	@if [ "$(CONFIRM)" != "YES" ]; then \
		echo ""; \
		echo "REFUSING: make db-reset DROPS SCHEMA public CASCADE."; \
		echo "That deletes every table and all data in database 'rustashop'."; \
		echo "This is not undoable from Make."; \
		echo ""; \
		echo "If you really want that, run:"; \
		echo "  CONFIRM=YES make db-reset"; \
		echo ""; \
		exit 1; \
	fi
	@echo "CONFIRM=YES: dropping schema public and re-running migrations..."
	cd $(ROOT) && $(COMPOSE) exec -T postgres psql -U rustashop -d rustashop -c 'DROP SCHEMA public CASCADE; CREATE SCHEMA public;'
	@$(MAKE) db-migrate

clean:
	cd $(ROOT) && $(CARGO) clean
