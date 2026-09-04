use std::io::Cursor;
use std::net::IpAddr;
use std::str::FromStr;
use std::time::SystemTime;

use anyhow::{Context, Result};
use base64::{engine::general_purpose, Engine as _};
use camino::Utf8PathBuf;
use chrono;
use data_url::mime::Mime;
use data_url::DataUrl;
use hyper::{Body, Request, Response, StatusCode};
use image::{DynamicImage, ImageFormat};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Pool, Sqlite};
use tokio::fs;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use torii_processors::fetch::{fetch_content_from_http, fetch_content_from_ipfs};
use torii_sqlite::constants::TOKENS_TABLE;
use tracing::{debug, error, trace};

use super::Handler;

pub(crate) const LOG_TARGET: &str = "torii::server::handlers::static";

fn parse_image_query(query_str: &str) -> ImageQuery {
    let mut height = None;
    let mut width = None;

    for pair in query_str.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            match key {
                "h" | "height" => {
                    if let Ok(h) = value.parse::<u32>() {
                        height = Some(h);
                    }
                }
                "w" | "width" => {
                    if let Ok(w) = value.parse::<u32>() {
                        width = Some(w);
                    }
                }
                _ => {}
            }
        }
    }

    ImageQuery { height, width }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ImageQuery {
    #[serde(alias = "h")]
    height: Option<u32>,
    #[serde(alias = "w")]
    width: Option<u32>,
}

/// The resource requested for a token or contract under `/static`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StaticResource {
    Image,
    Metadata,
}

impl StaticResource {
    fn parse(segment: &str) -> Option<Self> {
        match segment {
            "image" => Some(Self::Image),
            "metadata" => Some(Self::Metadata),
            _ => None,
        }
    }
}

/// Builds a quoted, content-derived ETag from the first 8 bytes of the SHA-256 hash.
fn content_etag(contents: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(contents);
    let hash_bytes = hasher.finalize();
    format!(
        "\"{}\"",
        hash_bytes[..8]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    )
}

fn json_error_response(status: StatusCode, message: &str) -> Response<Body> {
    let body = serde_json::json!({ "error": message }).to_string();
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(body))
        .unwrap()
}

#[derive(Debug)]
pub struct StaticHandler {
    artifacts_dir: Utf8PathBuf,
    pool: Pool<Sqlite>,
}

impl StaticHandler {
    pub fn new(artifacts_dir: Utf8PathBuf, pool: Pool<Sqlite>) -> Self {
        Self {
            artifacts_dir,
            pool,
        }
    }
}

#[async_trait::async_trait]
impl Handler for StaticHandler {
    fn should_handle(&self, req: &Request<Body>) -> bool {
        req.uri().path().starts_with("/static")
    }

    async fn handle(&self, req: Request<Body>, _client_addr: IpAddr) -> Response<Body> {
        let path = req.uri().path();

        // Remove "/static/" prefix to get the actual path
        let path = path.strip_prefix("/static/").unwrap_or("");

        // Parse query parameters
        let query = req.uri().query().unwrap_or("");
        let query = parse_image_query(query);

        match self.serve_static_file(path, query, &req).await {
            Ok(response) => response,
            Err(e) => {
                error!(target: LOG_TARGET, error = ?e, "Failed to serve static file");
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::empty())
                    .unwrap()
            }
        }
    }
}
impl StaticHandler {
    async fn serve_static_file(
        &self,
        path: &str,
        query: ImageQuery,
        req: &Request<Body>,
    ) -> Result<Response<Body>> {
        // Split the path and validate format
        let parts: Vec<&str> = path.split('/').collect();

        // Handle both token format: contract_address/token_id/{image|metadata}
        // and contract format: contract_address/{image|metadata}
        let (contract_address, token_id_part, is_contract, resource) = match parts.len() {
            3 if StaticResource::parse(parts[2]).is_some() => {
                // Token format: contract_address/token_id/{image|metadata}
                if !parts[0].starts_with("0x") || !parts[1].starts_with("0x") {
                    return Ok(Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::empty())
                        .unwrap());
                }
                (
                    parts[0],
                    parts[1],
                    false,
                    StaticResource::parse(parts[2]).unwrap(),
                )
            }
            2 if StaticResource::parse(parts[1]).is_some() => {
                // Contract format: contract_address/{image|metadata}
                if !parts[0].starts_with("0x") {
                    return Ok(Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::empty())
                        .unwrap());
                }
                (parts[0], "", true, StaticResource::parse(parts[1]).unwrap())
            }
            _ => {
                return Ok(Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::empty())
                    .unwrap());
            }
        };

        let token_image_dir = if is_contract {
            self.artifacts_dir.join(contract_address)
        } else {
            self.artifacts_dir
                .join(contract_address)
                .join(token_id_part)
        };

        let token_id = if is_contract {
            contract_address.to_string()
        } else {
            format!("{}:{}", contract_address, token_id_part)
        };

        if resource == StaticResource::Metadata {
            return self.serve_metadata(&token_id, req).await;
        }

        // We'll generate ETag from content hash after reading the file

        // We'll get Last-Modified from actual file metadata (matches content-based ETag approach)

        // Store conditional request headers for later comparison
        let client_etag = req
            .headers()
            .get("if-none-match")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_string());

        let client_modified_since = req
            .headers()
            .get("if-modified-since")
            .and_then(|h| h.to_str().ok())
            .and_then(|s| httpdate::parse_http_date(s).ok());

        // Check if image needs to be refetched based on timestamps
        let should_fetch = if token_image_dir.exists() {
            match self
                .check_if_image_outdated(&token_image_dir, &token_id)
                .await
            {
                Ok(needs_update) => needs_update,
                Err(e) => {
                    error!(target: LOG_TARGET, error = ?e, "Failed to check image timestamps, will attempt to fetch");
                    true
                }
            }
        } else {
            true
        };

        let db_timestamp = match self.get_token_updated_at(&token_id).await {
            Ok(timestamp) => Some(timestamp),
            Err(e) => {
                debug!(target: LOG_TARGET, error = ?e, "Failed to get database timestamp");
                None
            }
        };

        if should_fetch {
            match self
                .fetch_and_process_image(&token_id, db_timestamp.as_deref())
                .await
            {
                Ok(_) => {}
                Err(e) => {
                    error!(target: LOG_TARGET, error = ?e, "Failed to fetch and process image for token_id: {}", token_id);
                    return Ok(Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::empty())
                        .unwrap());
                }
            };
        }

        let file_name = match self.file_name_from_dir_and_query(token_image_dir, &query) {
            Ok(file_name) => file_name,
            Err(e) => {
                error!(target: LOG_TARGET, error = ?e, "Failed to get file name from directory and query");
                return Ok(Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::empty())
                    .unwrap());
            }
        };

        match File::open(&file_name).await {
            Ok(mut file) => {
                let mut contents = vec![];
                if file.read_to_end(&mut contents).await.is_ok() {
                    let mime = mime_guess::from_path(&file_name)
                        .first_or_octet_stream()
                        .to_string();

                    // Generate ETag from content hash
                    let etag = content_etag(&contents);

                    // Check conditional requests now that we have the content ETag
                    if let Some(ref client_etag_str) = client_etag {
                        if client_etag_str == &etag {
                            return Ok(Response::builder()
                                .status(StatusCode::NOT_MODIFIED)
                                .header("etag", etag)
                                .header(
                                    "cache-control",
                                    "public, max-age=3600, stale-while-revalidate=86400",
                                )
                                .body(Body::empty())
                                .unwrap());
                        }
                    }

                    // Get file modification time for Last-Modified header from the file path
                    let file_last_modified = if let Ok(metadata) = std::fs::metadata(&file_name) {
                        metadata.modified().ok()
                    } else {
                        None
                    };

                    // Check If-Modified-Since against file modification time
                    if let (Some(client_time), Some(file_mod_time)) =
                        (client_modified_since, file_last_modified)
                    {
                        let server_time_secs = file_mod_time
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();
                        let client_time_secs = client_time
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs();

                        if server_time_secs <= client_time_secs {
                            return Ok(Response::builder()
                                .status(StatusCode::NOT_MODIFIED)
                                .header("etag", etag)
                                .header("last-modified", httpdate::fmt_http_date(file_mod_time))
                                .header(
                                    "cache-control",
                                    "public, max-age=3600, stale-while-revalidate=86400",
                                )
                                .body(Body::empty())
                                .unwrap());
                        }
                    }

                    // Build response with content-based ETag and file-based Last-Modified
                    let mut response_builder = Response::builder()
                        .header("content-type", mime)
                        .header("etag", etag)
                        .header(
                            "cache-control",
                            "public, max-age=3600, stale-while-revalidate=86400",
                        );

                    // Add Last-Modified header from file modification time
                    if let Some(file_mod_time) = file_last_modified {
                        response_builder = response_builder
                            .header("last-modified", httpdate::fmt_http_date(file_mod_time));
                    }

                    Ok(response_builder.body(Body::from(contents)).unwrap())
                } else {
                    Ok(Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::empty())
                        .unwrap())
                }
            }
            Err(_) => Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::empty())
                .unwrap()),
        }
    }

    /// Serves the token or contract metadata JSON stored in the database.
    ///
    /// Unlike the image route, this does not touch the filesystem: the stored
    /// `metadata` column is returned verbatim, so the response always reflects the
    /// latest indexed state. Clients get a content-based ETag and are asked to
    /// revalidate on every request.
    async fn serve_metadata(&self, token_id: &str, req: &Request<Body>) -> Result<Response<Body>> {
        let query_str = format!("SELECT metadata FROM {TOKENS_TABLE} WHERE id = ?");
        let row = sqlx::query_as::<_, (String,)>(&query_str)
            .bind(token_id)
            .fetch_optional(&self.pool)
            .await
            .context("Failed to fetch metadata from database")?;

        let metadata = match row {
            Some((metadata,)) if !metadata.trim().is_empty() => metadata,
            Some(_) => {
                return Ok(json_error_response(
                    StatusCode::NOT_FOUND,
                    "No metadata stored for token",
                ));
            }
            None => {
                return Ok(json_error_response(
                    StatusCode::NOT_FOUND,
                    "Token not found",
                ));
            }
        };

        if let Err(e) = serde_json::from_str::<serde_json::Value>(&metadata) {
            error!(target: LOG_TARGET, token_id = %token_id, error = ?e, "Stored metadata is not valid JSON");
            return Ok(json_error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Stored metadata is not valid JSON",
            ));
        }

        let etag = content_etag(metadata.as_bytes());

        let client_etag = req
            .headers()
            .get("if-none-match")
            .and_then(|h| h.to_str().ok());
        if client_etag == Some(etag.as_str()) {
            return Ok(Response::builder()
                .status(StatusCode::NOT_MODIFIED)
                .header("etag", etag)
                .header("cache-control", "public, no-cache")
                .body(Body::empty())
                .unwrap());
        }

        Ok(Response::builder()
            .header("content-type", "application/json")
            .header("etag", etag)
            .header("cache-control", "public, no-cache")
            .body(Body::from(metadata))
            .unwrap())
    }

    fn file_name_from_dir_and_query(
        &self,
        token_image_dir: Utf8PathBuf,
        query: &ImageQuery,
    ) -> Result<Utf8PathBuf> {
        let mut entries = std::fs::read_dir(&token_image_dir)
            .ok()
            .into_iter()
            .flatten()
            .flatten();

        // Find the base image (without @medium or @small)
        let base_image = entries
            .find(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .map(|name| name.starts_with("image") && !name.contains('@'))
                    .unwrap_or(false)
            })
            .with_context(|| "Failed to find base image")?;

        let base_filename = base_image.file_name();
        let base_filename = base_filename.to_str().unwrap();
        let base_ext = base_filename.split('.').next_back().unwrap();

        let suffix = match (query.width, query.height) {
            // If either dimension is <= 100px, use small version
            (Some(w), _) if w <= 100 => "@small",
            (_, Some(h)) if h <= 100 => "@small",
            // If either dimension is <= 250px, use medium version
            (Some(w), _) if w <= 250 => "@medium",
            (_, Some(h)) if h <= 250 => "@medium",
            // If no dimensions specified or larger than 250px, use original
            _ => "",
        };

        let target_filename = format!("image{}.{}", suffix, base_ext);
        Ok(token_image_dir.join(target_filename))
    }

    async fn get_token_updated_at(&self, token_id: &str) -> Result<String> {
        // For both tokens and contracts, we can use the same query since contract address is the ID for contracts
        let query_str = format!("SELECT updated_at FROM {TOKENS_TABLE} WHERE id = ?");
        let query_result = sqlx::query_as::<_, (String,)>(&query_str)
            .bind(token_id)
            .fetch_one(&self.pool)
            .await
            .context("Failed to fetch updated_at from database")?;

        Ok(query_result.0)
    }

    async fn get_first_token_metadata(
        &self,
        contract_address: &str,
    ) -> Result<(serde_json::Value, String)> {
        // Find tokens with this contract address that have non-empty metadata
        let pattern = format!("{}:%", contract_address);
        let query_str = format!(
            "SELECT metadata, id FROM {TOKENS_TABLE} WHERE id LIKE ? AND metadata != '' ORDER BY id LIMIT 100"
        );
        let query_results = sqlx::query_as::<_, (String, String)>(&query_str)
            .bind(&pattern)
            .fetch_all(&self.pool)
            .await
            .context("Failed to find any tokens for contract address")?;

        // Try to find a token with valid metadata that contains an image field
        for (metadata_str, token_id) in query_results {
            if let Ok(metadata) = serde_json::from_str::<serde_json::Value>(&metadata_str) {
                if metadata.get("image").is_some() {
                    return Ok((metadata, token_id));
                }
            }
        }

        Err(anyhow::anyhow!(
            "No tokens found with valid image metadata for contract address: {}",
            contract_address
        ))
    }

    async fn check_if_image_outdated(
        &self,
        token_image_dir: &Utf8PathBuf,
        token_id: &str,
    ) -> Result<bool> {
        // Find the base image file in the directory
        let mut entries = match std::fs::read_dir(token_image_dir) {
            Ok(entries) => entries,
            Err(_) => return Ok(true), // Directory doesn't exist, need to fetch
        };

        let base_image_file = entries.find_map(|entry| {
            let entry = entry.ok()?;
            let file_name = entry.file_name();
            let file_name_str = file_name.to_str()?;
            if file_name_str.starts_with("image") && !file_name_str.contains('@') {
                Some(entry.path())
            } else {
                None
            }
        });

        let existing_image_path = match base_image_file {
            Some(path) => path,
            None => return Ok(true), // No existing image, need to fetch
        };

        // Get file modification time
        let file_modified_time = match std::fs::metadata(&existing_image_path) {
            Ok(metadata) => match metadata.modified() {
                Ok(time) => time,
                Err(_) => return Ok(true), // Can't get file time, refetch
            },
            Err(_) => return Ok(true), // Can't read file metadata, refetch
        };

        // Get token updated_at timestamp from database
        let db_timestamp = self.get_token_updated_at(token_id).await?;

        // Parse the database timestamp format: "2025-09-09 11:46:17"
        let db_updated_time = match chrono::NaiveDateTime::parse_from_str(
            &db_timestamp,
            "%Y-%m-%d %H:%M:%S",
        ) {
            Ok(naive_dt) => {
                let timestamp_utc = naive_dt.and_utc();
                SystemTime::from(timestamp_utc)
            }
            Err(_) => {
                // If we can't parse the timestamp, assume we need to refetch
                debug!(target: LOG_TARGET, "Failed to parse updated_at timestamp: {}", db_timestamp);
                return Ok(true);
            }
        };

        // Compare timestamps - refetch if database was updated after file
        let needs_refetch = db_updated_time > file_modified_time;
        Ok(needs_refetch)
    }

    async fn patch_svg_images_regex(&self, svg_data: &[u8]) -> anyhow::Result<Vec<u8>> {
        let svg_str = std::str::from_utf8(svg_data)?;
        // Regex for href and xlink:href in <image ...> tags
        let re = Regex::new(r#"(href|xlink:href)\s*=\s*["']([^"']+)["']"#).unwrap();

        let mut patched_svg = String::with_capacity(svg_str.len());
        let mut last_end = 0;

        for cap in re.captures_iter(svg_str) {
            let m = cap.get(0).unwrap();
            let attr_name = &cap[1];
            let href = &cap[2];

            patched_svg.push_str(&svg_str[last_end..m.start()]);

            // Only patch if not already a data URI
            if href.starts_with("data:") {
                patched_svg.push_str(m.as_str());
            } else {
                // Fetch the image bytes using your fetchers
                let image_bytes = if href.starts_with("http://") || href.starts_with("https://") {
                    fetch_content_from_http(href).await?
                } else if href.starts_with("ipfs://") {
                    let cid = href.strip_prefix("ipfs://").unwrap();
                    fetch_content_from_ipfs(cid).await?
                } else {
                    // fallback: leave as is
                    patched_svg.push_str(m.as_str());
                    last_end = m.end();
                    continue;
                };
                let mime = mime_guess::from_path(href).first_or_octet_stream();
                let b64 = general_purpose::STANDARD.encode(&image_bytes);
                let data_uri = format!("{}=\"data:{};base64,{}\"", attr_name, mime, b64);
                patched_svg.push_str(&data_uri);
            }
            last_end = m.end();
        }
        patched_svg.push_str(&svg_str[last_end..]);
        Ok(patched_svg.into_bytes())
    }

    fn set_file_timestamp(
        &self,
        file_path: &std::path::Path,
        timestamp_str: &str,
    ) -> anyhow::Result<()> {
        use filetime::{set_file_times, FileTime};

        // Parse database timestamp format: "2025-09-09 11:46:17"
        let timestamp = chrono::NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%d %H:%M:%S")
            .context("Failed to parse timestamp")?;

        // Assume UTC timezone for database timestamps
        let timestamp_utc = timestamp.and_utc();
        let system_time = SystemTime::from(timestamp_utc);
        let file_time = FileTime::from_system_time(system_time);

        // Set both access and modification times to the database timestamp
        set_file_times(file_path, file_time, file_time).context("Failed to set file times")?;

        Ok(())
    }

    async fn fetch_and_process_image(
        &self,
        token_id: &str,
        db_timestamp: Option<&str>,
    ) -> anyhow::Result<String> {
        let is_contract = !token_id.contains(':');

        // For both tokens and contracts, we can use the same query since contract address is the ID for contracts
        let query_str = format!("SELECT metadata FROM {TOKENS_TABLE} WHERE id = ?");
        let query_result = sqlx::query_as::<_, (String,)>(&query_str)
            .bind(token_id)
            .fetch_one(&self.pool)
            .await;

        // Try to get metadata and image_uri, with fallback for contracts
        let metadata = match query_result {
            Ok(result) => {
                // Check if metadata is empty or whitespace-only
                let metadata_str = result.0.trim();
                if metadata_str.is_empty() {
                    if is_contract {
                        // Fallback: try to find first token with this contract address
                        debug!(target: LOG_TARGET, contract_address = %token_id, "Empty metadata for contract, searching for first token");
                        let (fallback_metadata, fallback_token_id) =
                            self.get_first_token_metadata(token_id).await?;
                        debug!(target: LOG_TARGET, contract_address = %token_id, fallback_token = %fallback_token_id, "Using fallback token image");
                        fallback_metadata
                    } else {
                        return Err(anyhow::anyhow!("Empty metadata for token"));
                    }
                } else {
                    // Try to parse the metadata
                    match serde_json::from_str::<serde_json::Value>(metadata_str) {
                        Ok(metadata) => {
                            // Check if image field exists
                            if metadata.get("image").is_some() {
                                metadata
                            } else if is_contract {
                                // Fallback: try to find first token with this contract address
                                debug!(target: LOG_TARGET, contract_address = %token_id, "No image found in contract metadata, searching for first token");
                                let (fallback_metadata, fallback_token_id) =
                                    self.get_first_token_metadata(token_id).await?;
                                debug!(target: LOG_TARGET, contract_address = %token_id, fallback_token = %fallback_token_id, "Using fallback token image");
                                fallback_metadata
                            } else {
                                return Err(anyhow::anyhow!("Image URL not found in metadata"));
                            }
                        }
                        Err(e) => {
                            if is_contract {
                                // Fallback: try to find first token with this contract address
                                debug!(target: LOG_TARGET, contract_address = %token_id, error = ?e, "Failed to parse contract metadata, searching for first token");
                                let (fallback_metadata, fallback_token_id) =
                                    self.get_first_token_metadata(token_id).await?;
                                debug!(target: LOG_TARGET, contract_address = %token_id, fallback_token = %fallback_token_id, "Using fallback token image");
                                fallback_metadata
                            } else {
                                return Err(anyhow::anyhow!("Failed to parse metadata: {}", e));
                            }
                        }
                    }
                }
            }
            Err(_) if is_contract => {
                // Fallback: try to find first token with this contract address
                debug!(target: LOG_TARGET, contract_address = %token_id, "Contract metadata not found, searching for first token");
                let (fallback_metadata, fallback_token_id) =
                    self.get_first_token_metadata(token_id).await?;
                debug!(target: LOG_TARGET, contract_address = %token_id, fallback_token = %fallback_token_id, "Using fallback token image");
                fallback_metadata
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Failed to fetch metadata from database: {}",
                    e
                ));
            }
        };

        let image_uri = metadata
            .get("image")
            .context("Image URL not found in metadata")?
            .as_str()
            .context("Image field not a string")?
            .to_string();

        let image_type = match &image_uri {
            uri if uri.starts_with("http") || uri.starts_with("https") => {
                debug!(image_uri = %uri, "Fetching image from http/https URL");
                // Fetch image from HTTP/HTTPS URL
                let response = fetch_content_from_http(uri)
                    .await
                    .context("Failed to fetch image from URL")?;

                // svg files typically start with <svg or <?xml
                if response.starts_with(b"<svg") || response.starts_with(b"<?xml") {
                    ErcImageType::Svg(response.to_vec())
                } else {
                    let format = image::guess_format(&response).with_context(|| {
                        format!(
                            "Unknown file format for token_id: {}, data: {:?}",
                            token_id, &response
                        )
                    })?;
                    ErcImageType::DynamicImage((
                        image::load_from_memory_with_format(&response, format)
                            .context("Failed to load image from bytes")?,
                        format,
                    ))
                }
            }
            uri if uri.starts_with("ipfs") => {
                debug!(image_uri = %uri, "Fetching image from IPFS");
                let cid = uri.strip_prefix("ipfs://").unwrap();
                let response = fetch_content_from_ipfs(cid)
                    .await
                    .context("Failed to read image bytes from IPFS response")?;

                if response.starts_with(b"<svg") || response.starts_with(b"<?xml") {
                    ErcImageType::Svg(response.to_vec())
                } else {
                    let format = image::guess_format(&response).with_context(|| {
                        format!(
                            "Unknown file format for token_id: {}, cid: {}, data: {:?}",
                            token_id, cid, &response
                        )
                    })?;
                    ErcImageType::DynamicImage((
                        image::load_from_memory_with_format(&response, format)
                            .context("Failed to load image from bytes")?,
                        format,
                    ))
                }
            }
            uri if uri.starts_with("data") => {
                debug!("Parsing image from data URI");
                trace!(data_uri = %uri);
                // Parse and decode data URI
                let data_url = DataUrl::process(uri).context("Failed to parse data URI")?;

                // Check if it's an SVG
                if data_url.mime_type() == &Mime::from_str("image/svg+xml").unwrap() {
                    let decoded = data_url
                        .decode_to_vec()
                        .context("Failed to decode data URI")?;
                    ErcImageType::Svg(decoded.0)
                } else {
                    let decoded = data_url
                        .decode_to_vec()
                        .context("Failed to decode data URI")?;
                    let format = image::guess_format(&decoded.0).with_context(|| {
                        format!("Unknown file format for token_id: {}", token_id)
                    })?;
                    ErcImageType::DynamicImage((
                        image::load_from_memory_with_format(&decoded.0, format)
                            .context("Failed to load image from bytes")?,
                        format,
                    ))
                }
            }
            uri => {
                return Err(anyhow::anyhow!("Unsupported URI scheme: {}", uri));
            }
        };

        // Extract contract_address and token_id from token_id
        let (contract_address, token_id_part) = if token_id.contains(':') {
            let parts: Vec<&str> = token_id.split(':').collect();
            if parts.len() != 2 {
                return Err(anyhow::anyhow!(
                    "token_id must be in format contract_address:token_id"
                ));
            }
            (parts[0], parts[1])
        } else {
            // Contract address only
            (token_id, "")
        };

        let dir_path = if token_id_part.is_empty() {
            // Contract case - store in contract_address directory
            self.artifacts_dir.join(contract_address)
        } else {
            // Token case - store in contract_address/token_id directory
            self.artifacts_dir
                .join(contract_address)
                .join(token_id_part)
        };

        // Create directories if they don't exist
        fs::create_dir_all(&dir_path)
            .await
            .context("Failed to create directories for image storage")?;

        // Define base image name
        let base_image_name = "image";

        let relative_path = if token_id_part.is_empty() {
            // Contract case - just contract_address
            Utf8PathBuf::from(contract_address)
        } else {
            // Token case - contract_address/token_id_part
            Utf8PathBuf::new()
                .join(contract_address)
                .join(token_id_part)
        };

        match image_type {
            ErcImageType::DynamicImage((img, format)) => {
                let format_ext = format.extensions_str()[0];

                let target_sizes = [("medium", 250, 250), ("small", 100, 100)];

                // Save original image
                let original_file_name = format!("{}.{}", base_image_name, format_ext);
                let original_file_path = dir_path.join(&original_file_name);
                let mut file = fs::File::create(&original_file_path)
                    .await
                    .with_context(|| format!("Failed to create file: {:?}", original_file_path))?;
                let encoded_image = self
                    .encode_image_to_vec(&img, format)
                    .with_context(|| format!("Failed to encode image: {:?}", original_file_path))?;
                file.write_all(&encoded_image).await.with_context(|| {
                    format!("Failed to write image to file: {:?}", original_file_path)
                })?;

                // Set file timestamp to match database timestamp for outdated check
                if let Some(timestamp) = db_timestamp {
                    if let Err(e) =
                        self.set_file_timestamp(original_file_path.as_std_path(), timestamp)
                    {
                        debug!(target: LOG_TARGET, error = ?e, "Failed to set file timestamp");
                    }
                }

                // Save resized images
                for (label, max_width, max_height) in &target_sizes {
                    let resized_image = self.resize_image_to_fit(&img, *max_width, *max_height);
                    let file_name = format!("@{}.{}", label, format_ext);
                    let file_path = dir_path.join(format!("{}{}", base_image_name, file_name));
                    let mut file = fs::File::create(&file_path)
                        .await
                        .with_context(|| format!("Failed to create file: {:?}", file_path))?;
                    let encoded_image = self
                        .encode_image_to_vec(&resized_image, format)
                        .context("Failed to encode image")?;
                    file.write_all(&encoded_image).await.with_context(|| {
                        format!("Failed to write image to file: {:?}", file_path)
                    })?;

                    // Set file timestamp to match database timestamp for outdated check
                    if let Some(timestamp) = db_timestamp {
                        if let Err(e) = self.set_file_timestamp(file_path.as_std_path(), timestamp)
                        {
                            debug!(target: LOG_TARGET, error = ?e, "Failed to set file timestamp for resized image");
                        }
                    }
                }

                // No need to store hash files anymore - we use timestamp comparison

                Ok(format!("{}/{}", relative_path, base_image_name))
            }
            ErcImageType::Svg(svg_data) => {
                // Patch SVG to embed images
                let patched_svg = self.patch_svg_images_regex(&svg_data).await?;
                let file_name = format!("{}.svg", base_image_name);
                let file_path = dir_path.join(&file_name);
                // Save the patched SVG file
                let mut file = File::create(&file_path)
                    .await
                    .with_context(|| format!("Failed to create file: {:?}", file_path))?;
                file.write_all(&patched_svg)
                    .await
                    .with_context(|| format!("Failed to write SVG to file: {:?}", file_path))?;

                // Set file timestamp to match database timestamp for outdated check
                if let Some(timestamp) = db_timestamp {
                    if let Err(e) = self.set_file_timestamp(file_path.as_std_path(), timestamp) {
                        debug!(target: LOG_TARGET, error = ?e, "Failed to set file timestamp for SVG");
                    }
                }
                Ok(format!("{}/{}", relative_path, file_name))
            }
        }
    }

    fn resize_image_to_fit(
        &self,
        image: &DynamicImage,
        max_width: u32,
        max_height: u32,
    ) -> DynamicImage {
        image.resize_to_fill(max_width, max_height, image::imageops::FilterType::Lanczos3)
    }

    fn encode_image_to_vec(&self, image: &DynamicImage, format: ImageFormat) -> Result<Vec<u8>> {
        let mut buf = Vec::new();
        image
            .write_to(&mut Cursor::new(&mut buf), format)
            .with_context(|| "Failed to encode image")?;
        Ok(buf)
    }
}

#[derive(Debug)]
pub enum ErcImageType {
    DynamicImage((DynamicImage, ImageFormat)),
    Svg(Vec<u8>),
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr};

    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    const CONTRACT: &str = "0xabc";
    const TOKEN: &str = "0x1";
    const CONTRACT_METADATA: &str = r#"{"name":"Collection","image":"ipfs://contract"}"#;
    const TOKEN_METADATA: &str = r#"{"name":"Token #1","image":"ipfs://token","attributes":[{"trait_type":"Rank","value":"S"}]}"#;

    async fn handler() -> StaticHandler {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();

        sqlx::query(&format!(
            "CREATE TABLE {TOKENS_TABLE} (id TEXT PRIMARY KEY, metadata TEXT NOT NULL DEFAULT '', \
             updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP)"
        ))
        .execute(&pool)
        .await
        .unwrap();

        for (id, metadata) in [
            (CONTRACT.to_string(), CONTRACT_METADATA),
            (format!("{CONTRACT}:{TOKEN}"), TOKEN_METADATA),
            (format!("{CONTRACT}:0x2"), ""),
            (format!("{CONTRACT}:0x3"), "not json"),
        ] {
            sqlx::query(&format!(
                "INSERT INTO {TOKENS_TABLE} (id, metadata) VALUES (?, ?)"
            ))
            .bind(id)
            .bind(metadata)
            .execute(&pool)
            .await
            .unwrap();
        }

        let artifacts_dir =
            std::env::temp_dir().join(format!("torii-static-test-{}", uuid::Uuid::new_v4()));
        StaticHandler::new(Utf8PathBuf::from_path_buf(artifacts_dir).unwrap(), pool)
    }

    async fn get(handler: &StaticHandler, path: &str, etag: Option<&str>) -> Response<Body> {
        let mut builder = Request::builder().uri(path);
        if let Some(etag) = etag {
            builder = builder.header("if-none-match", etag);
        }
        let req = builder.body(Body::empty()).unwrap();
        handler.handle(req, IpAddr::V4(Ipv4Addr::LOCALHOST)).await
    }

    async fn body_string(response: Response<Body>) -> String {
        let bytes = hyper::body::to_bytes(response.into_body()).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn serves_token_metadata_from_database() {
        let handler = handler().await;
        let response = get(
            &handler,
            &format!("/static/{CONTRACT}/{TOKEN}/metadata"),
            None,
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "application/json"
        );
        assert!(response.headers().contains_key("etag"));
        assert_eq!(body_string(response).await, TOKEN_METADATA);
    }

    #[tokio::test]
    async fn serves_contract_metadata_from_database() {
        let handler = handler().await;
        let response = get(&handler, &format!("/static/{CONTRACT}/metadata"), None).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(body_string(response).await, CONTRACT_METADATA);
    }

    #[tokio::test]
    async fn returns_not_modified_when_etag_matches() {
        let handler = handler().await;
        let path = format!("/static/{CONTRACT}/{TOKEN}/metadata");
        let first = get(&handler, &path, None).await;
        let etag = first
            .headers()
            .get("etag")
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        let second = get(&handler, &path, Some(&etag)).await;
        assert_eq!(second.status(), StatusCode::NOT_MODIFIED);
        assert_eq!(second.headers().get("etag").unwrap(), etag.as_str());
        assert!(body_string(second).await.is_empty());
    }

    #[tokio::test]
    async fn returns_not_found_for_unknown_token() {
        let handler = handler().await;
        let response = get(
            &handler,
            &format!("/static/{CONTRACT}/0x999/metadata"),
            None,
        )
        .await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            body_string(response).await,
            r#"{"error":"Token not found"}"#
        );
    }

    #[tokio::test]
    async fn returns_not_found_for_empty_metadata() {
        let handler = handler().await;
        let response = get(&handler, &format!("/static/{CONTRACT}/0x2/metadata"), None).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            body_string(response).await,
            r#"{"error":"No metadata stored for token"}"#
        );
    }

    #[tokio::test]
    async fn returns_server_error_for_invalid_json_metadata() {
        let handler = handler().await;
        let response = get(&handler, &format!("/static/{CONTRACT}/0x3/metadata"), None).await;

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn rejects_unknown_resource_and_malformed_ids() {
        let handler = handler().await;

        let unknown = get(&handler, &format!("/static/{CONTRACT}/{TOKEN}/owner"), None).await;
        assert_eq!(unknown.status(), StatusCode::NOT_FOUND);

        let bad_contract = get(&handler, "/static/abc/metadata", None).await;
        assert_eq!(bad_contract.status(), StatusCode::NOT_FOUND);

        let bad_token = get(&handler, &format!("/static/{CONTRACT}/1/metadata"), None).await;
        assert_eq!(bad_token.status(), StatusCode::NOT_FOUND);
    }
}
