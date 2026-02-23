import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { PlatformInstancesContent } from '../components/platform/PlatformInstancesContent';
import { useKiroInstanceStore } from '../stores/useKiroInstanceStore';
import { useKiroAccountStore } from '../stores/useKiroAccountStore';
import type { KiroAccount } from '../types/kiro';
import { getKiroAccountDisplayEmail, getKiroQuotaClass, getKiroUsage } from '../types/kiro';
import { usePlatformRuntimeSupport } from '../hooks/usePlatformRuntimeSupport';

/**
 * Kiro 多开实例内容组件（不包含 header）
 * 用于嵌入到 KiroAccountsPage 中
 */
export function KiroInstancesContent() {
  const { t } = useTranslation();
  const instanceStore = useKiroInstanceStore();
  const { accounts, fetchAccounts } = useKiroAccountStore();
  type AccountForSelect = KiroAccount & { email: string };
  const accountsForSelect = useMemo(
    () =>
      accounts.map((acc) => ({
        ...acc,
        email: acc.email || getKiroAccountDisplayEmail(acc),
      })) as AccountForSelect[],
    [accounts],
  );
  const isSupportedPlatform = usePlatformRuntimeSupport('desktop');

  const resolveQuotaClass = (percentage: number) => getKiroQuotaClass(percentage);

  const renderKiroQuotaPreview = (account: AccountForSelect) => {
    const usage = getKiroUsage(account);
    const inlinePct = usage.inlineSuggestionsUsedPercent;
    const chatPct = usage.chatMessagesUsedPercent;
    if (inlinePct == null && chatPct == null) {
      return <span className="account-quota-empty">{t('instances.quota.empty', '暂无配额缓存')}</span>;
    }
    return (
      <div className="account-quota-preview">
        <span className="account-quota-item">
          <span className={`quota-dot ${resolveQuotaClass(inlinePct ?? 0)}`} />
          <span className={`quota-text ${resolveQuotaClass(inlinePct ?? 0)}`}>
            {t('common.shared.instances.quota.inline', 'Inline Suggestions')} {inlinePct ?? '-'}%
          </span>
        </span>
        <span className="account-quota-item">
          <span className={`quota-dot ${resolveQuotaClass(chatPct ?? 0)}`} />
          <span className={`quota-text ${resolveQuotaClass(chatPct ?? 0)}`}>
            {t('common.shared.instances.quota.chat', 'Chat messages')} {chatPct ?? '-'}%
          </span>
        </span>
      </div>
    );
  };

  return (
    <PlatformInstancesContent<AccountForSelect>
      instanceStore={instanceStore}
      accounts={accountsForSelect}
      fetchAccounts={fetchAccounts}
      renderAccountQuotaPreview={renderKiroQuotaPreview}
      getAccountSearchText={(account) => account.email}
      appType="kiro"
      isSupported={isSupportedPlatform}
      unsupportedTitleKey="common.shared.instances.unsupported.title"
      unsupportedTitleDefault="暂不支持当前系统"
      unsupportedDescKey="kiro.instances.unsupported.descPlatform"
      unsupportedDescDefault="Kiro 多开实例仅支持 macOS、Windows 和 Linux。"
    />
  );
}
