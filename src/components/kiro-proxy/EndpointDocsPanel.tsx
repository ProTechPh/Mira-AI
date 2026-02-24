import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';

interface EndpointDocsPanelProps {
  host: string;
  port: number;
}

export function EndpointDocsPanel({ host, port }: EndpointDocsPanelProps) {
  const { t } = useTranslation();
  const base = useMemo(() => `http://${host}:${port}`, [host, port]);

  return (
    <section className="kiro-proxy-panel">
      <div className="kiro-proxy-panel-head">
        <h3>{t('kiroProxy.endpoints.title', 'Endpoint Docs')}</h3>
      </div>
      <div className="kiro-proxy-docs">
        <p>
          {t('kiroProxy.endpoints.base', 'Base URL')}: <code>{base}</code>
        </p>
        <p>
          <code>POST {base}/v1/chat/completions</code>
        </p>
        <p>
          <code>GET {base}/v1/models</code>
        </p>
        <p>
          <code>POST {base}/v1/messages</code>
        </p>
        <p>
          <code>GET {base}/admin/stats</code>
        </p>
        <pre>
{`curl ${base}/v1/models \\
  -H "Authorization: Bearer YOUR_PROXY_KEY"`}
        </pre>
      </div>
    </section>
  );
}
