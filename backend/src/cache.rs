use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct TrackedCache {
    inner: Arc<Cache<String, String>>,
    name: &'static str,
}

impl TrackedCache {
    fn new(cache: Cache<String, String>, name: &'static str) -> Self {
        Self {
            inner: Arc::new(cache),
            name,
        }
    }

    pub async fn get(&self, key: &String) -> Option<String> {
        let result = self.inner.get(key).await;
        if result.is_some() {
            metrics::counter!("llm_cache_hits", "cache" => self.name).increment(1);
        } else {
            metrics::counter!("llm_cache_misses", "cache" => self.name).increment(1);
        }
        result
    }

    pub async fn insert(&self, key: String, value: String) {
        self.inner.insert(key, value).await;
    }

    pub async fn invalidate(&self, key: &String) {
        self.inner.invalidate(key).await;
    }

    pub fn entry_count(&self) -> u64 {
        self.inner.entry_count()
    }
}

#[derive(Clone)]
pub struct LlmCache {
    /// 解读缓存 TTL: 7天
    pub interp: TrackedCache,
    /// 健康评估缓存 TTL: 1天
    pub assess: TrackedCache,
    /// 用药-检验关联缓存 TTL: 12小时
    pub med_lab: TrackedCache,
    /// 名称标准化缓存 TTL: 30天
    pub norm: TrackedCache,
}

impl LlmCache {
    pub fn new() -> Self {
        let interp = TrackedCache::new(
            Cache::builder()
                .max_capacity(1_000)
                .time_to_live(Duration::from_secs(7 * 24 * 3600))
                .build(),
            "interp",
        );
        let assess = TrackedCache::new(
            Cache::builder()
                .max_capacity(500)
                .time_to_live(Duration::from_secs(24 * 3600))
                .build(),
            "assess",
        );
        let med_lab = TrackedCache::new(
            Cache::builder()
                .max_capacity(500)
                .time_to_live(Duration::from_secs(12 * 3600))
                .build(),
            "med_lab",
        );
        let norm = TrackedCache::new(
            Cache::builder()
                .max_capacity(10_000)
                .time_to_live(Duration::from_secs(30 * 24 * 3600))
                .build(),
            "norm",
        );
        Self {
            interp,
            assess,
            med_lab,
            norm,
        }
    }
}
