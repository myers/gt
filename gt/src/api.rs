use clap::Args;
use eyre::Result;

use crate::config::Config;
use crate::repo;

#[derive(Args)]
pub struct ApiCommand {
    /// API endpoint path (e.g., repos/{owner}/{repo}/issues)
    endpoint: String,

    /// HTTP method
    #[arg(short = 'X', long = "method")]
    method: Option<String>,

    /// Add a string parameter (key=value)
    #[arg(short = 'f', long = "raw-field", value_name = "KEY=VALUE")]
    raw_fields: Vec<String>,

    /// Add a typed parameter (key=value). Values "true", "false", "null",
    /// and integers are converted to their JSON types. @file reads from file.
    #[arg(short = 'F', long = "field", value_name = "KEY=VALUE")]
    fields: Vec<String>,

    /// Add a request header (key:value)
    #[arg(short = 'H', long = "header", value_name = "KEY:VALUE")]
    headers: Vec<String>,

    /// Include HTTP response headers in output
    #[arg(short = 'i', long = "include")]
    include: bool,

    /// Silence response body
    #[arg(long)]
    silent: bool,

    /// Filter JSON output with a jq expression
    #[arg(long = "jq", value_name = "EXPR")]
    jq_expr: Option<String>,

    /// Make additional requests to fetch all pages
    #[arg(long)]
    paginate: bool,
}

impl ApiCommand {
    pub async fn run(&self) -> Result<()> {
        let config = Config::load()?;
        let api = config.client()?;

        // Expand {owner} and {repo} placeholders
        let endpoint = self.expand_placeholders(&config)?;

        // Build the full URL using the API crate
        let url: url::Url = api.url_for(&endpoint).parse()
            .map_err(|e| eyre::eyre!("Invalid URL: {e}"))?;

        // Determine method
        let has_body_fields = !self.raw_fields.is_empty() || !self.fields.is_empty();
        let method = match &self.method {
            Some(m) => m.to_uppercase(),
            None => {
                if has_body_fields {
                    "POST".to_string()
                } else {
                    "GET".to_string()
                }
            }
        };

        // Build request body
        let body = if has_body_fields {
            Some(self.build_body()?)
        } else {
            None
        };

        if self.paginate {
            self.run_paginated(&api, &url, &method, &body).await
        } else {
            self.run_single(&api, &url, &method, &body).await
        }
    }

    fn expand_placeholders(&self, config: &Config) -> Result<String> {
        let mut endpoint = self.endpoint.clone();
        if endpoint.contains("{owner}") || endpoint.contains("{repo}") {
            let repo_info = repo::resolve_repo(None, &config.url)?;
            endpoint = endpoint.replace("{owner}", &repo_info.owner);
            endpoint = endpoint.replace("{repo}", &repo_info.name);
        }
        Ok(endpoint)
    }

    fn build_body(&self) -> Result<serde_json::Value> {
        let mut map = serde_json::Map::new();

        for field in &self.raw_fields {
            let (key, value) = parse_key_value(field)?;
            map.insert(key, serde_json::Value::String(value));
        }

        for field in &self.fields {
            let (key, value) = parse_key_value(field)?;
            let typed_value = parse_typed_value(&value)?;
            map.insert(key, typed_value);
        }

        Ok(serde_json::Value::Object(map))
    }

    async fn run_single(
        &self,
        api: &gitea_api::Gitea,
        url: &url::Url,
        method: &str,
        body: &Option<serde_json::Value>,
    ) -> Result<()> {
        let resp = self.send_request(api, url, method, body).await?;

        if self.include {
            println!("{} {}", resp.status().as_u16(), resp.status().canonical_reason().unwrap_or(""));
            for (key, value) in resp.headers() {
                println!("{}: {}", key, value.to_str().unwrap_or(""));
            }
            println!();
        }

        if !self.silent {
            let text = resp.text().await?;
            self.output(&text)?;
        }

        Ok(())
    }

    async fn run_paginated(
        &self,
        api: &gitea_api::Gitea,
        base_url: &url::Url,
        method: &str,
        body: &Option<serde_json::Value>,
    ) -> Result<()> {
        let mut page = 1u32;
        loop {
            let mut url = base_url.clone();
            url.query_pairs_mut().append_pair("page", &page.to_string());
            if !url.query_pairs().any(|(k, _)| k == "limit") {
                url.query_pairs_mut().append_pair("limit", "50");
            }

            let resp = self.send_request(api, &url, method, body).await?;

            let text = resp.text().await?;
            if !self.silent {
                self.output(&text)?;
            }

            // Check if there's more data
            let parsed: serde_json::Value = serde_json::from_str(&text).unwrap_or_default();
            if let Some(arr) = parsed.as_array() {
                if arr.is_empty() {
                    break;
                }
            } else {
                break; // Non-array response, no pagination
            }

            page += 1;
        }

        Ok(())
    }

    async fn send_request(
        &self,
        api: &gitea_api::Gitea,
        url: &url::Url,
        method: &str,
        body: &Option<serde_json::Value>,
    ) -> Result<gitea_api::Response> {
        // Parse custom headers
        let mut headers = Vec::new();
        for h in &self.headers {
            let (key, value) = h
                .split_once(':')
                .ok_or_else(|| eyre::eyre!("Invalid header format: {h}. Use key:value"))?;
            headers.push((key.trim().to_string(), value.trim().to_string()));
        }

        let method = method.parse::<gitea_api::Method>()
            .map_err(|_| eyre::eyre!("Unsupported HTTP method: {method}"))?;

        let resp = api
            .request(method, url.as_str(), &headers, body.as_ref())
            .await
            .map_err(|e| eyre::eyre!("{e}"))?;

        if !resp.status().is_success() && !self.include {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            eyre::bail!("{} {}\n{}", status.as_u16(), status.canonical_reason().unwrap_or(""), text);
        }

        Ok(resp)
    }

    fn output(&self, text: &str) -> Result<()> {
        if let Some(ref expr) = self.jq_expr {
            // Simple jq-like field extraction: .field or .[].field
            let parsed: serde_json::Value = serde_json::from_str(text)?;
            let results = jq_select(&parsed, expr)?;
            for r in results {
                match r {
                    serde_json::Value::String(s) => println!("{s}"),
                    other => println!("{}", serde_json::to_string_pretty(&other)?),
                }
            }
        } else {
            // Pretty-print JSON, or raw if not JSON
            match serde_json::from_str::<serde_json::Value>(text) {
                Ok(v) => println!("{}", serde_json::to_string_pretty(&v)?),
                Err(_) => print!("{text}"),
            }
        }
        Ok(())
    }
}

fn parse_key_value(s: &str) -> Result<(String, String)> {
    let (key, value) = s
        .split_once('=')
        .ok_or_else(|| eyre::eyre!("Invalid field format: {s}. Use key=value"))?;
    Ok((key.to_string(), value.to_string()))
}

fn parse_typed_value(s: &str) -> Result<serde_json::Value> {
    // @file reads from file
    if let Some(path) = s.strip_prefix('@') {
        if path == "-" {
            let content = std::io::read_to_string(std::io::stdin())?;
            return Ok(serde_json::Value::String(content));
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| eyre::eyre!("Failed to read {path}: {e}"))?;
        return Ok(serde_json::Value::String(content));
    }

    // Boolean/null literals
    match s {
        "true" => return Ok(serde_json::Value::Bool(true)),
        "false" => return Ok(serde_json::Value::Bool(false)),
        "null" => return Ok(serde_json::Value::Null),
        _ => {}
    }

    // Integer
    if let Ok(n) = s.parse::<i64>() {
        return Ok(serde_json::json!(n));
    }

    // Fall back to string
    Ok(serde_json::Value::String(s.to_string()))
}

/// Simple jq-like field selector.
/// Supports: .field, .[].field, .field.nested, .[].field.nested
fn jq_select(value: &serde_json::Value, expr: &str) -> Result<Vec<serde_json::Value>> {
    let expr = expr.trim_start_matches('.');
    if expr.is_empty() {
        return Ok(vec![value.clone()]);
    }

    let parts: Vec<&str> = expr.splitn(2, '.').collect();
    let (head, rest) = (parts[0], parts.get(1).copied());

    if head == "[]" {
        // Array iteration
        if let Some(arr) = value.as_array() {
            let mut results = Vec::new();
            for item in arr {
                if let Some(rest) = rest {
                    results.extend(jq_select(item, &format!(".{rest}"))?);
                } else {
                    results.push(item.clone());
                }
            }
            return Ok(results);
        }
        return Ok(vec![]);
    }

    // Object field access
    if let Some(obj) = value.as_object() {
        if let Some(field_value) = obj.get(head) {
            if let Some(rest) = rest {
                return jq_select(field_value, &format!(".{rest}"));
            }
            return Ok(vec![field_value.clone()]);
        }
    }

    Ok(vec![])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_key_value() {
        let (k, v) = parse_key_value("name=hello").unwrap();
        assert_eq!(k, "name");
        assert_eq!(v, "hello");

        assert!(parse_key_value("noeq").is_err());
    }

    #[test]
    fn test_parse_typed_value() {
        assert_eq!(parse_typed_value("true").unwrap(), serde_json::json!(true));
        assert_eq!(parse_typed_value("false").unwrap(), serde_json::json!(false));
        assert_eq!(parse_typed_value("null").unwrap(), serde_json::json!(null));
        assert_eq!(parse_typed_value("42").unwrap(), serde_json::json!(42));
        assert_eq!(
            parse_typed_value("hello").unwrap(),
            serde_json::json!("hello")
        );
    }

    #[test]
    fn test_jq_select_field() {
        let v = serde_json::json!({"name": "test", "id": 1});
        let r = jq_select(&v, ".name").unwrap();
        assert_eq!(r, vec![serde_json::json!("test")]);
    }

    #[test]
    fn test_jq_select_array() {
        let v = serde_json::json!([{"name": "a"}, {"name": "b"}]);
        let r = jq_select(&v, ".[].name").unwrap();
        assert_eq!(r, vec![serde_json::json!("a"), serde_json::json!("b")]);
    }

    #[test]
    fn test_jq_select_nested() {
        let v = serde_json::json!({"repo": {"name": "test"}});
        let r = jq_select(&v, ".repo.name").unwrap();
        assert_eq!(r, vec![serde_json::json!("test")]);
    }

    #[test]
    fn test_jq_select_identity() {
        let v = serde_json::json!({"a": 1});
        let r = jq_select(&v, ".").unwrap();
        assert_eq!(r, vec![v]);
    }
}
