# wasmer-php-migration fixture

Bridges one PrestaShop-style hook (`actionCartUpdateQuantityBefore`) to a rustashop
domain event draft. The Wasmer PHP guest proposes; the Rust host validates and
commits (or rejects). No PHP-FPM.

## Contract

- **stdin:** JSON `LegacyHookInput` (`hook`, `cart_id`, `id_product`, `quantity`, `operator`)
- **stdout:** JSON `DomainEventDraft` (`event_type`, `cart_id`, `product_id`, `quantity`, `operator`)

## Run (host)

```bash
cargo test -p rustashop-sandbox php_migration
```
