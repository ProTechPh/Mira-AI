import { useState, useEffect, useCallback } from 'react'
import { AccountManager } from './components/accounts'
import { Sidebar, type PageType } from './components/layout'
import { HomePage, AboutPage, SettingsPage, MachineIdPage, MiraSettingsPage, ProxyPage, MProxyPage } from './components/pages'
import { UpdateDialog } from './components/UpdateDialog'
import { CloseConfirmDialog } from './components/CloseConfirmDialog'
import { useAccountsStore } from './store/accounts'

function App(): React.JSX.Element {
  const [currentPage, setCurrentPage] = useState<PageType>('home')
  const [sidebarCollapsed, setSidebarCollapsed] = useState(true)
  
  const { 
    loadFromStorage, 
    startAutoTokenRefresh, 
    stopAutoTokenRefresh, 
    handleBackgroundRefreshResult, 
    handleBackgroundCheckResult,
    accounts,
    activeAccountId,
    setActiveAccount,
    checkAndRefreshExpiringTokens
  } = useAccountsStore()

  // 切换到下一个可用账户
  const switchToNextAccount = useCallback(() => {
    const activeAccounts = Array.from(accounts.values()).filter(acc => acc.status === 'active')
    if (activeAccounts.length <= 1) return

    const currentIndex = activeAccounts.findIndex(acc => acc.id === activeAccountId)
    const nextIndex = (currentIndex + 1) % activeAccounts.length
    setActiveAccount(activeAccounts[nextIndex].id)
  }, [accounts, activeAccountId, setActiveAccount])

  // 更新托盘账户信息
  const updateTrayInfo = useCallback(() => {
    // 更新账户列表
    const accountList = Array.from(accounts.values()).map(acc => ({
      id: acc.id,
      email: acc.email || 'Unknown',
      idp: acc.idp || 'Unknown',
      status: acc.status
    }))
    window.api.updateTrayAccountList(accountList)

    // 更新当前账户
    if (activeAccountId) {
      const activeAccount = accounts.get(activeAccountId)
      if (activeAccount) {
        window.api.updateTrayAccount({
          id: activeAccount.id,
          email: activeAccount.email || 'Unknown',
          idp: activeAccount.idp || 'Unknown',
          status: activeAccount.status,
          subscription: activeAccount.subscription?.title || undefined,
          usage: activeAccount.usage ? {
            usedCredits: activeAccount.usage.current || 0,
            totalCredits: activeAccount.usage.limit || 0,
            totalRequests: 0,
            successRequests: 0,
            failedRequests: 0
          } : undefined
        })
      } else {
        window.api.updateTrayAccount(null)
      }
    } else {
      window.api.updateTrayAccount(null)
    }
  }, [accounts, activeAccountId])
  
  // 应用启动时加载数据并启动自动刷新
  useEffect(() => {
    loadFromStorage().then(() => {
      startAutoTokenRefresh()
      // Sync window title with language preference
      const { language } = useAccountsStore.getState()
      const actualLang = language === 'auto' 
        ? (navigator.language.startsWith('zh') ? 'zh' : 'en')
        : language
      window.api.updateTrayLanguage(actualLang)
      
      // Set authenticated flag if there are any accounts
      const { accounts } = useAccountsStore.getState()
      window.api.setAuthenticated(accounts.size > 0)
    })
    
    return () => {
      stopAutoTokenRefresh()
    }
  }, [loadFromStorage, startAutoTokenRefresh, stopAutoTokenRefresh])

  // 账户变化时更新托盘信息和认证状态
  useEffect(() => {
    updateTrayInfo()
    // Update authenticated status based on account count
    window.api.setAuthenticated(accounts.size > 0)
  }, [updateTrayInfo, accounts])

  // 监听托盘刷新账户事件
  useEffect(() => {
    const unsubscribe = window.api.onTrayRefreshAccount(() => {
      checkAndRefreshExpiringTokens()
      updateTrayInfo()
    })
    return () => {
      unsubscribe()
    }
  }, [checkAndRefreshExpiringTokens, updateTrayInfo])

  // 监听托盘切换账户事件
  useEffect(() => {
    const unsubscribe = window.api.onTraySwitchAccount(() => {
      switchToNextAccount()
    })
    return () => {
      unsubscribe()
    }
  }, [switchToNextAccount])

  // 监听后台刷新结果
  useEffect(() => {
    const unsubscribe = window.api.onBackgroundRefreshResult((data) => {
      handleBackgroundRefreshResult(data)
    })
    return () => {
      unsubscribe()
    }
  }, [handleBackgroundRefreshResult])

  // 监听后台检查结果
  useEffect(() => {
    const unsubscribe = window.api.onBackgroundCheckResult((data) => {
      handleBackgroundCheckResult(data)
    })
    return () => {
      unsubscribe()
    }
  }, [handleBackgroundCheckResult])

  const renderPage = () => {
    switch (currentPage) {
      case 'home':
        return <HomePage />
      case 'accounts':
        return <AccountManager />
      case 'machineId':
        return <MachineIdPage />
      case 'kiroSettings':
        return <MiraSettingsPage />
      case 'proxy':
        return <ProxyPage />
      case 'kproxy':
        return <MProxyPage />
      case 'settings':
        return <SettingsPage />
      case 'about':
        return <AboutPage />
      default:
        return <HomePage />
    }
  }

  return (
    <div className="h-screen bg-background flex">
      <Sidebar
        currentPage={currentPage}
        onPageChange={setCurrentPage}
        collapsed={sidebarCollapsed}
        onToggleCollapse={() => setSidebarCollapsed(!sidebarCollapsed)}
      />
      <main className="flex-1 overflow-auto">
        {renderPage()}
      </main>
      <UpdateDialog />
      <CloseConfirmDialog />
    </div>
  )
}

export default App
