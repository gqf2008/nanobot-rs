//! Web 搜索工具 - 使用 Brave Search API

use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use super::{Tool, ToolContext, ToolDef, ToolResult};

/// Web 搜索工具
pub struct WebSearchTool {
    api_key: String,
}

impl WebSearchTool {
    pub fn new(api_key: String) -> Self {
        Self { api_key }
    }

    async fn search(&self,
        query: &str,
        count: u32,
        country: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        let client = reqwest::Client::new();
        
        let mut url = reqwest::Url::parse("https://api.search.brave.com/res/v1/web/search")?;
        url.query_pairs_mut()
            .append_pair("q", query)
            .append_pair("count", &count.to_string());
        
        if let Some(c) = country {
            url.query_pairs_mut().append_pair("country", c);
        }

        let response = client
            .get(url)
            .header("Accept", "application/json")
            .header("X-Subscription-Token", &self.api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!("搜索 API 错误: {} - {}", status, text));
        }

        let search_response: BraveSearchResponse = response.json().await?;
        
        let results = search_response.web.results.into_iter()
            .map(|r| SearchResult {
                title: r.title,
                url: r.url,
                description: r.description,
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl Tool for WebSearchTool {
    fn definition(&self) -> &ToolDef {
        lazy_static::lazy_static! {
            static ref DEF: ToolDef = ToolDef {
                name: "web_search".to_string(),
                description: "使用 Brave Search 搜索网页信息".to_string(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "搜索查询词"
                        },
                        "count": {
                            "type": "integer",
                            "description": "返回结果数量（1-10），默认 5",
                            "default": 5
                        }
                    },
                    "required": ["query"]
                }),
            };
        }
        &DEF
    }

    async fn execute(&self, args: Value, _ctx: &ToolContext) -> Result<ToolResult> {
        let query = args.get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::anyhow!("缺少 query 参数"))?;

        let count = args.get("count")
            .and_then(|v| v.as_u64())
            .map(|c| c.min(10).max(1) as u32)
            .unwrap_or(5);

        match self.search(query, count, Some("US")).await {
            Ok(results) => {
                if results.is_empty() {
                    Ok(ToolResult::success("未找到相关结果".to_string()))
                } else {
                    let output = results.iter()
                        .enumerate()
                        .map(|(i, r)| format!(
                            "{}. {}\n   URL: {}\n   {}\n",
                            i + 1, r.title, r.url, r.description
                        ))
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(ToolResult::success(output))
                }
            }
            Err(e) => Ok(ToolResult::error(format!("搜索失败: {}", e))),
        }
    }
}

#[derive(Debug)]
struct SearchResult {
    title: String,
    url: String,
    description: String,
}

// Brave Search API 响应结构
#[derive(Debug, Deserialize)]
struct BraveSearchResponse {
    web: WebResults,
}

#[derive(Debug, Deserialize)]
struct WebResults {
    results: Vec<WebResult>,
}

#[derive(Debug, Deserialize)]
struct WebResult {
    title: String,
    url: String,
    description: String,
}
