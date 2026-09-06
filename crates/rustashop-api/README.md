# rustashop-api

Actix-web commerce HTTP surface: catalog, cart, checkout, admin routes, cart
WebSocket, install static, and OpenAPI via utoipa.

JSON handlers bind through the Serenade HTTP front (Actix adapter). Requires a
configured persistence backend and `DATABASE_URL` for integration tests.
