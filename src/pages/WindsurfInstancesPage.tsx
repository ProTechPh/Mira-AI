import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { PlatformInstancesContent } from '../components/platform/PlatformInstancesContent';
import { useWindsurfInstanceStore } from '../stores/useWindsurfInstanceStore';
import { useWindsurfAccountStore } from '../stores/useWindsurfAccountStore';
import type { WindsurfAccount } from '../types/windsurf';
import { getWindsurfAccountDisplayEmail, getWindsurfQuotaClass, getWindsurfUsage } from '../types/windsurf';
import { usePlatformRuntimeSupport } from '../hooks/usePlatformRuntimeSupport';

/**
 * Windsurf 多开实例内容组件（不包含 header）
 * 用于嵌入到 WindsurfAccountsPage 中
 */
export function WindsurfInstancesContent() {
  const { t } = useTranslation();
  const instanceStore = useWindsurfInstanceStore();
  const { accounts, fetchAccounts } = useWindsurfAccountStore();
  type AccountForSelect = WindsurfAccount & { email: string };
  const accountsForSelect = useMemo(
    () =>
      accounts.map((acc) => ({
        ...acc,
        email: acc.email || getWindsurfAccountDisplayEmail(acc),
      })) as AccountForSelect[],
    [accounts],
  );
  const isSupportedPlatform = usePlatformRuntimeSupport('desktop');

  const resolveQuotaClass = (percentage: number) => getWindsurfQuotaClass(percentage);

  const renderWindsurfQuotaPreview = (account: AccountForSelect) => {
    const usage = getWindsurfUsage(account);
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
      renderAccountQuotaPreview={renderWindsurfQuotaPreview}
      getAccountSearchText={(account) => account.email}
      appType="windsurf"
      isSupported={isSupportedPlatform}
      unsupportedTitleKey="common.shared.instances.unsupported.title"
      unsupportedTitleDefault="暂不支持当前系统"
      unsupportedDescKey="windsurf.instances.unsupported.descPlatform"
      unsupportedDescDefault="Windsurf 多开实例仅支持 macOS、Windows 和 Linux。"
    />
  );
}
