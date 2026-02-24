use crate::models::kiro::KiroAccount;
use crate::modules::config::KiroProxyConfig;
use crate::modules::kiro_account;
use std::collections::HashMap;

use super::types::ProxyAccountView;

#[derive(Debug, Clone)]
pub struct PoolAccount {
    pub account: KiroAccount,
    pub request_count: u64,
    pub error_count: u64,
    pub cooldown_until: Option<i64>,
}

#[derive(Debug, Default)]
pub struct AccountPool {
    order: Vec<String>,
    accounts: HashMap<String, PoolAccount>,
    cursor: usize,
}

impl AccountPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn sync_accounts(&mut self, config: &KiroProxyConfig) {
        let all = kiro_account::list_accounts();
        let mut accounts = HashMap::new();
        let mut order = Vec::new();

        let selected_set: std::collections::HashSet<String> =
            config.selected_account_ids.iter().cloned().collect();

        for account in all {
            if !selected_set.is_empty() && !selected_set.contains(&account.id) {
                continue;
            }
            if account.access_token.trim().is_empty() {
                continue;
            }
            if is_unusable_account(&account) {
                continue;
            }

            order.push(account.id.clone());
            let previous = self.accounts.get(&account.id);
            accounts.insert(
                account.id.clone(),
                PoolAccount {
                    account,
                    request_count: previous.map(|p| p.request_count).unwrap_or(0),
                    error_count: previous.map(|p| p.error_count).unwrap_or(0),
                    cooldown_until: previous.and_then(|p| p.cooldown_until),
                },
            );
        }

        self.order = order;
        self.accounts = accounts;
        if self.cursor >= self.order.len() {
            self.cursor = 0;
        }
    }

    pub fn update_account(&mut self, updated: KiroAccount) {
        if let Some(entry) = self.accounts.get_mut(&updated.id) {
            entry.account = updated;
        }
    }

    pub fn next_account(&mut self, config: &KiroProxyConfig) -> Option<PoolAccount> {
        if self.order.is_empty() {
            return None;
        }

        let now = chrono::Utc::now().timestamp();

        if !config.enable_multi_account {
            let preferred = config
                .selected_account_ids
                .first()
                .and_then(|id| self.accounts.get(id).cloned())
                .or_else(|| self.order.first().and_then(|id| self.accounts.get(id).cloned()));

            if let Some(candidate) = preferred {
                if candidate
                    .cooldown_until
                    .map(|deadline| deadline > now)
                    .unwrap_or(false)
                {
                    return self.shortest_cooldown(now);
                }
                return Some(candidate);
            }
            return None;
        }

        for _ in 0..self.order.len() {
            let idx = self.cursor % self.order.len();
            self.cursor = (self.cursor + 1) % self.order.len();
            let account_id = &self.order[idx];
            if let Some(candidate) = self.accounts.get(account_id) {
                if candidate
                    .cooldown_until
                    .map(|deadline| deadline > now)
                    .unwrap_or(false)
                {
                    continue;
                }
                return Some(candidate.clone());
            }
        }

        self.shortest_cooldown(now)
    }

    fn shortest_cooldown(&self, now: i64) -> Option<PoolAccount> {
        self.accounts
            .values()
            .min_by_key(|entry| entry.cooldown_until.unwrap_or(now) - now)
            .cloned()
    }

    pub fn record_success(&mut self, account_id: &str) {
        if let Some(entry) = self.accounts.get_mut(account_id) {
            entry.request_count = entry.request_count.saturating_add(1);
            entry.account.last_used = chrono::Utc::now().timestamp();
            entry.error_count = 0;
            entry.cooldown_until = None;
        }
    }

    pub fn record_error(&mut self, account_id: &str, quota_error: bool) {
        if let Some(entry) = self.accounts.get_mut(account_id) {
            entry.error_count = entry.error_count.saturating_add(1);
            entry.account.last_used = chrono::Utc::now().timestamp();
            let cooldown_seconds = if quota_error { 3600 } else { 45 };
            entry.cooldown_until = Some(chrono::Utc::now().timestamp() + cooldown_seconds);
        }
    }

    pub fn views(&self) -> Vec<ProxyAccountView> {
        let mut list: Vec<ProxyAccountView> = self
            .accounts
            .values()
            .map(|entry| ProxyAccountView {
                id: entry.account.id.clone(),
                email: entry.account.email.clone(),
                enabled: true,
                status: entry.account.status.clone(),
                status_reason: entry.account.status_reason.clone(),
                last_used: entry.account.last_used,
                request_count: entry.request_count,
                error_count: entry.error_count,
                cooldown_until: entry.cooldown_until,
                profile_arn: extract_profile_arn(&entry.account),
            })
            .collect();

        list.sort_by(|a, b| b.last_used.cmp(&a.last_used));
        list
    }

    pub fn all_accounts(&self) -> Vec<PoolAccount> {
        self.accounts.values().cloned().collect()
    }
}

fn normalize_status_value(value: Option<&str>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_ascii_lowercase())
        }
    })
}

fn is_unusable_status(value: Option<&str>) -> bool {
    matches!(
        normalize_status_value(value).as_deref(),
        Some("banned") | Some("ban") | Some("forbidden") | Some("disabled")
    )
}

fn is_unusable_reason(value: Option<&str>) -> bool {
    let Some(reason) = normalize_status_value(value) else {
        return false;
    };
    reason.contains("bearer token included in the request is invalid")
        || reason.contains("invalid bearer token")
        || reason.contains("forbidden")
        || reason.contains("disabled")
        || reason.contains("banned")
}

fn is_unusable_account(account: &KiroAccount) -> bool {
    is_unusable_status(account.status.as_deref()) || is_unusable_reason(account.status_reason.as_deref())
}

pub fn extract_profile_arn(account: &KiroAccount) -> Option<String> {
    fn pick_profile_arn(value: &serde_json::Value) -> Option<String> {
        let obj = value.as_object()?;
        for key in ["profileArn", "profile_arn", "arn"] {
            if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
                let trimmed = v.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        None
    }

    account
        .kiro_profile_raw
        .as_ref()
        .and_then(pick_profile_arn)
        .or_else(|| {
            account
                .kiro_auth_token_raw
                .as_ref()
                .and_then(pick_profile_arn)
        })
}

pub fn extract_machine_id(account: &KiroAccount) -> Option<String> {
    fn pick_machine_id(value: &serde_json::Value) -> Option<String> {
        let obj = value.as_object()?;

        for key in [
            "machineId",
            "machine_id",
            "deviceId",
            "device_id",
            "clientDeviceId",
        ] {
            if let Some(v) = obj.get(key).and_then(|v| v.as_str()) {
                let trimmed = v.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }

        None
    }

    account
        .machine_id
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_string())
        .or_else(|| {
            account
                .kiro_profile_raw
                .as_ref()
                .and_then(pick_machine_id)
        })
        .or_else(|| {
            account
                .kiro_auth_token_raw
                .as_ref()
                .and_then(pick_machine_id)
        })
}
