//! Catalog JSON cache via `serenade-cache` ([`ArrayAdapter`] + tag invalidation).

use std::sync::Arc;
use std::time::Duration;

use serenade_cache::{ArrayAdapter, ArrayCacheItem, CacheItem, CacheItemPool};

use crate::products::{ProductDetailResponse, ProductListResponse};

/// Invalidation tag applied to every catalog list/detail entry.
pub const CATALOG_CACHE_TAG: &str = "catalog";

/// Env for list/detail TTL seconds (default `30`). `0` disables TTL (until tag invalidate).
pub const CATALOG_CACHE_TTL_SECS_ENV: &str = "RUSTASHOP_CATALOG_CACHE_TTL_SECS";

const DEFAULT_TTL_SECS: u64 = 30;

/// In-process catalog response cache (default CI / local).
///
/// Multi-node: build Serenade `RedisAdapter` (feature `redis`) with the same tag
/// contract and register it in place of [`ArrayAdapter`].
#[derive(Clone)]
pub struct CatalogCache {
    pool: Arc<ArrayAdapter>,
    ttl: Option<Duration>,
}

impl std::fmt::Debug for CatalogCache {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CatalogCache")
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

impl CatalogCache {
    /// Empty pool with TTL from [`CATALOG_CACHE_TTL_SECS_ENV`].
    #[must_use]
    pub fn from_env() -> Self {
        Self::with_ttl(ttl_from_env())
    }

    /// Empty pool with an explicit TTL (`None` = no expiry besides tag invalidate).
    #[must_use]
    pub fn with_ttl(ttl: Option<Duration>) -> Self {
        Self {
            pool: Arc::new(ArrayAdapter::new()),
            ttl,
        }
    }

    /// Cache key for a product list page.
    #[must_use]
    pub fn list_key(limit: u32, offset: u32) -> String {
        format!("products:list:{limit}:{offset}")
    }

    /// Cache key for a product detail.
    #[must_use]
    pub fn detail_key(product_id: &str) -> String {
        format!("products:detail:{product_id}")
    }

    /// Returns a cached list page when present.
    #[must_use]
    pub fn get_list(&self, key: &str) -> Option<Arc<ProductListResponse>> {
        self.get_typed(key)
    }

    /// Stores a list page under `key` with the catalog tag.
    pub fn put_list(&self, key: &str, value: ProductListResponse) {
        self.put_typed(key, value);
    }

    /// Returns a cached product detail when present.
    #[must_use]
    pub fn get_detail(&self, key: &str) -> Option<Arc<ProductDetailResponse>> {
        self.get_typed(key)
    }

    /// Stores a product detail under `key` with the catalog tag.
    pub fn put_detail(&self, key: &str, value: ProductDetailResponse) {
        self.put_typed(key, value);
    }

    /// Drops every entry tagged [`CATALOG_CACHE_TAG`] (call after admin product write).
    ///
    /// # Errors
    ///
    /// Propagates pool invalidation failures.
    pub fn invalidate_catalog(&self) -> Result<usize, serenade_cache::CacheError> {
        self.pool.invalidate_tags(&[CATALOG_CACHE_TAG])
    }

    fn get_typed<T: Clone + Send + Sync + 'static>(&self, key: &str) -> Option<Arc<T>> {
        let item = self.pool.get_item(key).ok()?;
        if !item.is_hit() {
            return None;
        }
        let value = item.get()?.downcast_ref::<T>()?;
        Some(Arc::new(value.clone()))
    }

    fn put_typed<T: Send + Sync + 'static>(&self, key: &str, value: T) {
        let mut item = ArrayCacheItem::miss(key);
        item.set(Arc::new(value));
        item.tag(&[CATALOG_CACHE_TAG]);
        if let Some(ttl) = self.ttl {
            item.expires_after(Some(ttl));
        }
        let _ = self.pool.save(item);
    }
}

fn ttl_from_env() -> Option<Duration> {
    std::env::var(CATALOG_CACHE_TTL_SECS_ENV).map_or_else(
        |_| Some(Duration::from_secs(DEFAULT_TTL_SECS)),
        |raw| match raw.parse::<u64>() {
            Ok(0) => None,
            Ok(secs) => Some(Duration::from_secs(secs)),
            Err(_) => Some(Duration::from_secs(DEFAULT_TTL_SECS)),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_miss_and_tag_invalidation() {
        let cache = CatalogCache::with_ttl(None);
        let key = CatalogCache::list_key(20, 0);
        assert!(cache.get_list(&key).is_none());
        cache.put_list(&key, ProductListResponse { items: Vec::new() });
        assert!(cache.get_list(&key).is_some());
        assert_eq!(cache.invalidate_catalog().expect("invalidate"), 1);
        assert!(cache.get_list(&key).is_none());
    }

    #[test]
    fn detail_round_trip() {
        let cache = CatalogCache::with_ttl(Some(Duration::from_secs(60)));
        let key = CatalogCache::detail_key("p1");
        cache.put_detail(
            &key,
            ProductDetailResponse {
                id: "p1".into(),
                category_id: None,
                slug: "hoodie".into(),
                name: "Hoodie".into(),
                description: None,
                enabled: true,
                variants: Vec::new(),
            },
        );
        let hit = cache.get_detail(&key).expect("hit");
        assert_eq!(hit.slug, "hoodie");
    }

    #[test]
    fn list_key_shape() {
        assert_eq!(CatalogCache::list_key(20, 0), "products:list:20:0");
    }
}
