import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import { Play, RefreshCw, RotateCcw, Square } from 'lucide-react';
import {
  addKiroProxyApiKey,
  clearKiroProxyLogs,
  deleteKiroProxyApiKey,
  getKiroProxyAccounts,
  getKiroProxyApiKeys,
  getKiroProxyConfig,
  getKiroProxyLogs,
  getKiroProxyModels,
  getKiroProxyStats,
  getKiroProxyStatus,
  refreshKiroProxyModels,
  resetKiroProxyApiKeyUsage,
  resetKiroProxyStats,
  startKiroProxy,
  stopKiroProxy,
  syncKiroProxyAccounts,
  updateKiroProxyApiKey,
  updateKiroProxyConfig,
  type KiroProxyAccountView,
  type KiroProxyAdminStatsResponse,
  type KiroProxyApiKeyView,
  type KiroProxyConfig,
  type KiroProxyModelMappingRule,
  type KiroProxyModelView,
  type KiroProxyRequestLog,
  type KiroProxyStatus,
} from '../services/kiroProxyService';
import {
  AccountSelectionPanel,
  ApiKeysPanel,
  EndpointDocsPanel,
  LogsPanel,
  ModelMappingsPanel,
} from '../components/kiro-proxy';
import '../components/kiro-proxy/kiroProxy.css';

function formatDuration(totalSeconds?: number | null): string {
  if (!totalSeconds || totalSeconds <= 0) {
    return '-';
  }
  const seconds = Math.floor(totalSeconds % 60);
  const minutes = Math.floor((totalSeconds / 60) % 60);
  const hours = Math.floor(totalSeconds / 3600);
  if (hours > 0) {
    return `${hours}h ${minutes}m ${seconds}s`;
  }
  return `${minutes}m ${seconds}s`;
}

export function KiroProxyPage() {
  const { t } = useTranslation();
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<KiroProxyStatus | null>(null);
  const [config, setConfig] = useState<KiroProxyConfig | null>(null);
  const [stats, setStats] = useState<KiroProxyAdminStatsResponse | null>(null);
  const [accounts, setAccounts] = useState<KiroProxyAccountView[]>([]);
  const [models, setModels] = useState<KiroProxyModelView[]>([]);
  const [logs, setLogs] = useState<KiroProxyRequestLog[]>([]);
  const [apiKeys, setApiKeys] = useState<KiroProxyApiKeyView[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [messageTone, setMessageTone] = useState<'error' | 'success'>('success');
  const [clockNowSec, setClockNowSec] = useState<number>(() => Math.floor(Date.now() / 1000));
  const refreshTimerRef = useRef<number | null>(null);

  const loadCore = useCallback(async () => {
    const [statusResult, configResult, accountsResult, modelsResult, statsResult, logsResult, apiKeyResult] =
      await Promise.allSettled([
        getKiroProxyStatus(),
        getKiroProxyConfig(),
        getKiroProxyAccounts(),
        getKiroProxyModels(),
        getKiroProxyStats(),
        getKiroProxyLogs(200),
        getKiroProxyApiKeys(),
      ]);

    if (statusResult.status === 'fulfilled') setStatus(statusResult.value);
    if (configResult.status === 'fulfilled') setConfig(configResult.value);
    if (accountsResult.status === 'fulfilled') setAccounts(accountsResult.value);
    if (modelsResult.status === 'fulfilled') setModels(modelsResult.value);
    if (statsResult.status === 'fulfilled') setStats(statsResult.value);
    if (logsResult.status === 'fulfilled') setLogs(logsResult.value.logs);
    if (apiKeyResult.status === 'fulfilled') setApiKeys(apiKeyResult.value);
  }, []);

  const refreshStatsAndLogs = useCallback(async () => {
    const [statsResult, logsResult, apiKeysResult] = await Promise.allSettled([
      getKiroProxyStats(),
      getKiroProxyLogs(200),
      getKiroProxyApiKeys(),
    ]);
    if (statsResult.status === 'fulfilled') setStats(statsResult.value);
    if (logsResult.status === 'fulfilled') setLogs(logsResult.value.logs);
    if (apiKeysResult.status === 'fulfilled') setApiKeys(apiKeysResult.value);
  }, []);

  useEffect(() => {
    let unmounted = false;
    const init = async () => {
      try {
        await loadCore();
      } finally {
        if (!unmounted) {
          setLoading(false);
        }
      }
    };
    void init();
    return () => {
      unmounted = true;
    };
  }, [loadCore]);

  useEffect(() => {
    const timer = window.setInterval(() => {
      setClockNowSec(Math.floor(Date.now() / 1000));
    }, 1000);
    return () => window.clearInterval(timer);
  }, []);

  useEffect(() => {
    let unlistenFns: UnlistenFn[] = [];

    const scheduleRefresh = () => {
      if (refreshTimerRef.current != null) {
        return;
      }
      refreshTimerRef.current = window.setTimeout(() => {
        refreshTimerRef.current = null;
        void refreshStatsAndLogs();
      }, 500);
    };

    const register = async () => {
      const statusUnlisten = await listen<KiroProxyStatus>('kiro-proxy:status-change', (event) => {
        setStatus(event.payload);
      });
      const responseUnlisten = await listen<Record<string, unknown>>('kiro-proxy:response', () => {
        scheduleRefresh();
      });
      const errorUnlisten = await listen<Record<string, unknown>>('kiro-proxy:error', () => {
        scheduleRefresh();
      });
      unlistenFns = [statusUnlisten, responseUnlisten, errorUnlisten];
    };

    void register();
    return () => {
      if (refreshTimerRef.current != null) {
        window.clearTimeout(refreshTimerRef.current);
        refreshTimerRef.current = null;
      }
      unlistenFns.forEach((unlisten) => unlisten());
    };
  }, [refreshStatsAndLogs]);

  const showSuccess = useCallback((text: string) => {
    setMessage(text);
    setMessageTone('success');
  }, []);

  const showError = useCallback((error: unknown) => {
    setMessage(String(error).replace(/^Error:\s*/, ''));
    setMessageTone('error');
  }, []);

  const withBusy = useCallback(
    async (task: () => Promise<void>, successText: string) => {
      setBusy(true);
      setMessage(null);
      try {
        await task();
        showSuccess(successText);
      } catch (error) {
        showError(error);
      } finally {
        setBusy(false);
      }
    },
    [showError, showSuccess],
  );

  const updateConfigField = useCallback(
    <K extends keyof KiroProxyConfig>(key: K, value: KiroProxyConfig[K]) => {
      setConfig((prev) => (prev ? { ...prev, [key]: value } : prev));
    },
    [],
  );

  const endpointUrl = useMemo(() => {
    if (!status) {
      return 'http://127.0.0.1:5580';
    }
    return `http://${status.host}:${status.port}`;
  }, [status]);

  const handleStart = () =>
    withBusy(async () => {
      const next = await startKiroProxy();
      setStatus(next);
      await refreshStatsAndLogs();
    }, t('kiroProxy.messages.started', 'Proxy started'));

  const handleStop = () =>
    withBusy(async () => {
      const next = await stopKiroProxy();
      setStatus(next);
      await refreshStatsAndLogs();
    }, t('kiroProxy.messages.stopped', 'Proxy stopped'));

  const handleRestart = () =>
    withBusy(async () => {
      await stopKiroProxy();
      const next = await startKiroProxy();
      setStatus(next);
      await refreshStatsAndLogs();
    }, t('kiroProxy.messages.restarted', 'Proxy restarted'));

  const handleSaveConfig = () => {
    if (!config) return;
    return withBusy(async () => {
      const saved = await updateKiroProxyConfig(config);
      setConfig(saved);
      setStatus(await getKiroProxyStatus());
      setAccounts(await getKiroProxyAccounts());
    }, t('kiroProxy.messages.configSaved', 'Config saved'));
  };

  const handleSyncAccounts = () =>
    withBusy(async () => {
      const synced = await syncKiroProxyAccounts();
      setAccounts(synced);
    }, t('kiroProxy.messages.accountsSynced', 'Accounts synced'));

  const handleRefreshModels = () =>
    withBusy(async () => {
      const next = await refreshKiroProxyModels();
      setModels(next);
    }, t('kiroProxy.messages.modelsRefreshed', 'Models refreshed'));

  const handleAddApiKey = async (input: Parameters<typeof addKiroProxyApiKey>[0]) => {
    await addKiroProxyApiKey(input);
    const [keys, cfg] = await Promise.all([getKiroProxyApiKeys(), getKiroProxyConfig()]);
    setApiKeys(keys);
    setConfig(cfg);
    showSuccess(t('kiroProxy.messages.apiKeyAdded', 'API key added'));
  };

  const handleUpdateApiKey = async (input: Parameters<typeof updateKiroProxyApiKey>[0]) => {
    await updateKiroProxyApiKey(input);
    setApiKeys(await getKiroProxyApiKeys());
    showSuccess(t('kiroProxy.messages.apiKeyUpdated', 'API key updated'));
  };

  const handleDeleteApiKey = async (id: string) => {
    await deleteKiroProxyApiKey(id);
    const [keys, cfg] = await Promise.all([getKiroProxyApiKeys(), getKiroProxyConfig()]);
    setApiKeys(keys);
    setConfig(cfg);
    showSuccess(t('kiroProxy.messages.apiKeyDeleted', 'API key deleted'));
  };

  const handleResetApiKeyUsage = async (id: string) => {
    await resetKiroProxyApiKeyUsage(id);
    setApiKeys(await getKiroProxyApiKeys());
    showSuccess(t('kiroProxy.messages.apiKeyUsageReset', 'API key usage reset'));
  };

  const handleSaveMappings = async (mappings: KiroProxyModelMappingRule[]) => {
    if (!config) return;
    const nextConfig: KiroProxyConfig = { ...config, modelMappings: mappings };
    const saved = await updateKiroProxyConfig(nextConfig);
    setConfig(saved);
    showSuccess(t('kiroProxy.messages.mappingsSaved', 'Model mappings saved'));
  };

  const runtimeStatusClass = status?.running ? 'is-ok' : 'is-off';
  const computedUptimeSeconds = useMemo(() => {
    if (status?.running && typeof status.startedAt === 'number' && status.startedAt > 0) {
      return Math.max(0, clockNowSec - status.startedAt);
    }
    if (typeof status?.uptimeSeconds === 'number') {
      return status.uptimeSeconds;
    }
    if (typeof stats?.status?.uptimeSeconds === 'number') {
      return stats.status.uptimeSeconds;
    }
    return null;
  }, [clockNowSec, stats?.status?.uptimeSeconds, status?.running, status?.startedAt, status?.uptimeSeconds]);

  return (
    <div className="kiro-proxy-page">
      <section className="kiro-proxy-hero">
        <div>
          <h2>{t('kiroProxy.title', 'Kiro API Proxy')}</h2>
          <p>{t('kiroProxy.subtitle', 'OpenAI + Claude compatible Kiro proxy service')}</p>
          <p>{endpointUrl}</p>
        </div>
        <div className="kiro-proxy-actions">
          <button
            className="btn btn-success"
            onClick={() => void handleStart()}
            disabled={busy || status?.running === true}
          >
            <Play size={16} />
            {t('kiroProxy.actions.start', 'Start')}
          </button>
          <button
            className="btn btn-danger"
            onClick={() => void handleStop()}
            disabled={busy || status?.running !== true}
          >
            <Square size={16} />
            {t('kiroProxy.actions.stop', 'Stop')}
          </button>
          <button
            className="btn btn-secondary"
            onClick={() => void handleRestart()}
            disabled={busy || status?.running !== true}
          >
            <RotateCcw size={16} />
            {t('kiroProxy.actions.restart', 'Restart')}
          </button>
          <button className="btn btn-secondary" onClick={() => void loadCore()} disabled={busy || loading}>
            <RefreshCw size={16} />
            {t('common.refresh', 'Refresh')}
          </button>
        </div>
      </section>

      {message && (
        <section className="kiro-proxy-panel">
          <span className={`kiro-proxy-status ${messageTone === 'error' ? 'is-bad' : 'is-ok'}`}>
            {message}
          </span>
        </section>
      )}

      <div className="kiro-proxy-grid">
        <section className="kiro-proxy-panel">
          <div className="kiro-proxy-panel-head">
            <h3>{t('kiroProxy.status.title', 'Runtime Status')}</h3>
            <span className={`kiro-proxy-status ${runtimeStatusClass}`}>
              {status?.running
                ? t('kiroProxy.status.running', 'Running')
                : t('kiroProxy.status.stopped', 'Stopped')}
            </span>
          </div>
          <div className="kiro-proxy-metrics">
            <div className="kiro-proxy-metric">
              <span>{t('kiroProxy.status.uptime', 'Uptime')}</span>
              <strong>{formatDuration(computedUptimeSeconds)}</strong>
            </div>
            <div className="kiro-proxy-metric">
              <span>{t('kiroProxy.status.totalRequests', 'Requests')}</span>
              <strong>{stats?.aggregate.totalRequests ?? status?.requestCount ?? 0}</strong>
            </div>
            <div className="kiro-proxy-metric">
              <span>{t('kiroProxy.status.tokens', 'Tokens')}</span>
              <strong>
                {(stats?.aggregate.totalInputTokens ?? 0) + (stats?.aggregate.totalOutputTokens ?? 0)}
              </strong>
            </div>
            <div className="kiro-proxy-metric">
              <span>{t('kiroProxy.status.credits', 'Credits')}</span>
              <strong>{(stats?.aggregate.totalCredits ?? 0).toFixed(2)}</strong>
            </div>
          </div>
        </section>

        <section className="kiro-proxy-panel">
          <div className="kiro-proxy-panel-head">
            <h3>{t('kiroProxy.config.title', 'Proxy Config')}</h3>
            <button className="btn btn-primary btn-sm" onClick={() => void handleSaveConfig()} disabled={busy || !config}>
              {t('common.save', 'Save')}
            </button>
          </div>
          {config ? (
            <>
              <div className="kiro-proxy-inline-form">
                <label className="kiro-proxy-check">
                  <input
                    type="checkbox"
                    checked={config.enabled}
                    onChange={(event) => updateConfigField('enabled', event.target.checked)}
                  />
                  <span>{t('kiroProxy.config.enabled', 'Enabled')}</span>
                  <em title="Turns the proxy service feature on/off in saved config. Start/Stop still controls runtime.">!</em>
                </label>
                <label className="kiro-proxy-check">
                  <input
                    type="checkbox"
                    checked={config.autoStart}
                    onChange={(event) => updateConfigField('autoStart', event.target.checked)}
                  />
                  <span>{t('kiroProxy.config.autoStart', 'Auto start')}</span>
                  <em title="Automatically starts the proxy when Mira starts.">!</em>
                </label>
                <label className="kiro-proxy-check">
                  <input
                    type="checkbox"
                    checked={config.logRequests}
                    onChange={(event) => updateConfigField('logRequests', event.target.checked)}
                  />
                  <span>{t('kiroProxy.config.logRequests', 'Log requests')}</span>
                  <em title="Stores request/response history for logs and stats.">!</em>
                </label>
                <label className="kiro-proxy-check">
                  <input
                    type="checkbox"
                    checked={config.disableTools}
                    onChange={(event) => updateConfigField('disableTools', event.target.checked)}
                  />
                  <span>{t('kiroProxy.config.disableTools', 'Disable tools')}</span>
                  <em title="Removes tool/function-calling behavior from translated requests.">!</em>
                </label>
              </div>
              <div className="kiro-proxy-inline-form kiro-proxy-config-grid">
                <div className="kiro-proxy-field">
                  <label className="kiro-proxy-field-label">
                    Host <em title="Bind address of proxy server. Use 127.0.0.1 for local-only access.">!</em>
                  </label>
                  <input
                    value={config.host}
                    onChange={(event) => updateConfigField('host', event.target.value)}
                    placeholder="127.0.0.1"
                  />
                </div>
                <div className="kiro-proxy-field">
                  <label className="kiro-proxy-field-label">
                    Port <em title="HTTP port used by the local proxy endpoint.">!</em>
                  </label>
                  <input
                    value={String(config.port)}
                    onChange={(event) => updateConfigField('port', Number.parseInt(event.target.value, 10) || 5580)}
                    inputMode="numeric"
                    placeholder="5580"
                  />
                </div>
                <div className="kiro-proxy-field">
                  <label className="kiro-proxy-field-label">
                    Max retries <em title="Maximum retry attempts for upstream errors.">!</em>
                  </label>
                  <input
                    value={String(config.maxRetries)}
                    onChange={(event) => updateConfigField('maxRetries', Number.parseInt(event.target.value, 10) || 1)}
                    inputMode="numeric"
                    placeholder="3"
                  />
                </div>
                <div className="kiro-proxy-field">
                  <label className="kiro-proxy-field-label">
                    Retry delay (ms) <em title="Base delay in milliseconds before retrying failed upstream calls.">!</em>
                  </label>
                  <input
                    value={String(config.retryDelayMs)}
                    onChange={(event) => updateConfigField('retryDelayMs', Number.parseInt(event.target.value, 10) || 1000)}
                    inputMode="numeric"
                    placeholder="1000"
                  />
                </div>
              </div>
              <div className="kiro-proxy-inline-form kiro-proxy-config-grid">
                <div className="kiro-proxy-field">
                  <label className="kiro-proxy-field-label">
                    Preferred endpoint <em title="Choose specific upstream endpoint, or Auto for fallback routing.">!</em>
                  </label>
                  <select
                    value={config.preferredEndpoint || ''}
                    onChange={(event) =>
                      updateConfigField('preferredEndpoint', event.target.value || null)
                    }
                  >
                    <option value="">{t('kiroProxy.config.endpointAuto', 'Auto endpoint')}</option>
                    <option value="CodeWhisperer">CodeWhisperer</option>
                    <option value="AmazonQ">AmazonQ</option>
                  </select>
                </div>
                <div className="kiro-proxy-field">
                  <label className="kiro-proxy-field-label">
                    Thinking output <em title="Format of reasoning/thinking content returned in streaming responses.">!</em>
                  </label>
                  <select
                    value={config.thinkingOutputFormat}
                    onChange={(event) => updateConfigField('thinkingOutputFormat', event.target.value)}
                  >
                    <option value="reasoning_content">reasoning_content</option>
                    <option value="thinking">thinking</option>
                    <option value="think">think</option>
                  </select>
                </div>
                <div className="kiro-proxy-field">
                  <label className="kiro-proxy-field-label">
                    Auto continue rounds <em title="Extra continuation rounds when response is incomplete (0 = disabled).">!</em>
                  </label>
                  <input
                    value={String(config.autoContinueRounds)}
                    onChange={(event) =>
                      updateConfigField('autoContinueRounds', Number.parseInt(event.target.value, 10) || 0)
                    }
                    inputMode="numeric"
                    placeholder="0"
                  />
                </div>
                <div className="kiro-proxy-field">
                  <label className="kiro-proxy-field-label">
                    Model cache TTL (sec) <em title="How long fetched model list is cached before refetch.">!</em>
                  </label>
                  <input
                    value={String(config.modelCacheTtlSec)}
                    onChange={(event) =>
                      updateConfigField('modelCacheTtlSec', Number.parseInt(event.target.value, 10) || 300)
                    }
                    inputMode="numeric"
                    placeholder="300"
                  />
                </div>
              </div>
              <div className="kiro-proxy-inline-form kiro-proxy-config-grid">
                <div className="kiro-proxy-field">
                  <label className="kiro-proxy-field-label">
                    Token refresh before expiry (sec) <em title="Refresh account token this many seconds before expiry.">!</em>
                  </label>
                  <input
                    value={String(config.tokenRefreshBeforeExpirySec)}
                    onChange={(event) =>
                      updateConfigField(
                        'tokenRefreshBeforeExpirySec',
                        Number.parseInt(event.target.value, 10) || 300,
                      )
                    }
                    inputMode="numeric"
                    placeholder="300"
                  />
                </div>
                <div className="kiro-proxy-field">
                  <label className="kiro-proxy-field-label">
                    Legacy single API key <em title="Old single-key auth mode. Prefer API Keys manager below.">!</em>
                  </label>
                  <input
                    value={config.apiKey || ''}
                    onChange={(event) => updateConfigField('apiKey', event.target.value || null)}
                    placeholder={t('kiroProxy.config.legacyApiKey', 'Legacy single API key')}
                  />
                </div>
                <label className="kiro-proxy-check">
                  <input
                    type="checkbox"
                    checked={config.autoSwitchOnQuotaExhausted}
                    onChange={(event) =>
                      updateConfigField('autoSwitchOnQuotaExhausted', event.target.checked)
                    }
                  />
                  <span>{t('kiroProxy.config.autoSwitchQuota', 'Auto switch on quota exhausted')}</span>
                  <em title="Automatically fallback to another account when quota/credits are exhausted.">!</em>
                </label>
              </div>
            </>
          ) : (
            <div className="kiro-proxy-empty">{t('common.loading', 'Loading...')}</div>
          )}
        </section>
      </div>

      <div className="kiro-proxy-grid">
        <AccountSelectionPanel
          accounts={accounts}
          selectedIds={config?.selectedAccountIds || []}
          enableMultiAccount={config?.enableMultiAccount || false}
          onToggleMultiAccount={(enabled) => updateConfigField('enableMultiAccount', enabled)}
          onToggleAccount={(id) => {
            const selected = new Set(config?.selectedAccountIds || []);
            if (selected.has(id)) {
              selected.delete(id);
            } else {
              selected.add(id);
            }
            updateConfigField('selectedAccountIds', [...selected]);
          }}
        />

        <section className="kiro-proxy-panel">
          <div className="kiro-proxy-panel-head">
            <h3>{t('kiroProxy.models.title', 'Models')}</h3>
            <div className="kiro-proxy-actions">
              <button className="btn btn-secondary btn-sm" onClick={() => void handleSyncAccounts()} disabled={busy}>
                {t('kiroProxy.actions.syncAccounts', 'Sync Accounts')}
              </button>
              <button className="btn btn-secondary btn-sm" onClick={() => void handleRefreshModels()} disabled={busy}>
                {t('kiroProxy.actions.refreshModels', 'Refresh Models')}
              </button>
            </div>
          </div>
          <div className="kiro-proxy-table-wrap">
            <table className="kiro-proxy-table">
              <thead>
                <tr>
                  <th>{t('kiroProxy.models.id', 'Model ID')}</th>
                  <th>{t('kiroProxy.models.name', 'Name')}</th>
                  <th>{t('kiroProxy.models.source', 'Source')}</th>
                </tr>
              </thead>
              <tbody>
                {models.length === 0 ? (
                  <tr>
                    <td colSpan={3} className="kiro-proxy-empty-cell">
                      {t('kiroProxy.models.empty', 'No models loaded')}
                    </td>
                  </tr>
                ) : (
                  models.map((item) => (
                    <tr key={item.id}>
                      <td>{item.id}</td>
                      <td>{item.name}</td>
                      <td>{item.source}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </section>
      </div>

      <ApiKeysPanel
        apiKeys={apiKeys}
        busy={busy}
        onAdd={handleAddApiKey}
        onUpdate={handleUpdateApiKey}
        onDelete={handleDeleteApiKey}
        onResetUsage={handleResetApiKeyUsage}
      />

      <ModelMappingsPanel
        mappings={config?.modelMappings || []}
        apiKeys={apiKeys}
        busy={busy}
        onSave={handleSaveMappings}
      />

      <div className="kiro-proxy-grid">
        <section className="kiro-proxy-panel">
          <div className="kiro-proxy-panel-head">
            <h3>{t('kiroProxy.stats.title', 'Stats')}</h3>
            <button
              className="btn btn-danger btn-sm"
              onClick={() =>
                withBusy(async () => {
                  await resetKiroProxyStats();
                  await refreshStatsAndLogs();
                }, t('kiroProxy.messages.statsReset', 'Stats reset'))
              }
              disabled={busy}
            >
              {t('kiroProxy.actions.resetStats', 'Reset Stats')}
            </button>
          </div>
          <div className="kiro-proxy-docs">
            <p>{t('kiroProxy.stats.total', 'Total')}: {stats?.aggregate.totalRequests ?? 0}</p>
            <p>{t('kiroProxy.stats.success', 'Success')}: {stats?.aggregate.successRequests ?? 0}</p>
            <p>{t('kiroProxy.stats.failed', 'Failed')}: {stats?.aggregate.failedRequests ?? 0}</p>
            <p>{t('kiroProxy.stats.inputTokens', 'Input Tokens')}: {stats?.aggregate.totalInputTokens ?? 0}</p>
            <p>{t('kiroProxy.stats.outputTokens', 'Output Tokens')}: {stats?.aggregate.totalOutputTokens ?? 0}</p>
            <p>{t('kiroProxy.stats.credits', 'Credits')}: {(stats?.aggregate.totalCredits ?? 0).toFixed(2)}</p>
          </div>
        </section>

        <EndpointDocsPanel host={status?.host || '127.0.0.1'} port={status?.port || 5580} />
      </div>

      <LogsPanel
        logs={logs}
        busy={busy}
        onRefresh={refreshStatsAndLogs}
        onClear={async () => {
          await clearKiroProxyLogs();
          await refreshStatsAndLogs();
        }}
      />

    </div>
  );
}
