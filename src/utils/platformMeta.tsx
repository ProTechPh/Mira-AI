import { ReactNode } from 'react';
import { Github } from 'lucide-react';
import { TFunction } from 'i18next';
import { PlatformId } from '../types/platform';
import { RobotIcon } from '../components/icons/RobotIcon';
import { CodexIcon } from '../components/icons/CodexIcon';
import { WindsurfIcon } from '../components/icons/WindsurfIcon';
import { KiroIcon } from '../components/icons/KiroIcon';

export function getPlatformLabel(platformId: PlatformId, _t: TFunction): string {
  switch (platformId) {
    case 'antigravity':
      return 'Antigravity';
    case 'codex':
      return 'Codex';
    case 'github-copilot':
      return 'GitHub Copilot';
    case 'windsurf':
      return 'Windsurf';
    case 'kiro':
      return 'Kiro';
    default:
      return platformId;
  }
}

export function renderPlatformIcon(platformId: PlatformId, size = 20): ReactNode {
  switch (platformId) {
    case 'antigravity':
      return <RobotIcon style={{ width: size, height: size }} />;
    case 'codex':
      return <CodexIcon size={size} />;
    case 'github-copilot':
      return <Github size={size} />;
    case 'windsurf':
      return <WindsurfIcon style={{ width: size, height: size }} />;
    case 'kiro':
      return <KiroIcon style={{ width: size, height: size }} />;
    default:
      return null;
  }
}
