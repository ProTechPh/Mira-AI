use std::collections::VecDeque;

use super::types::{
    ProxyAccountView, ProxyAdminStatsResponse, ProxyAggregateStats, ProxyDailyStats, ProxyModelStats,
    ProxyRequestLog, ProxyStatus,
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

        self.aggregate.total_input_tokens = self.aggregate.total_input_tokens.saturating_add(log.input_tokens);
        self.aggregate.total_output_tokens = self.aggregate.total_output_tokens.saturating_add(log.output_tokens);

        if let Some(model) = log.model.clone() {
            let entry = self.aggregate.by_model.entry(model).or_insert_with(ProxyModelStats::default);
            entry.requests = entry.requests.saturating_add(1);
            entry.input_tokens = entry.input_tokens.saturating_add(log.input_tokens);
            entry.output_tokens = entry.output_tokens.saturating_add(log.output_tokens);
        }

        let day_key = chrono::DateTime::<chrono::Utc>::from_timestamp(log.timestamp, 0)
            .unwrap_or_else(chrono::Utc::now)
            .format("%Y-%m-%d")
            .to_string();
        let daily = self.aggregate.daily.entry(day_key).or_insert_with(ProxyDailyStats::default);
        daily.requests = daily.requests.saturating_add(1);
        daily.input_tokens = daily.input_tokens.saturating_add(log.input_tokens);
        daily.output_tokens = daily.output_tokens.saturating_add(log.output_tokens);

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

    pub fn snapshot(&self, mut status: ProxyStatus, accounts: Vec<ProxyAccountView>) -> ProxyAdminStatsResponse {
        status.request_count = self.aggregate.total_requests;
        status.success_count = self.aggregate.success_requests;
        status.failed_count = self.aggregate.failed_requests;
        status.total_input_tokens = self.aggregate.total_input_tokens;
        status.total_output_tokens = self.aggregate.total_output_tokens;

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
