import {
  WindsurfAccount,
  getWindsurfAccountDisplayEmail,
  getWindsurfPlanBadge,
  getWindsurfUsage,
} from '../types/windsurf';
import * as windsurfService from '../services/windsurfService';
import { createProviderAccountStore } from './createProviderAccountStore';

const WINDSURF_ACCOUNTS_CACHE_KEY = 'agtools.windsurf.accounts.cache';

export const useWindsurfAccountStore = createProviderAccountStore<WindsurfAccount>(
  WINDSURF_ACCOUNTS_CACHE_KEY,
  {
    listAccounts: windsurfService.listWindsurfAccounts,
    deleteAccount: windsurfService.deleteWindsurfAccount,
    deleteAccounts: windsurfService.deleteWindsurfAccounts,
    injectAccount: windsurfService.injectWindsurfToVSCode,
    refreshToken: windsurfService.refreshWindsurfToken,
    refreshAllTokens: windsurfService.refreshAllWindsurfTokens,
    importFromJson: windsurfService.importWindsurfFromJson,
    exportAccounts: windsurfService.exportWindsurfAccounts,
    updateAccountTags: windsurfService.updateWindsurfAccountTags,
  },
  {
    getDisplayEmail: getWindsurfAccountDisplayEmail,
    getPlanBadge: getWindsurfPlanBadge,
    getUsage: getWindsurfUsage,
  },
);
