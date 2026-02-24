import { invoke } from '@tauri-apps/api/core';

export interface KiroProxyStatus {
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
  totalCredits: number;
  error?: string | null;
}

export interface KiroProxyApiKeyUsageDaily {
  requests: number;
  inputTokens: number;
  outputTokens: number;
  credits: number;
}

export interface KiroProxyApiKeyUsageModel {
  requests: number;
  inputTokens: number;
  outputTokens: number;
  credits: number;
}

export interface KiroProxyApiKeyUsage {
  totalRequests: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  totalCredits: number;
  daily: Record<string, KiroProxyApiKeyUsageDaily>;
  byModel: Record<string, KiroProxyApiKeyUsageModel>;
}

export interface KiroProxyUsageRecord {
  timestamp: number;
  model: string;
  inputTokens: number;
  outputTokens: number;
  credits: number;
  path: string;
}

export interface KiroProxyApiKeyConfig {
  id: string;
  name: string;
  key: string;
  enabled: boolean;
  createdAt: number;
  lastUsedAt?: number | null;
  creditsLimit?: number | null;
  usage: KiroProxyApiKeyUsage;
  usageHistory: KiroProxyUsageRecord[];
}

export interface KiroProxyApiKeyView {
  id: string;
  name: string;
  keyPreview: string;
  enabled: boolean;
  createdAt: number;
  lastUsedAt?: number | null;
  creditsLimit?: number | null;
  usage: KiroProxyApiKeyUsage;
  usageHistory: KiroProxyUsageRecord[];
}

export interface KiroProxyModelMappingRule {
  id: string;
  name: string;
  enabled: boolean;
  type: string;
  sourceModel: string;
  targetModels: string[];
  weights: number[];
  priority: number;
  apiKeyIds: string[];
}

export interface KiroProxyConfig {
  enabled: boolean;
  autoStart: boolean;
  host: string;
  port: number;
  apiKey?: string | null;
  apiKeys: KiroProxyApiKeyConfig[];
  enableMultiAccount: boolean;
  selectedAccountIds: string[];
  logRequests: boolean;
  maxRetries: number;
  retryDelayMs: number;
  thinkingOutputFormat: string;
  autoContinueRounds: number;
  disableTools: boolean;
  preferredEndpoint?: string | null;
  modelCacheTtlSec: number;
  tokenRefreshBeforeExpirySec: number;
  autoSwitchOnQuotaExhausted: boolean;
  modelMappings: KiroProxyModelMappingRule[];
}

export interface KiroProxyAccountView {
  id: string;
  email: string;
  enabled: boolean;
  status?: string | null;
  statusReason?: string | null;
  lastUsed: number;
  requestCount: number;
  errorCount: number;
  cooldownUntil?: number | null;
  profileArn?: string | null;
}

export interface KiroProxyModelView {
  id: string;
  name: string;
  description: string;
  source: string;
}

export interface KiroProxyRequestLog {
  timestamp: number;
  path: string;
  method: string;
  model?: string | null;
  accountId?: string | null;
  accountEmail?: string | null;
  apiKeyId?: string | null;
  inputTokens: number;
  outputTokens: number;
  credits: number;
  responseTimeMs: number;
  status: number;
  success: boolean;
  error?: string | null;
}

export interface KiroProxyModelStats {
  requests: number;
  inputTokens: number;
  outputTokens: number;
  credits: number;
}

export interface KiroProxyDailyStats {
  requests: number;
  inputTokens: number;
  outputTokens: number;
  credits: number;
}

export interface KiroProxyAggregateStats {
  totalRequests: number;
  successRequests: number;
  failedRequests: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  totalCredits: number;
  byModel: Record<string, KiroProxyModelStats>;
  daily: Record<string, KiroProxyDailyStats>;
}

export interface KiroProxyAdminStatsResponse {
  status: KiroProxyStatus;
  aggregate: KiroProxyAggregateStats;
  accounts: KiroProxyAccountView[];
}

export interface KiroProxyAdminLogsResponse {
  logs: KiroProxyRequestLog[];
}

export interface KiroProxyAddApiKeyInput {
  name: string;
  key: string;
  enabled?: boolean;
  creditsLimit?: number | null;
}

export interface KiroProxyUpdateApiKeyInput {
  id: string;
  name?: string;
  enabled?: boolean;
  creditsLimit?: number | null;
}

export async function startKiroProxy(): Promise<KiroProxyStatus> {
  return invoke<KiroProxyStatus>('kiro_proxy_start');
}

export async function stopKiroProxy(): Promise<KiroProxyStatus> {
  return invoke<KiroProxyStatus>('kiro_proxy_stop');
}

export async function getKiroProxyStatus(): Promise<KiroProxyStatus> {
  return invoke<KiroProxyStatus>('kiro_proxy_get_status');
}

export async function getKiroProxyConfig(): Promise<KiroProxyConfig> {
  return invoke<KiroProxyConfig>('kiro_proxy_get_config');
}

export async function updateKiroProxyConfig(config: KiroProxyConfig): Promise<KiroProxyConfig> {
  return invoke<KiroProxyConfig>('kiro_proxy_update_config', { config });
}

export async function syncKiroProxyAccounts(): Promise<KiroProxyAccountView[]> {
  return invoke<KiroProxyAccountView[]>('kiro_proxy_sync_accounts');
}

export async function getKiroProxyAccounts(): Promise<KiroProxyAccountView[]> {
  return invoke<KiroProxyAccountView[]>('kiro_proxy_get_accounts');
}

export async function refreshKiroProxyModels(): Promise<KiroProxyModelView[]> {
  return invoke<KiroProxyModelView[]>('kiro_proxy_refresh_models');
}

export async function getKiroProxyModels(): Promise<KiroProxyModelView[]> {
  return invoke<KiroProxyModelView[]>('kiro_proxy_get_models');
}

export async function getKiroProxyLogs(limit?: number): Promise<KiroProxyAdminLogsResponse> {
  return invoke<KiroProxyAdminLogsResponse>('kiro_proxy_get_logs', { limit: limit ?? null });
}

export async function clearKiroProxyLogs(): Promise<void> {
  return invoke<void>('kiro_proxy_clear_logs');
}

export async function resetKiroProxyStats(): Promise<void> {
  return invoke<void>('kiro_proxy_reset_stats');
}

export async function getKiroProxyStats(): Promise<KiroProxyAdminStatsResponse> {
  return invoke<KiroProxyAdminStatsResponse>('kiro_proxy_get_stats');
}

export async function getKiroProxyApiKeys(): Promise<KiroProxyApiKeyView[]> {
  return invoke<KiroProxyApiKeyView[]>('kiro_proxy_get_api_keys');
}

export async function addKiroProxyApiKey(input: KiroProxyAddApiKeyInput): Promise<void> {
  return invoke<void>('kiro_proxy_add_api_key', { input });
}

export async function updateKiroProxyApiKey(input: KiroProxyUpdateApiKeyInput): Promise<void> {
  return invoke<void>('kiro_proxy_update_api_key', { input });
}

export async function deleteKiroProxyApiKey(id: string): Promise<void> {
  return invoke<void>('kiro_proxy_delete_api_key', { id });
}

export async function resetKiroProxyApiKeyUsage(id: string): Promise<void> {
  return invoke<void>('kiro_proxy_reset_api_key_usage', { id });
}
