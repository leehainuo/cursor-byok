//! Parses and validates call-scoped plugin resource origins.
use url::Url;

use crate::{Error, Result};

pub fn normalize_origin(value: &str) -> Result<String> {
    let trimmed = value.trim().trim_end_matches('/');
    let url = Url::parse(trimmed)
        .map_err(|error| Error::Config(format!("invalid plugin resource origin: {error}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::Config(
            "plugin resource origin must be http or https".into(),
        ));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::Config(
            "plugin resource origin must not contain credentials".into(),
        ));
    }
    if url.host_str().is_none() {
        return Err(Error::Config("plugin resource origin has no host".into()));
    }
    if url.path() != "/" && !url.path().is_empty() {
        return Err(Error::Config(
            "plugin resource origin must not include a path".into(),
        ));
    }
    if url.query().is_some() || url.fragment().is_some() {
        return Err(Error::Config(
            "plugin resource origin must not include a query or fragment".into(),
        ));
    }
    Ok(url.origin().ascii_serialization())
}

pub fn origin_from_private_data(value: &serde_json::Value) -> Result<String> {
    let origin = value
        .get("origin")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| Error::Config("plugin resource privateData.origin is required".into()))?;
    normalize_origin(origin)
}

pub fn rewrite_private_origin(value: &mut serde_json::Value, origin: String) {
    match value.as_object_mut() {
        Some(object) => {
            object.insert("origin".into(), serde_json::Value::String(origin));
        }
        None => {
            *value = serde_json::json!({ "origin": origin });
        }
    }
}

pub fn origin_from_url(value: &str) -> Result<String> {
    let url = Url::parse(value.trim())
        .map_err(|error| Error::Config(format!("invalid plugin network URL: {error}")))?;
    request_origin(&url)
}

pub fn request_origin(url: &Url) -> Result<String> {
    if !url.username().is_empty() || url.password().is_some() {
        return Err(Error::Config(
            "plugin network URL must not contain credentials".into(),
        ));
    }
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::Config(
            "plugin network URL must be HTTP or HTTPS without credentials".into(),
        ));
    }
    Ok(url.origin().ascii_serialization())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_http_origins_and_strips_trailing_slashes() {
        assert_eq!(
            normalize_origin("http://127.0.0.1:3425/").unwrap(),
            "http://127.0.0.1:3425"
        );
        assert_eq!(
            normalize_origin("https://gateway.example").unwrap(),
            "https://gateway.example"
        );
        assert_eq!(
            normalize_origin("http://192.168.1.2:3425").unwrap(),
            "http://192.168.1.2:3425"
        );
    }

    #[test]
    fn rejects_credentials_paths_and_non_http_origins() {
        assert!(normalize_origin("http://user:pass@127.0.0.1:3425").is_err());
        assert!(normalize_origin("http://127.0.0.1:3425/v1").is_err());
        assert!(normalize_origin("ftp://127.0.0.1:3425").is_err());
        assert!(normalize_origin("https://example.com/path?q=1").is_err());
    }

    #[test]
    fn origin_from_url_keeps_scheme_host_port_and_allows_paths() {
        assert_eq!(
            origin_from_url("http://127.0.0.1:3425/v1/models").unwrap(),
            "http://127.0.0.1:3425"
        );
        assert_eq!(
            origin_from_url("https://gateway.example/v1").unwrap(),
            "https://gateway.example"
        );
        assert!(origin_from_url("http://user:pass@127.0.0.1:3425").is_err());
    }

    #[test]
    fn rewrites_private_origin_and_rejects_missing_origin() {
        let mut private_data =
            serde_json::json!({ "origin": "http://127.0.0.1:3425/", "apiKey": "k" });
        let origin = origin_from_private_data(&private_data).unwrap();
        rewrite_private_origin(&mut private_data, origin);
        assert_eq!(private_data["origin"], "http://127.0.0.1:3425");
        assert!(origin_from_private_data(&serde_json::json!({})).is_err());
    }
}
