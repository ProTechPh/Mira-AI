import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, RotateCcw, Trash2 } from 'lucide-react';
import type {
  KiroProxyAddApiKeyInput,
  KiroProxyApiKeyView,
  KiroProxyUpdateApiKeyInput,
} from '../../services/kiroProxyService';

interface ApiKeysPanelProps {
  apiKeys: KiroProxyApiKeyView[];
  busy: boolean;
  onAdd: (input: KiroProxyAddApiKeyInput) => Promise<void>;
  onUpdate: (input: KiroProxyUpdateApiKeyInput) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
  onResetUsage: (id: string) => Promise<void>;
}

export function ApiKeysPanel({
  apiKeys,
  busy,
  onAdd,
  onUpdate,
  onDelete,
  onResetUsage,
}: ApiKeysPanelProps) {
  const { t } = useTranslation();
  const [name, setName] = useState('');
  const [key, setKey] = useState('');
  const [creditsLimit, setCreditsLimit] = useState('');
  const [enabled, setEnabled] = useState(true);
  const [submitting, setSubmitting] = useState(false);

  const sorted = useMemo(
    () => [...apiKeys].sort((a, b) => b.createdAt - a.createdAt),
    [apiKeys],
  );

  const handleCreate = async () => {
    const trimmedKey = key.trim();
    if (!trimmedKey) {
      return;
    }

    const trimmedLimit = creditsLimit.trim();
    const parsedLimit = trimmedLimit ? Number.parseFloat(trimmedLimit) : undefined;
    const payload: KiroProxyAddApiKeyInput = {
      name: name.trim() || 'API Key',
      key: trimmedKey,
      enabled,
      creditsLimit: Number.isFinite(parsedLimit) ? parsedLimit : undefined,
    };

    setSubmitting(true);
    try {
      await onAdd(payload);
      setName('');
      setKey('');
      setCreditsLimit('');
      setEnabled(true);
    } finally {
      setSubmitting(false);
    }
  };

  const handleRename = async (item: KiroProxyApiKeyView) => {
    const nextName = window.prompt(
      t('kiroProxy.apiKeys.renamePrompt', 'New API key name'),
      item.name,
    );
    if (nextName === null || !nextName.trim()) {
      return;
    }
    await onUpdate({ id: item.id, name: nextName.trim() });
  };

  const handleSetLimit = async (item: KiroProxyApiKeyView) => {
    const input = window.prompt(
      t('kiroProxy.apiKeys.limitPrompt', 'Credits limit (number)'),
      item.creditsLimit != null ? String(item.creditsLimit) : '',
    );
    if (input === null) {
      return;
    }
    const parsed = Number.parseFloat(input.trim());
    if (!Number.isFinite(parsed) || parsed < 0) {
      return;
    }
    await onUpdate({ id: item.id, creditsLimit: parsed });
  };

  return (
    <section className="kiro-proxy-panel">
      <div className="kiro-proxy-panel-head">
        <h3>{t('kiroProxy.apiKeys.title', 'API Keys')}</h3>
      </div>

      <div className="kiro-proxy-inline-form">
        <input
          value={name}
          onChange={(event) => setName(event.target.value)}
          placeholder={t('kiroProxy.apiKeys.name', 'Key Name')}
          disabled={busy || submitting}
        />
        <input
          value={key}
          onChange={(event) => setKey(event.target.value)}
          placeholder={t('kiroProxy.apiKeys.value', 'sk-...')}
          disabled={busy || submitting}
        />
        <input
          value={creditsLimit}
          onChange={(event) => setCreditsLimit(event.target.value)}
          placeholder={t('kiroProxy.apiKeys.limit', 'Credits Limit')}
          disabled={busy || submitting}
          inputMode="decimal"
        />
        <label className="kiro-proxy-check">
          <input
            type="checkbox"
            checked={enabled}
            onChange={(event) => setEnabled(event.target.checked)}
            disabled={busy || submitting}
          />
          <span>{t('common.enabled', 'Enabled')}</span>
        </label>
        <button
          className="btn btn-primary btn-sm"
          onClick={() => void handleCreate()}
          disabled={busy || submitting || !key.trim()}
        >
          <Plus size={14} />
          {t('kiroProxy.apiKeys.add', 'Add')}
        </button>
      </div>

      <div className="kiro-proxy-table-wrap">
        <table className="kiro-proxy-table">
          <thead>
            <tr>
              <th>{t('kiroProxy.apiKeys.name', 'Key Name')}</th>
              <th>{t('kiroProxy.apiKeys.preview', 'Preview')}</th>
              <th>{t('kiroProxy.apiKeys.usage', 'Usage')}</th>
              <th>{t('kiroProxy.apiKeys.limit', 'Credits Limit')}</th>
              <th>{t('common.status', 'Status')}</th>
              <th>{t('common.shared.columns.actions', 'Actions')}</th>
            </tr>
          </thead>
          <tbody>
            {sorted.length === 0 ? (
              <tr>
                <td colSpan={6} className="kiro-proxy-empty-cell">
                  {t('kiroProxy.apiKeys.empty', 'No API keys')}
                </td>
              </tr>
            ) : (
              sorted.map((item) => (
                <tr key={item.id}>
                  <td>{item.name}</td>
                  <td>{item.keyPreview}</td>
                  <td>
                    {item.usage.totalRequests} / {item.usage.totalCredits.toFixed(2)}
                  </td>
                  <td>{item.creditsLimit != null ? item.creditsLimit.toFixed(2) : '-'}</td>
                  <td>
                    <span className={`kiro-proxy-status ${item.enabled ? 'is-ok' : 'is-off'}`}>
                      {item.enabled
                        ? t('common.enabled', 'Enabled')
                        : t('common.disabled', 'Disabled')}
                    </span>
                  </td>
                  <td className="kiro-proxy-actions">
                    <button
                      className="btn btn-secondary btn-sm"
                      onClick={() => void onUpdate({ id: item.id, enabled: !item.enabled })}
                      disabled={busy || submitting}
                    >
                      {item.enabled
                        ? t('common.disable', 'Disable')
                        : t('common.enable', 'Enable')}
                    </button>
                    <button
                      className="btn btn-secondary btn-sm"
                      onClick={() => void handleRename(item)}
                      disabled={busy || submitting}
                    >
                      {t('kiroProxy.apiKeys.rename', 'Rename')}
                    </button>
                    <button
                      className="btn btn-secondary btn-sm"
                      onClick={() => void handleSetLimit(item)}
                      disabled={busy || submitting}
                    >
                      {t('kiroProxy.apiKeys.setLimit', 'Set Limit')}
                    </button>
                    <button
                      className="btn btn-secondary btn-sm"
                      onClick={() => void onResetUsage(item.id)}
                      disabled={busy || submitting}
                    >
                      <RotateCcw size={12} />
                      {t('kiroProxy.apiKeys.resetUsage', 'Reset Usage')}
                    </button>
                    <button
                      className="btn btn-danger btn-sm"
                      onClick={() => void onDelete(item.id)}
                      disabled={busy || submitting}
                    >
                      <Trash2 size={12} />
                      {t('common.delete', 'Delete')}
                    </button>
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
