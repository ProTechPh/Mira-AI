import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { listen, UnlistenFn } from '@tauri-apps/api/event';
import { useTranslation } from 'react-i18next';
import { Play, RefreshCw, RotateCcw, Square } from 'lucide-react';
import {
  clearAntigravityProxyLogs,
  getAntigravityProxyAccounts,
  getAntigravityProxyConfig,
  getAntigravityProxyLogs,
  getAntigravityProxyModels,
  getAntigravityProxyStats,
  getAntigravityProxyStatus,
  refreshAntigravityProxyModels,
  resetAntigravityProxyStats,
  startAntigravityProxy,
  stopAntigravityProxy,
  syncAntigravityProxyAccounts,
  updateAntigravityProxyConfig,
  type AntigravityProxyAccountView,
  type AntigravityProxyAdminStatsResponse,
  type AntigravityProxyConfig,
  type AntigravityProxyModelView,
  type AntigravityProxyRequestLog,
  type AntigravityProxyStatus,
} from '../services/antigravityProxyService';
import '../components/kiro-proxy/kiroProxy.css';
import './antigravityProxy.css';

function formatDuration(totalSeconds?: number | null): string {
  if (!totalSeconds || totalSeconds <= 0) return '-';
  const seconds = Math.floor(totalSeconds % 60);
  const minutes = Math.floor((totalSeconds / 60) % 60);
  const hours = Math.floor(totalSeconds / 3600);
  if (hours > 0) return `${hours}h ${minutes}m ${seconds}s`;
  return `${minutes}m ${seconds}s`;
}

export function AntigravityProxyPage() {
  const { t } = useTranslation();
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<AntigravityProxyStatus | null>(null);
  const [config, setConfig] = useState<AntigravityProxyConfig | null>(null);
  const [stats, setStats] = useState<AntigravityProxyAdminStatsResponse | null>(null);
  const [accounts, setAccounts] = useState<AntigravityProxyAccountView[]>([]);
  const [models, setModels] = useState<AntigravityProxyModelView[]>([]);
  const [logs, setLogs] = useState<AntigravityProxyRequestLog[]>([]);
  const [message, setMessage] = useState<string | null>(null);
  const [messageTone, setMessageTone] = useState<'error' | 'success'>('success');
  const [clockNowSec, setClockNowSec] = useState<number>(() => Math.floor(Date.now() / 1000));
  const refreshTimerRef = useRef<number | null>(null);

  const loadCore = useCallback(async () => {
    const [statusResult, configResult, accountsResult, modelsResult, statsResult, logsResult] =
      await Promise.allSettled([
        getAntigravityProxyStatus(),
        getAntigravityProxyConfig(),
        getAntigravityProxyAccounts(),
        getAntigravityProxyModels(),
        getAntigravityProxyStats(),
        getAntigravityProxyLogs(200),
      ]);

    if (statusResult.status === 'fulfilled') setStatus(statusResult.value);
    if (configResult.status === 'fulfilled') setConfig(configResult.value);
    if (accountsResult.status === 'fulfilled') setAccounts(accountsResult.value);
    if (modelsResult.status === 'fulfilled') setModels(modelsResult.value);
    if (statsResult.status === 'fulfilled') setStats(statsResult.value);
    if (logsResult.status === 'fulfilled') setLogs(logsResult.value.logs);
  }, []);

  const refreshStatsAndLogs = useCallback(async () => {
    const [statsResult, logsResult] = await Promise.allSettled([
      getAntigravityProxyStats(),
      getAntigravityProxyLogs(200),
    ]);
    if (statsResult.status === 'fulfilled') setStats(statsResult.value);
    if (logsResult.status === 'fulfilled') setLogs(logsResult.value.logs);
  }, []);

  useEffect(() => {
    let unmounted = false;
    const init = async () => {
      try {
        await loadCore();
      } finally {
        if (!unmounted) setLoading(false);
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
      if (refreshTimerRef.current != null) return;
      refreshTimerRef.current = window.setTimeout(() => {
        refreshTimerRef.current = null;
        void refreshStatsAndLogs();
      }, 500);
    };

    const register = async () => {
      const statusUnlisten = await listen<AntigravityProxyStatus>('antigravity-proxy://status', (event) => {
        setStatus(event.payload);
      });
      const logsUnlisten = await listen<Record<string, unknown>>('antigravity-proxy://request-log', () => {
        scheduleRefresh();
      });
      unlistenFns = [statusUnlisten, logsUnlisten];
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
    <K extends keyof AntigravityProxyConfig>(key: K, value: AntigravityProxyConfig[K]) => {
      setConfig((prev) => (prev ? { ...prev, [key]: value } : prev));
    },
    [],
  );

  const endpointUrl = useMemo(() => {
    if (!status) return 'http://127.0.0.1:5581';
    return `http://${status.host}:${status.port}`;
  }, [status]);

  const liveUptimeSeconds = useMemo(() => {
    if (!status?.running) {
      return status?.uptimeSeconds ?? null;
    }
    if (typeof status.startedAt !== 'number') {
      return status?.uptimeSeconds ?? null;
    }
    return Math.max(0, clockNowSec - status.startedAt);
  }, [clockNowSec, status]);

  const handleStart = () =>
    withBusy(async () => {
      const next = await startAntigravityProxy();
      setStatus(next);
      await refreshStatsAndLogs();
    }, t('antigravityProxy.messages.started', 'Proxy started'));

  const handleStop = () =>
    withBusy(async () => {
      const next = await stopAntigravityProxy();
      setStatus(next);
      await refreshStatsAndLogs();
    }, t('antigravityProxy.messages.stopped', 'Proxy stopped'));

  const handleRestart = () =>
    withBusy(async () => {
      await stopAntigravityProxy();
      const next = await startAntigravityProxy();
      setStatus(next);
      await refreshStatsAndLogs();
    }, t('antigravityProxy.messages.restarted', 'Proxy restarted'));

  const handleSaveConfig = () => {
    if (!config) return;
    return withBusy(async () => {
      const saved = await updateAntigravityProxyConfig(config);
      setConfig(saved);
      setStatus(await getAntigravityProxyStatus());
      setAccounts(await getAntigravityProxyAccounts());
    }, t('antigravityProxy.messages.configSaved', 'Config saved'));
  };

  const handleSyncAccounts = () =>
    withBusy(async () => {
      setAccounts(await syncAntigravityProxyAccounts());
    }, t('antigravityProxy.messages.accountsSynced', 'Accounts synced'));

  const handleRefreshModels = () =>
    withBusy(async () => {
      setModels(await refreshAntigravityProxyModels());
    }, t('antigravityProxy.messages.modelsRefreshed', 'Models refreshed'));

  const handleClearLogs = () =>
    withBusy(async () => {
      await clearAntigravityProxyLogs();
      setLogs([]);
      await refreshStatsAndLogs();
    }, t('antigravityProxy.messages.logsCleared', 'Logs cleared'));

  const handleResetStats = () =>
    withBusy(async () => {
      await resetAntigravityProxyStats();
      await refreshStatsAndLogs();
    }, t('antigravityProxy.messages.statsReset', 'Stats reset'));

  if (loading) {
    return <div className="kiro-proxy-page antigravity-proxy-page">{t('common.loading', 'Loading...')}</div>;
  }

  return (
    <div className="kiro-proxy-page antigravity-proxy-page">
      <div className="kiro-proxy-header">
        <div>
          <h2>{t('antigravityProxy.title', 'Antigravity API Proxy')}</h2>
          <p>{t('antigravityProxy.subtitle', 'OpenAI-compatible Antigravity proxy service')}</p>
        </div>
        <div className="kiro-proxy-actions">
          <button className="kiro-proxy-btn" disabled={busy || status?.running} onClick={handleStart}>
            <Play size={16} />
            {t('antigravityProxy.actions.start', 'Start')}
          </button>
          <button className="kiro-proxy-btn" disabled={busy || !status?.running} onClick={handleStop}>
            <Square size={16} />
            {t('antigravityProxy.actions.stop', 'Stop')}
          </button>
          <button className="kiro-proxy-btn" disabled={busy} onClick={handleRestart}>
            <RotateCcw size={16} />
            {t('antigravityProxy.actions.restart', 'Restart')}
          </button>
        </div>
      </div>

      {message && (
        <div className={`kiro-proxy-banner ${messageTone === 'error' ? 'error' : 'success'}`}>
          {message}
        </div>
      )}

      <section className="kiro-proxy-panel">
        <h3>{t('antigravityProxy.status.title', 'Runtime Status')}</h3>
        <div className="kiro-proxy-grid status-grid">
          <div className="status-card">
            <span>{t('antigravityProxy.status.state', 'State')}</span>
            <strong className={status?.running ? 'status-running' : 'status-stopped'}>
              {status?.running
                ? t('antigravityProxy.status.running', 'Running')
                : t('antigravityProxy.status.stopped', 'Stopped')}
            </strong>
          </div>
          <div className="status-card">
            <span>{t('antigravityProxy.status.uptime', 'Uptime')}</span>
            <strong>{formatDuration(liveUptimeSeconds)}</strong>
          </div>
          <div className="status-card">
            <span>{t('antigravityProxy.status.totalRequests', 'Requests')}</span>
            <strong>{status?.requestCount ?? 0}</strong>
          </div>
          <div className="status-card">
            <span>{t('antigravityProxy.status.tokens', 'Tokens')}</span>
            <strong>
              {(status?.totalInputTokens ?? 0).toLocaleString()} / {(status?.totalOutputTokens ?? 0).toLocaleString()}
            </strong>
          </div>
        </div>
      </section>

      <section className="kiro-proxy-panel">
        <h3>{t('antigravityProxy.config.title', 'Proxy Config')}</h3>
        <div className="kiro-proxy-form-grid">
          <label className="proxy-field proxy-toggle">
            <span>{t('antigravityProxy.config.enabled', 'Enabled')}</span>
            <input
              type="checkbox"
              checked={config?.enabled ?? false}
              onChange={(e) => updateConfigField('enabled', e.target.checked)}
            />
          </label>
          <label className="proxy-field proxy-toggle">
            <span>{t('antigravityProxy.config.autoStart', 'Auto start')}</span>
            <input
              type="checkbox"
              checked={config?.autoStart ?? false}
              onChange={(e) => updateConfigField('autoStart', e.target.checked)}
            />
          </label>
          <label className="proxy-field proxy-toggle">
            <span>{t('antigravityProxy.config.authEnabled', 'Enable API key auth')}</span>
            <input
              type="checkbox"
              checked={config?.authEnabled ?? false}
              onChange={(e) => updateConfigField('authEnabled', e.target.checked)}
            />
          </label>
          <label className="proxy-field proxy-toggle">
            <span>{t('antigravityProxy.config.multiAccount', 'Enable multi-account routing')}</span>
            <input
              type="checkbox"
              checked={config?.enableMultiAccount ?? true}
              onChange={(e) => updateConfigField('enableMultiAccount', e.target.checked)}
            />
          </label>
          <label className="proxy-field proxy-toggle">
            <span>{t('antigravityProxy.config.logRequests', 'Log requests')}</span>
            <input
              type="checkbox"
              checked={config?.logRequests ?? true}
              onChange={(e) => updateConfigField('logRequests', e.target.checked)}
            />
          </label>
          <label className="proxy-field">
            <span>{t('antigravityProxy.config.host', 'Host')}</span>
            <input
              value={config?.host ?? ''}
              onChange={(e) => updateConfigField('host', e.target.value)}
              placeholder="127.0.0.1"
            />
          </label>
          <label className="proxy-field">
            <span>{t('antigravityProxy.config.port', 'Port')}</span>
            <input
              type="number"
              value={config?.port ?? 5581}
              onChange={(e) => updateConfigField('port', Number(e.target.value))}
            />
          </label>
          <label className="proxy-field">
            <span>{t('antigravityProxy.config.apiKey', 'API Key')}</span>
            <input
              value={config?.apiKey ?? ''}
              onChange={(e) => updateConfigField('apiKey', e.target.value)}
              placeholder="sk-..."
            />
          </label>
          <label className="proxy-field">
            <span>{t('antigravityProxy.config.maxRetries', 'Max retries')}</span>
            <input
              type="number"
              value={config?.maxRetries ?? 2}
              onChange={(e) => updateConfigField('maxRetries', Number(e.target.value))}
            />
          </label>
          <label className="proxy-field">
            <span>{t('antigravityProxy.config.retryDelayMs', 'Retry delay (ms)')}</span>
            <input
              type="number"
              value={config?.retryDelayMs ?? 800}
              onChange={(e) => updateConfigField('retryDelayMs', Number(e.target.value))}
            />
          </label>
          <label className="proxy-field">
            <span>{t('antigravityProxy.config.modelCacheTtlSec', 'Model cache TTL (sec)')}</span>
            <input
              type="number"
              value={config?.modelCacheTtlSec ?? 300}
              onChange={(e) => updateConfigField('modelCacheTtlSec', Number(e.target.value))}
            />
          </label>
          <label className="proxy-field">
            <span>{t('antigravityProxy.config.tokenRefreshBeforeExpirySec', 'Token refresh before expiry (sec)')}</span>
            <input
              type="number"
              value={config?.tokenRefreshBeforeExpirySec ?? 300}
              onChange={(e) => updateConfigField('tokenRefreshBeforeExpirySec', Number(e.target.value))}
            />
          </label>
        </div>
        <div className="kiro-proxy-inline-actions">
          <button className="kiro-proxy-btn" disabled={busy || !config} onClick={handleSaveConfig}>
            <RefreshCw size={16} />
            {t('antigravityProxy.actions.saveConfig', 'Save Config')}
          </button>
        </div>
      </section>

      <section className="kiro-proxy-panel">
        <h3>{t('antigravityProxy.endpoints.title', 'Endpoint Docs')}</h3>
        <p>{t('antigravityProxy.endpoints.base', 'Base URL')}: <code>{endpointUrl}</code></p>
        <ul className="kiro-proxy-endpoints">
          <li><code>GET /v1/models</code></li>
          <li><code>POST /v1/chat/completions</code></li>
        </ul>
      </section>

      <section className="kiro-proxy-panel">
        <div className="kiro-proxy-panel-header">
          <h3>{t('antigravityProxy.accounts.title', 'Account Pool')}</h3>
          <button className="kiro-proxy-btn" disabled={busy} onClick={handleSyncAccounts}>
            {t('antigravityProxy.actions.syncAccounts', 'Sync Accounts')}
          </button>
        </div>
        <table className="kiro-proxy-table">
          <thead>
            <tr>
              <th>{t('antigravityProxy.accounts.email', 'Email')}</th>
              <th>{t('antigravityProxy.accounts.requests', 'Requests')}</th>
              <th>{t('antigravityProxy.accounts.errors', 'Errors')}</th>
              <th>{t('antigravityProxy.accounts.cooldown', 'Cooldown')}</th>
            </tr>
          </thead>
          <tbody>
            {accounts.length === 0 && (
              <tr>
                <td colSpan={4}>{t('antigravityProxy.accounts.empty', 'No Antigravity accounts found')}</td>
              </tr>
            )}
            {accounts.map((account) => {
              const cooldownLeft = account.cooldownUntil ? Math.max(0, account.cooldownUntil - clockNowSec) : 0;
              return (
                <tr key={account.id}>
                  <td>{account.email}</td>
                  <td>{account.requestCount}</td>
                  <td>{account.errorCount}</td>
                  <td>{cooldownLeft > 0 ? `${cooldownLeft}s` : '-'}</td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </section>

      <section className="kiro-proxy-panel">
        <div className="kiro-proxy-panel-header">
          <h3>{t('antigravityProxy.models.title', 'Models')}</h3>
          <button className="kiro-proxy-btn" disabled={busy} onClick={handleRefreshModels}>
            {t('antigravityProxy.actions.refreshModels', 'Refresh Models')}
          </button>
        </div>
        <table className="kiro-proxy-table">
          <thead>
            <tr>
              <th>{t('antigravityProxy.models.id', 'Model ID')}</th>
              <th>{t('antigravityProxy.models.name', 'Name')}</th>
              <th>{t('antigravityProxy.models.source', 'Source')}</th>
            </tr>
          </thead>
          <tbody>
            {models.length === 0 && (
              <tr>
                <td colSpan={3}>{t('antigravityProxy.models.empty', 'No models loaded')}</td>
              </tr>
            )}
            {models.map((model) => (
              <tr key={model.id}>
                <td>{model.id}</td>
                <td>{model.name}</td>
                <td>{model.source}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>

      <section className="kiro-proxy-panel">
        <div className="kiro-proxy-panel-header">
          <h3>{t('antigravityProxy.stats.title', 'Stats')}</h3>
          <button className="kiro-proxy-btn" disabled={busy} onClick={handleResetStats}>
            {t('antigravityProxy.actions.resetStats', 'Reset Stats')}
          </button>
        </div>
        <p>{t('antigravityProxy.stats.total', 'Total')}: {stats?.aggregate.totalRequests ?? 0}</p>
        <p>{t('antigravityProxy.stats.success', 'Success')}: {stats?.aggregate.successRequests ?? 0}</p>
        <p>{t('antigravityProxy.stats.failed', 'Failed')}: {stats?.aggregate.failedRequests ?? 0}</p>
        <p>{t('antigravityProxy.stats.inputTokens', 'Input Tokens')}: {stats?.aggregate.totalInputTokens ?? 0}</p>
        <p>{t('antigravityProxy.stats.outputTokens', 'Output Tokens')}: {stats?.aggregate.totalOutputTokens ?? 0}</p>
      </section>

      <section className="kiro-proxy-panel">
        <div className="kiro-proxy-panel-header">
          <h3>{t('antigravityProxy.logs.title', 'Request Logs')}</h3>
          <button className="kiro-proxy-btn" disabled={busy} onClick={handleClearLogs}>
            {t('antigravityProxy.logs.clear', 'Clear')}
          </button>
        </div>
        <table className="kiro-proxy-table">
          <thead>
            <tr>
              <th>{t('antigravityProxy.logs.time', 'Time')}</th>
              <th>{t('antigravityProxy.logs.path', 'Path')}</th>
              <th>{t('antigravityProxy.logs.model', 'Model')}</th>
              <th>{t('antigravityProxy.logs.account', 'Account')}</th>
              <th>{t('antigravityProxy.logs.tokens', 'Tokens')}</th>
              <th>{t('antigravityProxy.logs.latency', 'Latency')}</th>
              <th>{t('antigravityProxy.logs.status', 'Status')}</th>
            </tr>
          </thead>
          <tbody>
            {logs.length === 0 && (
              <tr>
                <td colSpan={7}>{t('antigravityProxy.logs.empty', 'No request logs')}</td>
              </tr>
            )}
            {logs.map((log, index) => (
              <tr key={`${log.timestamp}-${index}`}>
                <td>{new Date(log.timestamp * 1000).toLocaleString()}</td>
                <td>{log.path}</td>
                <td>{log.model ?? '-'}</td>
                <td>{log.accountEmail ?? '-'}</td>
                <td>{log.inputTokens}/{log.outputTokens}</td>
                <td>{log.responseTimeMs}ms</td>
                <td className={log.success ? 'status-running' : 'status-stopped'}>{log.status}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </section>
    </div>
  );
}
