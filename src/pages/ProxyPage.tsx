import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { AntigravityProxyPage } from './AntigravityProxyPage';
import { KiroProxyPage } from './KiroProxyPage';
import './proxyPage.css';

type ProxyKind = 'antigravity' | 'kiro';

export function ProxyPage() {
  const { t } = useTranslation();
  const [kind, setKind] = useState<ProxyKind>('antigravity');

  const title = useMemo(() => {
    if (kind === 'kiro') return t('nav.kiroProxy', 'Kiro Proxy');
    return t('nav.antigravityProxy', 'Antigravity Proxy');
  }, [kind, t]);

  return (
    <div className="proxy-page-wrap">
      <div className="proxy-page-switcher">
        <div className="proxy-page-title">{t('nav.proxy', 'Proxy')}</div>
        <div className="proxy-page-segment" role="tablist" aria-label="Proxy selector">
          <button
            type="button"
            role="tab"
            aria-selected={kind === 'antigravity'}
            className={kind === 'antigravity' ? 'active' : ''}
            onClick={() => setKind('antigravity')}
          >
            {t('nav.antigravityProxy', 'Antigravity Proxy')}
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={kind === 'kiro'}
            className={kind === 'kiro' ? 'active' : ''}
            onClick={() => setKind('kiro')}
          >
            {t('nav.kiroProxy', 'Kiro Proxy')}
          </button>
        </div>
        <div className="proxy-page-subtitle">{title}</div>
      </div>

      {kind === 'antigravity' ? <AntigravityProxyPage /> : <KiroProxyPage />}
    </div>
  );
}
