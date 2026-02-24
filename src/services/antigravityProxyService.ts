import { invoke } from '@tauri-apps/api/core';

export interface AntigravityProxyStatus {
  running: boolean;
  host: string;
  port: number;
  startedAt?: number | null;
  uptimeSeconds?: number | null;
  requestCount: number;
  successCount: number;
  failedCount: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  error?: string | null;
}

export interface AntigravityProxyConfig {
  enabled: boolean;
  autoStart: boolean;
  host: string;
  port: number;
  authEnabled: boolean;
  apiKey?: string | null;
  enableMultiAccount: boolean;
  selectedAccountIds: string[];
  logRequests: boolean;
  maxRetries: number;
  retryDelayMs: number;
  modelCacheTtlSec: number;
  tokenRefreshBeforeExpirySec: number;
}

export interface AntigravityProxyAccountView {
  id: string;
  email: string;
  enabled: boolean;
  lastUsed: number;
  requestCount: number;
  errorCount: number;
  cooldownUntil?: number | null;
}

export interface AntigravityProxyModelView {
  id: string;
  name: string;
  description: string;
  source: string;
}

export interface AntigravityProxyRequestLog {
  timestamp: number;
  path: string;
  method: string;
  model?: string | null;
  accountId?: string | null;
  accountEmail?: string | null;
  inputTokens: number;
  outputTokens: number;
  responseTimeMs: number;
  status: number;
  success: boolean;
  error?: string | null;
}

export interface AntigravityProxyAdminLogsResponse {
  logs: AntigravityProxyRequestLog[];
}

export interface AntigravityProxyModelStats {
  requests: number;
  inputTokens: number;
  outputTokens: number;
}

export interface AntigravityProxyDailyStats {
  requests: number;
  inputTokens: number;
  outputTokens: number;
}

export interface AntigravityProxyAggregateStats {
  totalRequests: number;
  successRequests: number;
  failedRequests: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  byModel: Record<string, AntigravityProxyModelStats>;
  daily: Record<string, AntigravityProxyDailyStats>;
}

export interface AntigravityProxyAdminStatsResponse {
  status: AntigravityProxyStatus;
  aggregate: AntigravityProxyAggregateStats;
  accounts: AntigravityProxyAccountView[];
}

export async function startAntigravityProxy(): Promise<AntigravityProxyStatus> {
  return invoke<AntigravityProxyStatus>('antigravity_proxy_start');
}

export async function stopAntigravityProxy(): Promise<AntigravityProxyStatus> {
  return invoke<AntigravityProxyStatus>('antigravity_proxy_stop');
}

export async function getAntigravityProxyStatus(): Promise<AntigravityProxyStatus> {
  return invoke<AntigravityProxyStatus>('antigravity_proxy_get_status');
}

export async function getAntigravityProxyConfig(): Promise<AntigravityProxyConfig> {
  return invoke<AntigravityProxyConfig>('antigravity_proxy_get_config');
}

export async function updateAntigravityProxyConfig(
  config: AntigravityProxyConfig,
): Promise<AntigravityProxyConfig> {
  return invoke<AntigravityProxyConfig>('antigravity_proxy_update_config', { config });
}

export async function syncAntigravityProxyAccounts(): Promise<AntigravityProxyAccountView[]> {
  return invoke<AntigravityProxyAccountView[]>('antigravity_proxy_sync_accounts');
}

export async function getAntigravityProxyAccounts(): Promise<AntigravityProxyAccountView[]> {
  return invoke<AntigravityProxyAccountView[]>('antigravity_proxy_get_accounts');
}

export async function refreshAntigravityProxyModels(): Promise<AntigravityProxyModelView[]> {
  return invoke<AntigravityProxyModelView[]>('antigravity_proxy_refresh_models');
}

export async function getAntigravityProxyModels(): Promise<AntigravityProxyModelView[]> {
  return invoke<AntigravityProxyModelView[]>('antigravity_proxy_get_models');
}

export async function getAntigravityProxyLogs(
  limit?: number,
): Promise<AntigravityProxyAdminLogsResponse> {
  return invoke<AntigravityProxyAdminLogsResponse>('antigravity_proxy_get_logs', {
    limit: limit ?? null,
  });
}

export async function clearAntigravityProxyLogs(): Promise<void> {
  return invoke<void>('antigravity_proxy_clear_logs');
}

export async function resetAntigravityProxyStats(): Promise<void> {
  return invoke<void>('antigravity_proxy_reset_stats');
}

export async function getAntigravityProxyStats(): Promise<AntigravityProxyAdminStatsResponse> {
  return invoke<AntigravityProxyAdminStatsResponse>('antigravity_proxy_get_stats');
}
