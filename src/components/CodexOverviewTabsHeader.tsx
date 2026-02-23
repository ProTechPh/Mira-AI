import { PlatformOverviewTabsHeader, PlatformOverviewTab } from './platform/PlatformOverviewTabsHeader';

export type CodexTab = PlatformOverviewTab;

interface CodexOverviewTabsHeaderProps {
  active: CodexTab;
  onTabChange?: (tab: CodexTab) => void;
}

export function CodexOverviewTabsHeader({
  active,
  onTabChange,
}: CodexOverviewTabsHeaderProps) {
  return <PlatformOverviewTabsHeader platform="codex" active={active} onTabChange={onTabChange} />;
}
