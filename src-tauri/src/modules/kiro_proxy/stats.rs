use std::collections::{HashMap, VecDeque};

use crate::modules::config::{
    KiroProxyApiKeyUsageDaily, KiroProxyApiKeyUsageModel, KiroProxyConfig,
};

use super::types::{
    ProxyAdminStatsResponse, ProxyAggregateStats, ProxyDailyStats, ProxyModelStats, ProxyRequestLog,
    ProxyStatus, ProxyUsageRecord,
};

#[derive(Debug, Clone)]
pub struct StatsStore {
    pub aggregate: ProxyAggregateStats,
    logs: VecDeque<ProxyRequestLog>,
    max_logs: usize,
}

impl Default for StatsStore {
    fn default() -> Self {
        Self {
            aggregate: ProxyAggregateStats::default(),
            logs: VecDeque::new(),
            max_logs: 2000,
        }
    }
}

impl StatsStore {
    pub fn from_state(aggregate: ProxyAggregateStats, logs: Vec<ProxyRequestLog>) -> Self {
        let mut deque = VecDeque::from(logs);
        while deque.len() > 2000 {
            deque.pop_front();
        }
        Self {
            aggregate,
            logs: deque,
            max_logs: 2000,
        }
    }

    pub fn record(&mut self, log: ProxyRequestLog) {
        self.aggregate.total_requests = self.aggregate.total_requests.saturating_add(1);
        if log.success {
            self.aggregate.success_requests = self.aggregate.success_requests.saturating_add(1);
        } else {
            self.aggregate.failed_requests = self.aggregate.failed_requests.saturating_add(1);
        }

        self.aggregate.total_input_tokens = self
            .aggregate
            .total_input_tokens
            .saturating_add(log.input_tokens);
        self.aggregate.total_output_tokens = self
            .aggregate
            .total_output_tokens
            .saturating_add(log.output_tokens);
        self.aggregate.total_credits += log.credits;

        if let Some(model) = log.model.clone() {
            let entry = self
                .aggregate
                .by_model
                .entry(model)
                .or_insert_with(ProxyModelStats::default);
            entry.requests = entry.requests.saturating_add(1);
            entry.input_tokens = entry.input_tokens.saturating_add(log.input_tokens);
            entry.output_tokens = entry.output_tokens.saturating_add(log.output_tokens);
            entry.credits += log.credits;
        }

        let day_key = chrono::DateTime::<chrono::Utc>::from_timestamp(log.timestamp, 0)
            .unwrap_or_else(chrono::Utc::now)
            .format("%Y-%m-%d")
            .to_string();
        let daily = self
            .aggregate
            .daily
            .entry(day_key)
            .or_insert_with(ProxyDailyStats::default);
        daily.requests = daily.requests.saturating_add(1);
        daily.input_tokens = daily.input_tokens.saturating_add(log.input_tokens);
        daily.output_tokens = daily.output_tokens.saturating_add(log.output_tokens);
        daily.credits += log.credits;

        self.logs.push_back(log);
        while self.logs.len() > self.max_logs {
            self.logs.pop_front();
        }
    }

    pub fn reset(&mut self) {
        self.aggregate = ProxyAggregateStats::default();
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }

    pub fn logs(&self, limit: Option<usize>) -> Vec<ProxyRequestLog> {
        let limit = limit.unwrap_or(200);
        let len = self.logs.len();
        let start = len.saturating_sub(limit);
        self.logs.iter().skip(start).cloned().collect()
    }

    pub fn all_logs(&self) -> Vec<ProxyRequestLog> {
        self.logs.iter().cloned().collect()
    }

    pub fn snapshot(
        &self,
        mut status: ProxyStatus,
        accounts: Vec<super::types::ProxyAccountView>,
    ) -> ProxyAdminStatsResponse {
        status.request_count = self.aggregate.total_requests;
        status.success_count = self.aggregate.success_requests;
        status.failed_count = self.aggregate.failed_requests;
        status.total_input_tokens = self.aggregate.total_input_tokens;
        status.total_output_tokens = self.aggregate.total_output_tokens;
        status.total_credits = self.aggregate.total_credits;

        ProxyAdminStatsResponse {
            status,
            aggregate: self.aggregate.clone(),
            accounts,
        }
    }

    pub fn aggregate(&self) -> ProxyAggregateStats {
        self.aggregate.clone()
    }
}

pub fn record_api_key_usage(
    config: &mut KiroProxyConfig,
    api_key_id: &str,
    credits: f64,
    input_tokens: u64,
    output_tokens: u64,
    model: Option<&str>,
    path: &str,
) {
    let Some(api_key) = config.api_keys.iter_mut().find(|item| item.id == api_key_id) else {
        return;
    };

    let now = chrono::Utc::now().timestamp();
    let day_key = chrono::Utc::now().format("%Y-%m-%d").to_string();

    api_key.last_used_at = Some(now);
    api_key.usage.total_requests = api_key.usage.total_requests.saturating_add(1);
    api_key.usage.total_input_tokens = api_key.usage.total_input_tokens.saturating_add(input_tokens);
    api_key.usage.total_output_tokens = api_key.usage.total_output_tokens.saturating_add(output_tokens);
    api_key.usage.total_credits += credits;

    let daily = api_key
        .usage
        .daily
        .entry(day_key)
        .or_insert_with(KiroProxyApiKeyUsageDaily::default);
    daily.requests = daily.requests.saturating_add(1);
    daily.input_tokens = daily.input_tokens.saturating_add(input_tokens);
    daily.output_tokens = daily.output_tokens.saturating_add(output_tokens);
    daily.credits += credits;

    if let Some(model_name) = model {
        let by_model = api_key
            .usage
            .by_model
            .entry(model_name.to_string())
            .or_insert_with(KiroProxyApiKeyUsageModel::default);
        by_model.requests = by_model.requests.saturating_add(1);
        by_model.input_tokens = by_model.input_tokens.saturating_add(input_tokens);
        by_model.output_tokens = by_model.output_tokens.saturating_add(output_tokens);
        by_model.credits += credits;
    }

    api_key.usage_history.insert(
        0,
        ProxyUsageRecord {
            timestamp: now,
            model: model.unwrap_or("unknown").to_string(),
            input_tokens,
            output_tokens,
            credits,
            path: path.to_string(),
        },
    );

    if api_key.usage_history.len() > 100 {
        api_key.usage_history.truncate(100);
    }
}

pub fn api_key_views(config: &KiroProxyConfig) -> Vec<super::types::ProxyApiKeyView> {
    config
        .api_keys
        .iter()
        .map(|key| {
            let preview = if key.key.len() <= 8 {
                "***".to_string()
            } else {
                format!("{}***{}", &key.key[..4], &key.key[key.key.len() - 4..])
            };

            super::types::ProxyApiKeyView {
                id: key.id.clone(),
                name: key.name.clone(),
                key_preview: preview,
                enabled: key.enabled,
                created_at: key.created_at,
                last_used_at: key.last_used_at,
                credits_limit: key.credits_limit,
                usage: super::types::ProxyApiKeyUsage {
                    total_requests: key.usage.total_requests,
                    total_input_tokens: key.usage.total_input_tokens,
                    total_output_tokens: key.usage.total_output_tokens,
                    total_credits: key.usage.total_credits,
                    daily: key
                        .usage
                        .daily
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                super::types::ProxyApiKeyUsageDaily {
                                    requests: v.requests,
                                    input_tokens: v.input_tokens,
                                    output_tokens: v.output_tokens,
                                    credits: v.credits,
                                },
                            )
                        })
                        .collect::<HashMap<_, _>>(),
                    by_model: key
                        .usage
                        .by_model
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                super::types::ProxyApiKeyUsageModel {
                                    requests: v.requests,
                                    input_tokens: v.input_tokens,
                                    output_tokens: v.output_tokens,
                                    credits: v.credits,
                                },
                            )
                        })
                        .collect::<HashMap<_, _>>(),
                },
                usage_history: key.usage_history.clone(),
            }
        })
        .collect()
}