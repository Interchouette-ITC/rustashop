# Leptos + rangular shop (track B)

```bash
make run-api               # :8080
make shop-leptos-rangular  # :4181, proxies /api → API
make shop-tauri            # same wasm in a Tauri window
```

Routes: `/` catalog, `/products/:id` add-to-cart, `/cart`, `/checkout`.
Cart id key `rs.cartId` matches the Angular shop (same browser = shared cart).
Shared card markup: `templates/shop/default/product_card`.

API base defaults to `/api` (Trunk proxy) in the browser, or `http://127.0.0.1:8080` on non-http origins. Override with the shell bar or `sessionStorage` key `rs.shopApiBase`.
