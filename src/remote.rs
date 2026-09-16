use std::sync::Arc;

use anyhow::{Context, Result, bail};
use aws_config::Region;
use aws_credential_types::Credentials;
use aws_sdk_s3::{Client, config::Builder};
use aws_smithy_http_client::tls::{Provider, rustls_provider::CryptoMode};

use crate::config::RemoteConfig;

#[derive(Clone)]
pub struct Remote {
    pub name: String,
    pub cfg: RemoteConfig,
    client: Arc<Client>,
}

#[derive(Clone, Debug)]
pub struct RemoteEntry {
    pub name: String,
    pub key: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: String,
}

impl Remote {
    pub async fn connect(name: String, cfg: RemoteConfig) -> Result<Self> {
        let region = Region::new(cfg.region.clone().unwrap_or_else(|| "auto".into()));
        // aws-lc 대신 ring을 쓴다. 순수 Rust에 가까워 Haiku 등 교차 빌드가 쉽다.
        let http_client = aws_smithy_http_client::Builder::new()
            .tls_provider(Provider::Rustls(CryptoMode::Ring))
            .build_https();
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(region.clone())
            .http_client(http_client);
        if let (Some(id), Some(secret)) = (&cfg.access_key_id, &cfg.secret_access_key) {
            loader = loader.credentials_provider(Credentials::new(id, secret, None, None, "mdir"));
        }
        let shared = loader.load().await;
        let mut builder = Builder::from(&shared).region(region);
        if let Some(endpoint) = &cfg.endpoint {
            builder = builder.endpoint_url(endpoint);
        }
        if cfg.path_style || cfg.kind.eq_ignore_ascii_case("r2") {
            builder = builder.force_path_style(true);
        }
        Ok(Self {
            name,
            cfg,
            client: Arc::new(Client::from_conf(builder.build())),
        })
    }

    pub async fn buckets(&self) -> Result<Vec<RemoteEntry>> {
        if let Some(bucket) = &self.cfg.bucket {
            return Ok(vec![RemoteEntry {
                name: bucket.clone(),
                key: bucket.clone(),
                is_dir: true,
                size: 0,
                modified: String::new(),
            }]);
        }
        let out = self
            .client
            .list_buckets()
            .send()
            .await
            .context("버킷 목록 조회 실패")?;
        Ok(out
            .buckets()
            .iter()
            .filter_map(|b| b.name())
            .map(|name| RemoteEntry {
                name: name.into(),
                key: name.into(),
                is_dir: true,
                size: 0,
                modified: String::new(),
            })
            .collect())
    }

    pub async fn list(&self, bucket: &str, prefix: &str) -> Result<Vec<RemoteEntry>> {
        let out = self
            .client
            .list_objects_v2()
            .bucket(bucket)
            .prefix(prefix)
            .delimiter("/")
            .send()
            .await
            .with_context(|| format!("s3://{bucket}/{prefix} 조회 실패"))?;
        let mut entries = Vec::new();
        for p in out.common_prefixes() {
            if let Some(key) = p.prefix() {
                let name = key.trim_end_matches('/').rsplit('/').next().unwrap_or(key);
                entries.push(RemoteEntry {
                    name: name.into(),
                    key: key.into(),
                    is_dir: true,
                    size: 0,
                    modified: String::new(),
                });
            }
        }
        for o in out.contents() {
            let Some(key) = o.key() else { continue };
            if key == prefix {
                continue;
            }
            entries.push(RemoteEntry {
                name: key.rsplit('/').next().unwrap_or(key).into(),
                key: key.into(),
                is_dir: false,
                size: o.size().unwrap_or(0).max(0) as u64,
                modified: o.last_modified().map(|d| d.to_string()).unwrap_or_default(),
            });
        }
        entries.sort_by_key(|e| (!e.is_dir, e.name.to_lowercase()));
        Ok(entries)
    }

    pub async fn download(&self, bucket: &str, key: &str, path: &std::path::Path) -> Result<()> {
        if path.exists() {
            bail!("대상 파일이 이미 있습니다: {}", path.display());
        }
        let out = self
            .client
            .get_object()
            .bucket(bucket)
            .key(key)
            .send()
            .await
            .context("다운로드 실패")?;
        let bytes = out
            .body
            .collect()
            .await
            .context("다운로드 데이터 읽기 실패")?
            .into_bytes();
        tokio::fs::write(path, bytes)
            .await
            .context("로컬 파일 저장 실패")
    }
}
