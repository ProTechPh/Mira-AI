import { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { AlarmClock, Fingerprint, Layers } from 'lucide-react';
import { Page } from '../types/navigation';
import { RobotIcon } from './icons/RobotIcon';

interface OverviewTabsHeaderProps {
  active: Page;
  onNavigate?: (page: Page) => void;
  subtitle: string;
  title?: string;
}

interface TabSpec {
  key: Page;
  label: string;
  icon: ReactNode;
}

export function OverviewTabsHeader({
  active,
  onNavigate,
  subtitle,
  title,
}: OverviewTabsHeaderProps) {
  const { t } = useTranslation();
  const tabs: TabSpec[] = [
    {
      key: 'overview',
      label: t('overview.title'),
      icon: <RobotIcon className="tab-icon" />,
    },
    {
      key: 'instances',
      label: t('instances.title', '多开实例'),
      icon: <Layers className="tab-icon" />,
    },
    {
      key: 'fingerprints',
      label: t('fingerprints.title'),
      icon: <Fingerprint className="tab-icon" />,
    },
    {
      key: 'wakeup',
      label: t('wakeup.title'),
      icon: <AlarmClock className="tab-icon" />,
    },
  ];

  return (
    <>
      <div className="page-header">
        <div className="page-title">{title ?? t('overview.brandTitle')}</div>
        <div className="page-subtitle">{subtitle}</div>
      </div>
      <div className="page-tabs-row page-tabs-center">
        <div className="page-tabs filter-tabs">
          {tabs.map((tab) => (
            <button
              key={tab.key}
              className={`filter-tab${active === tab.key ? ' active' : ''}`}
              onClick={() => onNavigate?.(tab.key)}
            >
              {tab.icon}
              <span>{tab.label}</span>
            </button>
          ))}
        </div>
      </div>
    </>
  );
}
