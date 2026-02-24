import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Plus, Save, Trash2 } from 'lucide-react';
import type { KiroProxyApiKeyView, KiroProxyModelMappingRule } from '../../services/kiroProxyService';

interface EditableMapping {
  id: string;
  name: string;
  enabled: boolean;
  type: string;
  sourceModel: string;
  targetModels: string;
  weights: string;
  priority: string;
  apiKeyIds: string;
}

interface ModelMappingsPanelProps {
  mappings: KiroProxyModelMappingRule[];
  apiKeys: KiroProxyApiKeyView[];
  busy: boolean;
  onSave: (mappings: KiroProxyModelMappingRule[]) => Promise<void>;
}

function normalizeCsv(value: string): string[] {
  return value
    .split(',')
    .map((item) => item.trim())
    .filter((item) => item.length > 0);
}

function mappingToEditable(mapping: KiroProxyModelMappingRule): EditableMapping {
  return {
    id: mapping.id,
    name: mapping.name,
    enabled: mapping.enabled,
    type: mapping.type || 'replace',
    sourceModel: mapping.sourceModel,
    targetModels: mapping.targetModels.join(', '),
    weights: mapping.weights.join(', '),
    priority: String(mapping.priority),
    apiKeyIds: mapping.apiKeyIds.join(', '),
  };
}

export function ModelMappingsPanel({
  mappings,
  apiKeys,
  busy,
  onSave,
}: ModelMappingsPanelProps) {
  const { t } = useTranslation();
  const [rows, setRows] = useState<EditableMapping[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setRows(mappings.map(mappingToEditable));
  }, [mappings]);

  const apiKeyIdSet = useMemo(() => new Set(apiKeys.map((item) => item.id)), [apiKeys]);

  const updateRow = (id: string, patch: Partial<EditableMapping>) => {
    setRows((prev) => prev.map((row) => (row.id === id ? { ...row, ...patch } : row)));
  };

  const handleAdd = () => {
    const id = (globalThis.crypto?.randomUUID?.() || `${Date.now()}-${Math.random()}`).replace(
      /[^a-zA-Z0-9-_]/g,
      '',
    );
    setRows((prev) => [
      ...prev,
      {
        id,
        name: `Rule ${prev.length + 1}`,
        enabled: true,
        type: 'replace',
        sourceModel: '*',
        targetModels: 'claude-sonnet-4.5',
        weights: '',
        priority: '100',
        apiKeyIds: '',
      },
    ]);
  };

  const handleSave = async () => {
    const payload: KiroProxyModelMappingRule[] = rows.map((row) => {
      const weights = normalizeCsv(row.weights)
        .map((item) => Number.parseFloat(item))
        .filter((item) => Number.isFinite(item) && item > 0);
      const priority = Number.parseInt(row.priority, 10);
      const apiKeyIds = normalizeCsv(row.apiKeyIds).filter((id) => apiKeyIdSet.has(id));
      return {
        id: row.id,
        name: row.name.trim() || row.id,
        enabled: row.enabled,
        type: row.type.trim() || 'replace',
        sourceModel: row.sourceModel.trim() || '*',
        targetModels: normalizeCsv(row.targetModels),
        weights,
        priority: Number.isFinite(priority) ? priority : 100,
        apiKeyIds,
      };
    });

    setSaving(true);
    try {
      await onSave(payload);
    } finally {
      setSaving(false);
    }
  };

  return (
    <section className="kiro-proxy-panel">
      <div className="kiro-proxy-panel-head">
        <h3>{t('kiroProxy.mappings.title', 'Model Mapping')}</h3>
        <div className="kiro-proxy-actions">
          <button className="btn btn-secondary btn-sm" onClick={handleAdd} disabled={busy || saving}>
            <Plus size={14} />
            {t('common.add', 'Add')}
          </button>
          <button
            className="btn btn-primary btn-sm"
            onClick={() => void handleSave()}
            disabled={busy || saving}
          >
            <Save size={14} />
            {t('common.save', 'Save')}
          </button>
        </div>
      </div>

      <div className="kiro-proxy-mapping-list">
        {rows.length === 0 ? (
          <div className="kiro-proxy-empty">{t('kiroProxy.mappings.empty', 'No mapping rules')}</div>
        ) : (
          rows.map((row) => (
            <div className="kiro-proxy-mapping-card" key={row.id}>
              <div className="kiro-proxy-inline-form">
                <input
                  value={row.name}
                  onChange={(event) => updateRow(row.id, { name: event.target.value })}
                  placeholder={t('kiroProxy.mappings.name', 'Rule Name')}
                />
                <select
                  value={row.type}
                  onChange={(event) => updateRow(row.id, { type: event.target.value })}
                >
                  <option value="replace">replace</option>
                  <option value="loadbalance">loadbalance</option>
                </select>
                <label className="kiro-proxy-check">
                  <input
                    type="checkbox"
                    checked={row.enabled}
                    onChange={(event) => updateRow(row.id, { enabled: event.target.checked })}
                  />
                  <span>{t('common.enabled', 'Enabled')}</span>
                </label>
                <input
                  value={row.priority}
                  onChange={(event) => updateRow(row.id, { priority: event.target.value })}
                  placeholder={t('kiroProxy.mappings.priority', 'Priority')}
                  inputMode="numeric"
                />
                <button
                  className="btn btn-danger btn-sm"
                  onClick={() => setRows((prev) => prev.filter((item) => item.id !== row.id))}
                >
                  <Trash2 size={12} />
                  {t('common.delete', 'Delete')}
                </button>
              </div>
              <div className="kiro-proxy-inline-form">
                <input
                  value={row.sourceModel}
                  onChange={(event) => updateRow(row.id, { sourceModel: event.target.value })}
                  placeholder={t('kiroProxy.mappings.source', 'Source model (supports *)')}
                />
                <input
                  value={row.targetModels}
                  onChange={(event) => updateRow(row.id, { targetModels: event.target.value })}
                  placeholder={t('kiroProxy.mappings.targets', 'Target models, comma separated')}
                />
              </div>
              <div className="kiro-proxy-inline-form">
                <input
                  value={row.weights}
                  onChange={(event) => updateRow(row.id, { weights: event.target.value })}
                  placeholder={t('kiroProxy.mappings.weights', 'Weights, comma separated')}
                />
                <input
                  value={row.apiKeyIds}
                  onChange={(event) => updateRow(row.id, { apiKeyIds: event.target.value })}
                  placeholder={t('kiroProxy.mappings.scopedKeys', 'API key IDs, comma separated')}
                />
              </div>
            </div>
          ))
        )}
      </div>
    </section>
  );
}
