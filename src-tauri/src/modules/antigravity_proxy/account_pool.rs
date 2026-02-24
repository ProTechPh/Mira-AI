use std::collections::{HashMap, HashSet};

use crate::models::Account;
use crate::modules::account;
use crate::modules::config::AntigravityProxyConfig;

use super::types::ProxyAccountView;

#[derive(Debug, Clone)]
pub struct PoolAccount {
    pub account: Account,
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

    pub fn sync_accounts(&mut self, config: &AntigravityProxyConfig) {
        let all = account::list_accounts().unwrap_or_default();
        let mut accounts = HashMap::new();
        let mut order = Vec::new();

        let selected_set: HashSet<String> = config.selected_account_ids.iter().cloned().collect();

        for acc in all {
            if acc.disabled {
                continue;
            }
            if !selected_set.is_empty() && !selected_set.contains(&acc.id) {
                continue;
            }
            if acc.token.access_token.trim().is_empty() {
                continue;
            }

            order.push(acc.id.clone());
            let previous = self.accounts.get(&acc.id);
            accounts.insert(
                acc.id.clone(),
                PoolAccount {
                    account: acc,
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

    pub fn update_account(&mut self, updated: Account) {
        if let Some(entry) = self.accounts.get_mut(&updated.id) {
            entry.account = updated;
        }
    }

    pub fn next_account(
        &mut self,
        config: &AntigravityProxyConfig,
        current_account_id: Option<&str>,
    ) -> Option<PoolAccount> {
        if self.order.is_empty() {
            return None;
        }

        let now = chrono::Utc::now().timestamp();

        if !config.enable_multi_account {
            if let Some(current_id) = current_account_id {
                if let Some(candidate) = self.accounts.get(current_id).cloned() {
                    if candidate
                        .cooldown_until
                        .map(|deadline| deadline > now)
                        .unwrap_or(false)
                    {
                        return self.shortest_cooldown(now);
                    }
                    return Some(candidate);
                }
            }

            let preferred = self
                .order
                .first()
                .and_then(|id| self.accounts.get(id).cloned());
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
            let cooldown_seconds = if quota_error { 120 } else { 45 };
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
                enabled: !entry.account.disabled,
                last_used: entry.account.last_used,
                request_count: entry.request_count,
                error_count: entry.error_count,
                cooldown_until: entry.cooldown_until,
            })
            .collect();

        list.sort_by(|a, b| b.last_used.cmp(&a.last_used));
        list
    }
}
