import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { RotateCw, Trash2 } from 'lucide-react';
import type { KiroProxyRequestLog } from '../../services/kiroProxyService';

interface LogsPanelProps {
  logs: KiroProxyRequestLog[];
  busy: boolean;
  onRefresh: () => Promise<void>;
  onClear: () => Promise<void>;
}

function formatTs(value: number): string {
  if (!value) return '-';
  return new Date(value * 1000).toLocaleString();
}

export function LogsPanel({ logs, busy, onRefresh, onClear }: LogsPanelProps) {
  const { t } = useTranslation();
  const rows = useMemo(() => [...logs].sort((a, b) => b.timestamp - a.timestamp), [logs]);

  return (
    <section className="kiro-proxy-panel">
      <div className="kiro-proxy-panel-head">
        <h3>{t('kiroProxy.logs.title', 'Request Logs')}</h3>
        <div className="kiro-proxy-actions">
          <button className="btn btn-secondary btn-sm" onClick={() => void onRefresh()} disabled={busy}>
            <RotateCw size={14} />
            {t('common.refresh', 'Refresh')}
          </button>
          <button className="btn btn-danger btn-sm" onClick={() => void onClear()} disabled={busy}>
            <Trash2 size={14} />
            {t('kiroProxy.logs.clear', 'Clear')}
          </button>
        </div>
      </div>

      <div className="kiro-proxy-table-wrap">
        <table className="kiro-proxy-table">
          <thead>
            <tr>
              <th>{t('kiroProxy.logs.time', 'Time')}</th>
              <th>{t('kiroProxy.logs.path', 'Path')}</th>
              <th>{t('kiroProxy.logs.model', 'Model')}</th>
              <th>{t('kiroProxy.logs.account', 'Account')}</th>
              <th>{t('kiroProxy.logs.tokens', 'Tokens')}</th>
              <th>{t('kiroProxy.logs.credits', 'Credits')}</th>
              <th>{t('kiroProxy.logs.latency', 'Latency')}</th>
              <th>{t('kiroProxy.logs.status', 'Status')}</th>
            </tr>
          </thead>
          <tbody>
            {rows.length === 0 ? (
              <tr>
                <td colSpan={8} className="kiro-proxy-empty-cell">
                  {t('kiroProxy.logs.empty', 'No request logs')}
                </td>
              </tr>
            ) : (
              rows.slice(0, 400).map((item, index) => (
                <tr key={`${item.timestamp}-${item.path}-${index}`}>
                  <td>{formatTs(item.timestamp)}</td>
                  <td>{item.method} {item.path}</td>
                  <td>{item.model || '-'}</td>
                  <td>{item.accountEmail || '-'}</td>
                  <td>{item.inputTokens + item.outputTokens}</td>
                  <td>{item.credits.toFixed(2)}</td>
                  <td>{item.responseTimeMs}ms</td>
                  <td>
                    <span className={`kiro-proxy-status ${item.success ? 'is-ok' : 'is-bad'}`}>
                      {item.status}
                    </span>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </section>
  );
}
