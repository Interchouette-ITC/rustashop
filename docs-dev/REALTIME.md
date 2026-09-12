# Realtime and WebSockets

## Opinion

Live shop state is a **product default**, not a plugin. rustashop treats a push channel as part of the platform identity (similar in spirit to Meteor’s realtime default), while keeping HTTP/OpenAPI for bootstrap, idempotent commands, and inbound provider webhooks.

## Transport

| Now                                                        | Later                                                                            |
| ---------------------------------------------------------- | -------------------------------------------------------------------------------- |
| WebSocket as first-class gateway beside the Actix HTTP API | Evaluate WebTransport where it helps (unreliable datagrams, HTTP/3 environments) |

One gateway serves **both** UI stacks and the admin shell.

## Event categories (v0 interest)

| Category                | Examples                                                  | Priority       |
| ----------------------- | --------------------------------------------------------- | -------------- |
| Cart / checkout session | Line changes, totals, validation errors, lock for payment | High           |
| Inventory signals       | Low stock, sold out for a variant                         | High           |
| Order lifecycle         | Status transitions for customer and admin                 | High           |
| Admin feeds             | New orders, failed webhooks, sandbox job status           | High for admin |
| Presence / chat         | Support presence                                          | Defer          |

## Protocol shape (intent)

- **Typed** events with stable names and versioned payloads (JSON or a documented binary profile later).
- Align names with domain language used in OpenAPI (same `order_id`, money as integer minor units, etc.).
- Clients may apply optimistic updates; **server events win** on conflict.
- Auth: same session/bearer story as HTTP, bound to the socket.

Exact schema lands in `docs/API.md` / OpenAPI extensions when the server exists. Until then, design discussions should propose event names next to REST resources.

## Plugins and sandboxes

Wasm plugins and Wasmer jobs do not open sockets to browsers. They:

1. Call host APIs or return results to the host.
2. Host emits realtime events if the domain state changed.
3. Admin may subscribe to **sandbox job** events (stdout chunks, exit, artifacts) over the same gateway.

Autonomous `cart_quantity` jobs emit `job.log`, `job.proposal`, then `job.finished` (`committed` / `discarded` / `error`). A successful host commit also publishes `cart.updated` on the cart hub.

## Build early / defer

**Build early (alongside or just after cart HTTP):**

- Session-scoped cart updates
- Inventory signals for items in view / in cart
- Order status for the owning customer and admin list

**Defer:**

- Full catalog mirror over WS
- Replacing Stripe/Mollie webhooks with WS
- Making every admin screen WS-only before domain endpoints exist

## Acceptance ideas

- Angular and rangular clients both receive the same cart-total event after one HTTP line add.
- Integration test: connect WS, mutate cart via HTTP, assert event payload.
- Load smoke: many subscribers on one order status stream without blocking checkout HTTP.
