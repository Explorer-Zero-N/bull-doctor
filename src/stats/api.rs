use axum::extract::State;
use axum::response::{Html, IntoResponse, Json};
use std::collections::HashMap;

use crate::proxy::ProxyState;

const STATS_HTML: &str = include_str!("page.html");

pub async fn stats_page() -> Html<&'static str> {
    Html(STATS_HTML)
}

pub async fn stats_bootstrap(State(state): State<ProxyState>) -> impl IntoResponse {
    let entries = state.request_log.list().await;
    let summary = state.request_log.summary().await;

    // 按小时聚合数据
    let hourly_data = aggregate_by_hour(&entries);

    // 获取当前提供商信息
    let config = state.config.read().await;
    let active_provider = config.active_provider()
        .map(|p| p.name.clone())
        .unwrap_or_else(|_| "unknown".into());

    Json(serde_json::json!({
        "summary": {
            "count": summary.count,
            "total_input_tokens": summary.total_input_tokens,
            "total_output_tokens": summary.total_output_tokens,
            "total_cost_yuan": summary.total_cost_yuan,
            "success_rate": if summary.count > 0 {
                (entries.iter().filter(|e| e.ok).count() as f64 / summary.count as f64) * 100.0
            } else {
                0.0
            },
            "avg_duration_ms": if summary.count > 0 {
                entries.iter().map(|e| e.duration_ms).sum::<u64>() as f64 / summary.count as f64
            } else {
                0.0
            }
        },
        "entries": entries.into_iter().take(100).collect::<Vec<_>>(),
        "hourlyData": hourly_data,
        "provider": active_provider
    }))
}

fn aggregate_by_hour(entries: &[crate::request_log::RequestLogEntry]) -> Vec<serde_json::Value> {
    let mut hourly_counts: HashMap<u32, u32> = HashMap::new();
    let now = std::time::SystemTime::now();
    let one_hour_ago = now - std::time::Duration::from_secs(3600);

    if let Ok(one_hour_ago_ms) = one_hour_ago.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64) {
        for entry in entries {
            let entry_time_ms = entry.time_ms as u64;
            if entry_time_ms >= one_hour_ago_ms {
                let hour = entry_time_ms / (60 * 60 * 1000);
                *hourly_counts.entry(hour as u32).or_insert(0) += 1;
            }
        }
    }

    // 生成最近24小时的数据
    let mut result = Vec::new();
    let current_hour = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64 / (60 * 60 * 1000);

    for i in 0..24 {
        let hour = (current_hour - (23 - i)) as u32;
        let count = hourly_counts.get(&hour).unwrap_or(&0);
        result.push(serde_json::json!({
            "hour": format!("{:02}:00", (hour % 24) + 8), // 转换为北京时间
            "count": count
        }));
    }

    result
}