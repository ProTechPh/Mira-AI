import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import type { KiroProxyAccountView } from '../../services/kiroProxyService';

interface AccountSelectionPanelProps {
  accounts: KiroProxyAccountView[];
  selectedIds: string[];
  enableMultiAccount: boolean;
  onToggleAccount: (id: string) => void;
  onToggleMultiAccount: (enabled: boolean) => void;
}

function formatCooldown(ts?: number | null): string {
  if (!ts) return '-';
  const now = Math.floor(Date.now() / 1000);
  const diff = ts - now;
  if (diff <= 0) return 'ready';
  if (diff < 60) return `${diff}s`;
  if (diff < 3600) return `${Math.ceil(diff / 60)}m`;
  return `${Math.ceil(diff / 3600)}h`;
}

export function AccountSelectionPanel({
  accounts,
  selectedIds,
  enableMultiAccount,
  onToggleAccount,
  onToggleMultiAccount,
}: AccountSelectionPanelProps) {
  const { t } = useTranslation();
  const selectedSet = useMemo(() => new Set(selectedIds), [selectedIds]);

  return (
    <section className="kiro-proxy-panel">
      <div className="kiro-proxy-panel-head">
        <h3>{t('kiroProxy.accounts.title', 'Account Pool')}</h3>
        <label className="kiro-proxy-check">
          <input
            type="checkbox"
            checked={enableMultiAccount}
            onChange={(event) => onToggleMultiAccount(event.target.checked)}
          />
          <span>{t('kiroProxy.accounts.multi', 'Enable multi-account routing')}</span>
        </label>
      </div>

      <div className="kiro-proxy-account-list">
        {accounts.length === 0 ? (
          <div className="kiro-proxy-empty">{t('kiroProxy.accounts.empty', 'No Kiro accounts found')}</div>
        ) : (
          accounts.map((account) => {
            const selected = selectedSet.has(account.id);
            const effectiveSelected = selectedIds.length === 0 ? true : selected;
            return (
              <label className="kiro-proxy-account-item" key={account.id}>
                <input
                  type="checkbox"
                  checked={selected}
                  onChange={() => onToggleAccount(account.id)}
                />
                <span className="kiro-proxy-account-main">
                  <strong>{account.email}</strong>
                  <small>{account.id}</small>
                </span>
                <span className={`kiro-proxy-status ${effectiveSelected ? 'is-ok' : 'is-off'}`}>
                  {effectiveSelected
                    ? t('kiroProxy.accounts.inPool', 'In pool')
                    : t('kiroProxy.accounts.excluded', 'Excluded')}
                </span>
                <span className="kiro-proxy-account-meta">
                  {t('kiroProxy.accounts.cooldown', 'Cooldown')}: {formatCooldown(account.cooldownUntil)}
                </span>
              </label>
            );
          })
        )}
      </div>
    </section>
  );
}
