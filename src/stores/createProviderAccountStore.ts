import { create } from 'zustand';

type ProviderUsage = {
  inlineSuggestionsUsedPercent: number | null;
  chatMessagesUsedPercent: number | null;
  allowanceResetAt?: number | null;
  remainingCompletions?: number | null;
  remainingChat?: number | null;
  totalCompletions?: number | null;
  totalChat?: number | null;
};

type ProviderAccountAugmentation = {
  email?: string;
  plan_type?: string;
  quota?: unknown;
};

type ProviderService<TAccount> = {
  listAccounts: () => Promise<TAccount[]>;
  deleteAccount: (accountId: string) => Promise<void>;
  deleteAccounts: (accountIds: string[]) => Promise<void>;
  injectAccount: (accountId: string) => Promise<unknown>;
  refreshToken: (accountId: string) => Promise<unknown>;
  refreshAllTokens: () => Promise<unknown>;
  importFromJson: (jsonContent: string) => Promise<TAccount[]>;
  exportAccounts: (accountIds: string[]) => Promise<string>;
  updateAccountTags: (accountId: string, tags: string[]) => Promise<TAccount>;
};

type ProviderMapper<TAccount> = {
  getDisplayEmail: (account: TAccount) => string;
  getPlanBadge: (account: TAccount) => string;
  getUsage: (account: TAccount) => ProviderUsage;
};

export interface ProviderAccountStoreState<TAccount> {
  accounts: TAccount[];
  loading: boolean;
  error: string | null;
  fetchAccounts: () => Promise<void>;
  switchAccount: (accountId: string) => Promise<void>;
  deleteAccounts: (accountIds: string[]) => Promise<void>;
  refreshToken: (accountId: string) => Promise<void>;
  refreshAllTokens: () => Promise<void>;
  importFromJson: (jsonContent: string) => Promise<TAccount[]>;
  exportAccounts: (accountIds: string[]) => Promise<string>;
  updateAccountTags: (accountId: string, tags: string[]) => Promise<TAccount>;
}

export function createProviderAccountStore<TAccount extends ProviderAccountAugmentation>(
  cacheKey: string,
  service: ProviderService<TAccount>,
  mapper: ProviderMapper<TAccount>,
) {
  const loadCachedAccounts = (): TAccount[] => {
    try {
      const raw = localStorage.getItem(cacheKey);
      if (!raw) return [];
      const parsed = JSON.parse(raw);
      return Array.isArray(parsed) ? (parsed as TAccount[]) : [];
    } catch {
      return [];
    }
  };

  const persistAccountsCache = (accounts: TAccount[]) => {
    try {
      localStorage.setItem(cacheKey, JSON.stringify(accounts));
    } catch {
      // ignore cache write failures
    }
  };

  const mapAccountsForUnifiedView = (accounts: TAccount[]): TAccount[] => {
    return accounts.map((account) => {
      const email = mapper.getDisplayEmail(account);
      const usage = mapper.getUsage(account);
      const hourlyPct =
        usage.inlineSuggestionsUsedPercent ?? usage.chatMessagesUsedPercent;
      const weeklyPct =
        usage.chatMessagesUsedPercent ?? usage.inlineSuggestionsUsedPercent;
      const quota =
        hourlyPct == null && weeklyPct == null
          ? undefined
          : {
              hourly_percentage: hourlyPct ?? 0,
              weekly_percentage: weeklyPct ?? 0,
              hourly_reset_time: usage.allowanceResetAt ?? null,
              weekly_reset_time: usage.allowanceResetAt ?? null,
              raw_data: {
                remainingCompletions: usage.remainingCompletions,
                remainingChat: usage.remainingChat,
                totalCompletions: usage.totalCompletions,
                totalChat: usage.totalChat,
              },
            };

      return {
        ...account,
        email,
        plan_type: mapper.getPlanBadge(account),
        quota,
      };
    });
  };

  return create<ProviderAccountStoreState<TAccount>>((set, get) => ({
    accounts: loadCachedAccounts(),
    loading: false,
    error: null,

    fetchAccounts: async () => {
      set({ loading: true, error: null });
      try {
        const accounts = await service.listAccounts();
        const mapped = mapAccountsForUnifiedView(accounts);
        set({ accounts: mapped, loading: false });
        persistAccountsCache(mapped);
      } catch (e) {
        set({ error: String(e), loading: false });
      }
    },

    deleteAccounts: async (accountIds: string[]) => {
      if (accountIds.length === 0) return;
      if (accountIds.length === 1) {
        await service.deleteAccount(accountIds[0]);
      } else {
        await service.deleteAccounts(accountIds);
      }
      await get().fetchAccounts();
    },

    switchAccount: async (accountId: string) => {
      await service.injectAccount(accountId);
      await get().fetchAccounts();
    },

    refreshToken: async (accountId: string) => {
      await service.refreshToken(accountId);
      await get().fetchAccounts();
    },

    refreshAllTokens: async () => {
      await service.refreshAllTokens();
      await get().fetchAccounts();
    },

    importFromJson: async (jsonContent: string) => {
      const accounts = await service.importFromJson(jsonContent);
      await get().fetchAccounts();
      return accounts;
    },

    exportAccounts: async (accountIds: string[]) => {
      return await service.exportAccounts(accountIds);
    },

    updateAccountTags: async (accountId: string, tags: string[]) => {
      const account = await service.updateAccountTags(accountId, tags);
      await get().fetchAccounts();
      return account;
    },
  }));
}
