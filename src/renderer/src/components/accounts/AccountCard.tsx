import { memo, useState, useMemo } from 'react'
import { createPortal } from 'react-dom'
import { Card, CardContent, Badge, Button, Progress } from '../ui'
import { useAccountsStore } from '@/store/accounts'
import { useTranslation } from '@/hooks/useTranslation'
import type { Account, AccountTag, AccountGroup } from '@/types/account'
import {
  Check,
  RefreshCw,
  Trash2,
  Edit,
  Copy,
  AlertTriangle,
  Clock,
  Loader2,
  Info,
  FolderOpen,
  Power,
  Calendar,
  AlertCircle,
  KeyRound,
  X,
  ExternalLink,
  CreditCard,
  Sparkles,
  LogOut
} from 'lucide-react'
import { cn } from '@/lib/utils'

// 解析 ARGB 颜色转换为 CSS rgba
function toRgba(argbColor: string): string {
  // 支持格式: #AARRGGBB 或 #RRGGBB
  let alpha = 255
  let rgb = argbColor
  if (argbColor.length === 9 && argbColor.startsWith('#')) {
    alpha = parseInt(argbColor.slice(1, 3), 16)
    rgb = '#' + argbColor.slice(3)
  }
  const hex = rgb.startsWith('#') ? rgb.slice(1) : rgb
  const r = parseInt(hex.slice(0, 2), 16)
  const g = parseInt(hex.slice(2, 4), 16)
  const b = parseInt(hex.slice(4, 6), 16)
  return `rgba(${r}, ${g}, ${b}, ${alpha / 255})`
}

// 生成标签光环样式
function generateGlowStyle(tagColors: string[]): React.CSSProperties {
  if (tagColors.length === 0) return {}
  
  if (tagColors.length === 1) {
    const color = toRgba(tagColors[0])
    const colorTransparent = color.replace('1)', '0.15)') // 降低阴影透明度
    return {
      boxShadow: `0 0 0 1px ${color}, 0 4px 12px -2px ${colorTransparent}`
    }
  }
  
  // 多个标签时，使用渐变边框效果
  const gradientColors = tagColors.map((c, i) => {
    const percent = (i / tagColors.length) * 100
    const nextPercent = ((i + 1) / tagColors.length) * 100
    return `${toRgba(c)} ${percent}%, ${toRgba(c)} ${nextPercent}%`
  }).join(', ')
  
  return {
    background: `linear-gradient(white, white) padding-box, linear-gradient(135deg, ${gradientColors}) border-box`,
    border: '1.5px solid transparent',
    boxShadow: '0 4px 12px -2px rgba(0, 0, 0, 0.05)'
  }
}

interface AccountCardProps {
  account: Account
  tags: Map<string, AccountTag>
  groups: Map<string, AccountGroup>
  isSelected: boolean
  onSelect: () => void
  onEdit: () => void
  onShowDetail: () => void
}

const getSubscriptionColor = (type: string, title?: string): string => {
  const text = (title || type).toUpperCase()
  // KIRO PRO+ / PRO_PLUS - 紫色
  if (text.includes('PRO+') || text.includes('PRO_PLUS') || text.includes('PROPLUS')) return 'bg-purple-500'
  // KIRO POWER - 金色
  if (text.includes('POWER')) return 'bg-amber-500'
  // KIRO PRO - 蓝色
  if (text.includes('PRO')) return 'bg-blue-500'
  return 'bg-gray-500'
}

// 获取账户显示名称：昵称优先，无则邮箱，无邮箱则 userId
function getDisplayName(account: Account): string {
  if (account.nickname) return account.nickname
  if (account.email) return account.email
  if (account.userId) return account.userId
  return 'Unknown'
}

// 格式化 Token 到期时间
function formatTokenExpiry(expiresAt: number, t: (key: string, params?: Record<string, any>) => string): string {
  const now = Date.now()
  const diff = expiresAt - now
  
  if (diff <= 0) return t('time.expired')
  
  const minutes = Math.floor(diff / (60 * 1000))
  const hours = Math.floor(diff / (60 * 60 * 1000))
  
  if (minutes < 60) {
    return t('time.minutesShort', { n: minutes })
  } else if (hours < 24) {
    const remainingMinutes = minutes % 60
    return remainingMinutes > 0 
      ? t('time.hoursMinutesShort', { h: hours, m: remainingMinutes })
      : t('time.hoursShort', { n: hours })
  } else {
    const days = Math.floor(hours / 24)
    const remainingHours = hours % 24
    return remainingHours > 0
      ? t('time.daysHoursShort', { d: days, h: remainingHours })
      : t('time.daysShort', { n: days })
  }
}

export const AccountCard = memo(function AccountCard({
  account,
  tags,
  groups,
  isSelected,
  onSelect,
  onEdit,
  onShowDetail
}: AccountCardProps) {
  const {
    setActiveAccount,
    removeAccount,
    checkAccountStatus,
    refreshAccountToken,
    toggleSelection,
    maskEmail,
    maskNickname,
    usagePrecision
  } = useAccountsStore()

  const { t } = useTranslation()
  const isEn = t('common.unknown') === 'Unknown'

  // 格式化使用量数值
  const formatUsage = (value: number): string => {
    if (usagePrecision) {
      return value.toLocaleString(undefined, { minimumFractionDigits: 0, maximumFractionDigits: 2 })
    }
    return Math.floor(value).toLocaleString()
  }

  const handleSwitch = async (): Promise<void> => {
    const { credentials } = account
    
    // 社交登录只需要 refreshToken，IdC 登录需要 clientId 和 clientSecret
    if (!credentials.refreshToken) {
      alert(t('accountCard.incompleteCredentials'))
      return
    }
    if (credentials.authMethod !== 'social' && (!credentials.clientId || !credentials.clientSecret)) {
      alert(t('accountCard.incompleteCredentials'))
      return
    }
    
    // 写入凭证到本地 SSO 缓存
    const result = await window.api.switchAccount({
      accessToken: credentials.accessToken,
      refreshToken: credentials.refreshToken,
      clientId: credentials.clientId || '',
      clientSecret: credentials.clientSecret || '',
      region: credentials.region || 'us-east-1',
      startUrl: credentials.startUrl,
      authMethod: credentials.authMethod,
      provider: credentials.provider
    })
    
    if (result.success) {
      setActiveAccount(account.id)
    } else {
      alert(t('messages.switchFailed', { error: result.error || 'Unknown error' }))
    }
  }

  const handleRefresh = async (): Promise<void> => {
    // 获取最新的使用量数据
    await checkAccountStatus(account.id)
  }

  const handleLogout = async (): Promise<void> => {
    if (!confirm(t('confirm.logoutClearCache'))) {
      return
    }
    
    const result = await window.api.logoutAccount()
    if (result.success) {
      // 取消当前账号的激活状态
      setActiveAccount(null)
      alert(t('messages.logoutSuccess', { count: result.deletedCount || 0 }))
    } else {
      alert(t('messages.logoutFailed', { error: result.error || 'Unknown error' }))
    }
  }

  const [isRefreshingToken, setIsRefreshingToken] = useState(false)
  const handleRefreshToken = async (): Promise<void> => {
    setIsRefreshingToken(true)
    try {
      await refreshAccountToken(account.id)
    } finally {
      setIsRefreshingToken(false)
    }
  }

  const handleDelete = (): void => {
    if (confirm(t('messages.deleteAccountConfirm', { name: getDisplayName(account) }))) {
      removeAccount(account.id)
    }
  }

  const [copied, setCopied] = useState(false)
  const [emailCopied, setEmailCopied] = useState(false)

  const handleCopyCredentials = (): void => {
    const credentials = {
      accessToken: account.credentials.accessToken,
      refreshToken: account.credentials.refreshToken,
      clientId: account.credentials.clientId,
      clientSecret: account.credentials.clientSecret
    }
    navigator.clipboard.writeText(JSON.stringify(credentials, null, 2))
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  const accountTags = account.tags
    .map((id) => tags.get(id))
    .filter((t): t is AccountTag => t !== undefined)

  // 获取分组信息
  const accountGroup = account.groupId ? groups.get(account.groupId) : undefined

  // 生成光环样式
  const glowStyle = useMemo(() => {
    const tagColors = accountTags.map(t => t.color)
    return generateGlowStyle(tagColors)
  }, [accountTags])

  const isExpiringSoon = account.subscription.daysRemaining !== undefined &&
                         account.subscription.daysRemaining <= 7

  const isHighUsage = account.usage.percentUsed > 80

  // 检测账号是否被封禁/暂停（多种错误格式）
  const isUnauthorized = account.lastError?.includes('UnauthorizedException') || 
                         account.lastError?.includes('AccountSuspendedException') ||
                         account.lastError?.includes('账户已封禁') ||
                         account.lastError?.includes('HTTP 403') ||
                         account.lastError?.includes('HTTP 423')
  
  // 封禁详情弹窗状态
  const [showBanDialog, setShowBanDialog] = useState(false)
  
  // 订阅管理弹窗状态
  const [showSubscriptionDialog, setShowSubscriptionDialog] = useState(false)
  const [subscriptionLoading, setSubscriptionLoading] = useState(false)
  const [subscriptionPlans, setSubscriptionPlans] = useState<Array<{
    name: string
    qSubscriptionType: string
    description: { title: string; billingInterval: string; featureHeader: string; features: string[] }
    pricing: { amount: number; currency: string }
  }>>([])
  const [selectedPlan, setSelectedPlan] = useState<string | null>(null)
  const [paymentLoading, setPaymentLoading] = useState(false)

  // 是否为首次用户（需要选择订阅类型）
  const [isFirstTimeUser, setIsFirstTimeUser] = useState(false)
  // 订阅错误信息
  const [subscriptionError, setSubscriptionError] = useState<string | null>(null)
  // 订阅成功提示
  const [subscriptionSuccess, setSubscriptionSuccess] = useState<string | null>(null)

  // 点击订阅标签打开订阅管理
  const handleSubscriptionClick = async (e: React.MouseEvent): Promise<void> => {
    e.stopPropagation()
    if (subscriptionLoading || !account.credentials?.accessToken) return
    
    setSubscriptionLoading(true)
    try {
      // 统一先获取可用订阅列表
      const result = await window.api.accountGetSubscriptions(account.credentials.accessToken, account.credentials?.region)
      if (result.success && result.plans.length > 0) {
        setSubscriptionPlans(result.plans)
        // 检查是否是首次用户（当前订阅类型为 FREE 或无订阅）
        const currentType = account.subscription.type?.toUpperCase() || ''
        const isFirstTime = currentType === '' || currentType.includes('FREE')
        setIsFirstTimeUser(isFirstTime)
        setShowSubscriptionDialog(true)
      } else {
        console.error('[AccountCard] Failed to get subscriptions:', result.error)
      }
    } catch (error) {
      console.error('[AccountCard] Subscription click error:', error)
    } finally {
      setSubscriptionLoading(false)
    }
  }

  // 选择订阅计划并获取支付链接
  const handleSelectPlan = async (planName: string): Promise<void> => {
    if (paymentLoading || !account.credentials?.accessToken) return
    
    setSelectedPlan(planName)
    setPaymentLoading(true)
    setSubscriptionError(null)
    try {
      const result = await window.api.accountGetSubscriptionUrl(account.credentials.accessToken, planName, account.credentials?.region)
      if (result.success && result.url) {
        // 自动复制链接到剪贴板
        await navigator.clipboard.writeText(result.url)
        // 显示复制成功提示
        setSubscriptionSuccess(t('accountCard.linkCopied'))
        // 短暂显示后关闭弹窗并打开链接
        const urlToOpen = result.url
        setTimeout(async () => {
          setShowSubscriptionDialog(false)
          setSubscriptionSuccess(null)
          await window.api.openSubscriptionWindow(urlToOpen)
        }, 800)
      } else {
        const errorMsg = result.error || t('errors.failedToGetPaymentUrl')
        setSubscriptionError(errorMsg)
        console.error('[AccountCard] Failed to get payment URL:', result.error)
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : t('errors.unknown')
      setSubscriptionError(errorMsg)
      console.error('[AccountCard] Payment URL error:', error)
    } finally {
      setPaymentLoading(false)
      setSelectedPlan(null)
    }
  }

  // 获取订阅管理链接（已有订阅用户）
  const handleManageSubscription = async (): Promise<void> => {
    if (paymentLoading || !account.credentials?.accessToken) return
    
    setPaymentLoading(true)
    setSubscriptionError(null)
    try {
      const result = await window.api.accountGetSubscriptionUrl(account.credentials.accessToken, undefined, account.credentials?.region)
      if (result.success && result.url) {
        setShowSubscriptionDialog(false)
        await window.api.openSubscriptionWindow(result.url)
      } else {
        // 显示错误信息
        const errorMsg = result.error || t('errors.failedToGetManagementUrl')
        setSubscriptionError(errorMsg)
        console.error('[AccountCard] Failed to get management URL:', result.error)
      }
    } catch (error) {
      const errorMsg = error instanceof Error ? error.message : t('errors.unknown')
      setSubscriptionError(errorMsg)
      console.error('[AccountCard] Management URL error:', error)
    } finally {
      setPaymentLoading(false)
    }
  }

  // 封禁状态样式（红色）- 优先级最高
  const unauthorizedStyle: React.CSSProperties = isUnauthorized ? {
    backgroundColor: 'var(--card-unauthorized-bg)',
    borderColor: 'var(--card-unauthorized-border)',
    boxShadow: `
      0 0 0 1px var(--card-unauthorized-ring),
      0 4px 20px -2px var(--card-unauthorized-shadow),
      inset 0 0 20px var(--card-unauthorized-glow)
    `
  } : {}

  // 当前使用的高级感样式 - 流光边框时仅保留外发光
  const activeGlowStyle: React.CSSProperties = account.isActive ? {
    boxShadow: '0 8px 24px -4px var(--card-active-shadow)'
  } : {}

  // 最终样式合并逻辑
  let finalStyle: React.CSSProperties = {}
  
  if (account.isActive) {
    // 当前使用（包括封禁+当前使用）：流光边框 + 外发光，封禁通过角标显示
    finalStyle = { ...glowStyle, ...activeGlowStyle }
  } else if (isUnauthorized) {
    // 仅封禁状态：显示完整封禁样式
    finalStyle = unauthorizedStyle
  } else {
    // 普通状态：只显示标签光环
    finalStyle = glowStyle
  }

  return (
    <Card
      className={cn(
        'relative transition-all duration-300 hover:shadow-lg cursor-pointer h-full flex flex-col overflow-hidden border',
        // 边框颜色优先级：当前使用的流光边框优先于封禁边框
        account.isActive ? 'border-transparent active-glow-border' :
        isUnauthorized ? 'border-red-400/50' :
        '',
        
        isSelected && !account.isActive && !isUnauthorized && 'bg-primary/5',
        
        // 有光环时隐藏默认边框（当前使用和封禁除外）
        accountTags.length > 0 && !account.isActive && !isUnauthorized && 'border-transparent'
      )}
      style={finalStyle}
      onClick={() => toggleSelection(account.id)}
    >
      {/* 封禁角标 - 当前使用时显示在流光边框上 */}
      {account.isActive && isUnauthorized && (
        <div className="banned-badge" title={t('accounts.card.banned')} />
      )}
      <CardContent className="p-4 flex-1 flex flex-col gap-3 overflow-hidden">
        {/* Header: Checkbox, Email/Nickname, Group */}
        <div className="flex gap-3 items-start">
           {/* Checkbox */}
           <div
            className={cn(
              'w-5 h-5 rounded border-2 flex items-center justify-center transition-colors flex-shrink-0 mt-0.5 cursor-pointer',
              isSelected
                ? 'bg-primary border-primary text-primary-foreground'
                : 'border-muted-foreground/30 hover:border-primary'
            )}
            onClick={(e) => {
              e.stopPropagation()
              onSelect()
            }}
          >
            {isSelected && <Check className="h-3.5 w-3.5" />}
          </div>

           <div className="flex-1 min-w-0">
              <div className="flex items-center justify-between gap-2">
                 <h3 
                   className={cn(
                     "font-semibold text-sm truncate cursor-pointer transition-colors",
                     emailCopied ? "text-green-500" : "text-foreground/90 hover:text-primary"
                   )}
                   title={`${getDisplayName(account)} (${t('accountCard.clickToCopy')})`}
                   onClick={(e) => {
                     e.stopPropagation()
                     const text = account.email || account.userId || ''
                     if (text) {
                       navigator.clipboard.writeText(text)
                       setEmailCopied(true)
                       setTimeout(() => setEmailCopied(false), 1500)
                     }
                   }}
                 >{emailCopied ? t('accountCard.copied') : (account.email ? maskEmail(account.email) : getDisplayName(account))}</h3>
                 {/* Status Badge */}
                 <div className={cn(
                    "text-[10px] font-medium px-2 py-0.5 rounded-full flex items-center gap-1 flex-shrink-0",
                    isUnauthorized ? "text-red-600 bg-red-100 dark:text-red-400 dark:bg-red-900/30" :
                    account.status === 'active' ? "text-green-600 bg-green-100 dark:text-green-400 dark:bg-green-900/30" :
                    account.status === 'error' ? "text-red-600 bg-red-100 dark:text-red-400 dark:bg-red-900/30" :
                    account.status === 'expired' ? "text-orange-600 bg-orange-100 dark:text-orange-400 dark:bg-orange-900/30" :
                    account.status === 'refreshing' ? "text-primary bg-primary/10" :
                    "text-muted-foreground bg-muted"
                 )}>
                    {account.status === 'refreshing' && <Loader2 className="h-3 w-3 animate-spin" />}
                    {isUnauthorized && <AlertCircle className="h-3 w-3" />}
                    {isUnauthorized ? (
                      <span 
                        className="cursor-pointer hover:underline" 
                        onClick={(e) => { e.stopPropagation(); setShowBanDialog(true); }}
                      >
                        {t('accountCard.banned')}
                      </span>
                    ) : t(`status.${account.status}`)}
                 </div>
              </div>
              <div className="flex items-center gap-2 mt-1">
                  {account.nickname && <span className="text-xs text-muted-foreground truncate">{maskNickname(account.nickname)}</span>}
                  {accountGroup && (
                    <span
                      className="text-[10px] px-1.5 py-0.5 rounded bg-muted text-muted-foreground flex items-center gap-1"
                      style={{ color: accountGroup.color, backgroundColor: accountGroup.color + '15' }}
                    >
                      <FolderOpen className="w-3 h-3" /> {accountGroup.name}
                    </span>
                  )}
              </div>
           </div>
        </div>

        {/* Badges Row */}
        <div className="flex items-center gap-2 flex-wrap">
            <Badge 
              className={cn(
                'text-white text-[10px] h-5 px-2 border-0 cursor-pointer transition-all hover:opacity-80 hover:scale-105',
                getSubscriptionColor(account.subscription.type, account.subscription.title),
                subscriptionLoading && 'opacity-60 cursor-wait'
              )}
              onClick={handleSubscriptionClick}
              title={t('accountCard.clickToManageSubscription')}
            >
                {subscriptionLoading ? t('accountCard.loading') : (account.subscription.title || account.subscription.type)}
            </Badge>
            <Badge variant="outline" className="text-[10px] h-5 px-2 text-muted-foreground font-normal border-muted-foreground/30 bg-muted/30">
                {account.idp}
            </Badge>
            {account.isActive && (
              <Badge variant="default" className="ml-auto h-5 bg-green-500 text-white border-0 hover:bg-green-600">
                {t('accountCard.active')}
              </Badge>
            )}
        </div>

        {/* Usage Section */}
        <div className="bg-muted/30 p-3 rounded-lg space-y-2 border border-border/50">
            <div className="flex justify-between items-end text-xs">
                <span className="text-muted-foreground font-medium">{t('accountCard.usage')}</span>
                <span className={cn("font-mono font-medium", isHighUsage ? "text-amber-600" : "text-foreground")}>
                   {(account.usage.percentUsed * 100).toFixed(usagePrecision ? 2 : 0)}%
                </span>
            </div>
            <Progress
              value={account.usage.percentUsed * 100}
              className="h-1.5"
              indicatorClassName={isHighUsage ? "bg-amber-500" : "bg-primary"}
            />
            <div className="flex justify-between text-[10px] text-muted-foreground pt-0.5">
                <span>{formatUsage(account.usage.current)} / {formatUsage(account.usage.limit)}</span>
                {account.usage.nextResetDate && (
                  <span className="flex items-center gap-1">
                    <Calendar className="h-3 w-3" />
                     {(() => {
                      const d = account.usage.nextResetDate as unknown
                      try {
                         return (typeof d === 'string' ? d : new Date(d as Date).toISOString()).split('T')[0]
                      } catch { return 'Unknown' }
                    })()} {t('accountCard.reset')}
                  </span>
                )}
            </div>
        </div>

        {/* Detailed Quotas - Compact list */}
        <div className="space-y-1.5 min-h-0 overflow-y-auto pr-1 text-[10px] max-h-24">
           {/* 基础额度 */}
           {account.usage.baseLimit !== undefined && account.usage.baseLimit > 0 && (
             <div className="flex items-center gap-2">
               <div className="w-1.5 h-1.5 rounded-full bg-blue-500 flex-shrink-0" />
               <span className="text-muted-foreground">{t('accountCard.base')}</span>
               <span className="font-medium">{formatUsage(account.usage.baseCurrent ?? 0)}/{formatUsage(account.usage.baseLimit)}</span>
               {account.usage.nextResetDate && (
                 <span className="text-muted-foreground/70 ml-auto">
                   {t('accountCard.to')} {(() => {
                      const d = account.usage.nextResetDate as unknown
                      try { return (typeof d === 'string' ? d : new Date(d as Date).toISOString()).split('T')[0] } catch { return '' }
                   })()}
                 </span>
               )}
             </div>
           )}
           {/* 试用额度 */}
           {account.usage.freeTrialLimit !== undefined && account.usage.freeTrialLimit > 0 && (
             <div className="flex items-center gap-2">
               <div className="w-1.5 h-1.5 rounded-full bg-purple-500 flex-shrink-0" />
               <span className="text-muted-foreground">{t('accountCard.trial')}</span>
               <span className="font-medium">{formatUsage(account.usage.freeTrialCurrent ?? 0)}/{formatUsage(account.usage.freeTrialLimit)}</span>
               {account.usage.freeTrialExpiry && (
                 <span className="text-muted-foreground/70 ml-auto">
                   {t('accountCard.to')} {(() => {
                      const d = account.usage.freeTrialExpiry as unknown
                      try { return (typeof d === 'string' ? d : new Date(d as Date).toISOString()).split('T')[0] } catch { return '' }
                   })()}
                 </span>
               )}
             </div>
           )}
           {/* 奖励额度 */}
           {account.usage.bonuses?.map((bonus) => (
             <div key={bonus.code} className="flex items-center gap-2">
               <div className="w-1.5 h-1.5 rounded-full bg-cyan-500 flex-shrink-0" />
               <span className="text-muted-foreground truncate max-w-[80px]" title={bonus.name}>{bonus.name}:</span>
               <span className="font-medium">{formatUsage(bonus.current)}/{formatUsage(bonus.limit)}</span>
               {bonus.expiresAt && (
                 <span className="text-muted-foreground/70 ml-auto">
                   {t('accountCard.to')} {(() => {
                      const d = bonus.expiresAt as unknown
                      try { return (typeof d === 'string' ? d : new Date(d as Date).toISOString()).split('T')[0] } catch { return '' }
                   })()}
                 </span>
               )}
             </div>
           ))}
        </div>
        
        {/* Tags - placed before footer */}
        {accountTags.length > 0 && (
          <div className="flex flex-wrap gap-1 mt-auto pt-2">
            {accountTags.slice(0, 4).map((tag) => (
              <span
                key={tag.id}
                className="px-1.5 py-0.5 text-[10px] rounded-sm text-white font-medium shadow-sm"
                style={{ backgroundColor: toRgba(tag.color) }}
              >
                {tag.name}
              </span>
            ))}
             {accountTags.length > 4 && (
              <span className="px-1.5 py-0.5 text-[10px] text-muted-foreground bg-muted rounded-sm">
                +{accountTags.length - 4}
              </span>
            )}
          </div>
        )}

        {/* Footer Actions */}
        <div className="pt-3 border-t flex items-center justify-between mt-auto gap-2 shrink-0">
            {/* Left: Token expiry info */}
            <div className="text-[10px] text-muted-foreground flex flex-col leading-tight gap-0.5">
                <div className="flex items-center gap-1">
                   <Clock className="h-3 w-3" />
                   <span className={isExpiringSoon ? "text-amber-600 font-medium" : ""}>
                      {account.subscription.daysRemaining !== undefined ? t('time.daysShort', { n: account.subscription.daysRemaining }) + (t('common.unknown') === 'Unknown' ? ' left' : ' 剩') : '-'}
                   </span>
                </div>
                <div className="flex items-center gap-1" title={account.credentials.expiresAt ? new Date(account.credentials.expiresAt).toLocaleString(isEn ? 'en-US' : 'zh-CN') : t('common.unknown')}>
                   <KeyRound className="h-3 w-3" />
                   <span className={account.credentials.expiresAt && account.credentials.expiresAt - Date.now() < 5 * 60 * 1000 ? "text-red-500 font-medium" : ""}>
                      Token: {account.credentials.expiresAt ? formatTokenExpiry(account.credentials.expiresAt, t) : '-'}
                   </span>
                </div>
            </div>

            {/* Right: Actions */}
            <div className="flex items-center gap-0.5">
               {account.isActive ? (
                 <Button
                   size="icon"
                   variant="ghost"
                   className="h-7 w-7 hover:bg-destructive/10 hover:text-destructive transition-colors"
                   onClick={(e) => { e.stopPropagation(); handleLogout() }}
                   title={t('accountCard.logout')}
                 >
                   <LogOut className="h-3.5 w-3.5" />
                 </Button>
               ) : (
                 <Button
                   size="icon"
                   variant="ghost"
                   className="h-7 w-7 hover:bg-primary/10 hover:text-primary transition-colors"
                   onClick={(e) => { e.stopPropagation(); handleSwitch() }}
                   title={t('accountCard.switchToAccount')}
                 >
                   <Power className="h-3.5 w-3.5" />
                 </Button>
               )}
               
               <Button size="icon" variant="ghost" className="h-7 w-7 text-muted-foreground hover:text-foreground" onClick={(e) => { e.stopPropagation(); handleRefresh() }} disabled={account.status === 'refreshing'} title={t('accountCard.checkAccountInfo')}>
                  <RefreshCw className={cn("h-3.5 w-3.5", account.status === 'refreshing' && "animate-spin")} />
               </Button>
               <Button size="icon" variant="ghost" className="h-7 w-7 text-muted-foreground hover:text-foreground" onClick={(e) => { e.stopPropagation(); handleRefreshToken() }} disabled={isRefreshingToken} title={t('accountCard.refreshToken')}>
                  <KeyRound className={cn("h-3.5 w-3.5", isRefreshingToken && "animate-pulse")} />
               </Button>
               
               <Button size="icon" variant="ghost" className={cn("h-7 w-7 text-muted-foreground hover:text-foreground", copied && "text-green-500")} onClick={(e) => { e.stopPropagation(); handleCopyCredentials() }} title={t('accountCard.copyCredentials')}>
                  {copied ? <Check className="h-3.5 w-3.5" /> : <Copy className="h-3.5 w-3.5" />}
               </Button>

               <Button size="icon" variant="ghost" className="h-7 w-7 text-muted-foreground hover:text-foreground" onClick={(e) => { e.stopPropagation(); onShowDetail() }} title={t('accountCard.details')}>
                  <Info className="h-3.5 w-3.5" />
               </Button>
               
               <Button size="icon" variant="ghost" className="h-7 w-7 text-muted-foreground hover:text-foreground" onClick={(e) => { e.stopPropagation(); onEdit() }} title={t('accountCard.edit')}>
                  <Edit className="h-3.5 w-3.5" />
               </Button>
               
               <Button size="icon" variant="ghost" className="h-7 w-7 text-muted-foreground hover:text-destructive transition-colors" onClick={(e) => { e.stopPropagation(); handleDelete() }} title={t('accountCard.delete')}>
                  <Trash2 className="h-3.5 w-3.5" />
               </Button>
            </div>
        </div>

        {/* Error Message (Non-banned) */}
        {account.lastError && !isUnauthorized && (
          <div className="bg-red-50 text-red-600 text-[10px] p-1.5 rounded flex items-center gap-1.5 truncate mt-1" title={account.lastError}>
             <AlertTriangle className="h-3 w-3 shrink-0" />
             <span className="truncate">{account.lastError}</span>
          </div>
        )}
      </CardContent>

      {/* 封禁详情弹窗 */}
      {showBanDialog && isUnauthorized && createPortal(
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" onClick={() => setShowBanDialog(false)} />
          <div className="relative bg-background rounded-xl shadow-2xl w-full max-w-lg m-4 animate-in fade-in zoom-in-95 duration-200 border overflow-hidden">
            <div className="p-4 border-b flex items-center justify-between bg-red-50 dark:bg-red-900/20">
              <div className="flex items-center gap-2 text-red-600 dark:text-red-400">
                <AlertCircle className="h-5 w-5" />
                <span className="font-bold">{t('accountCard.accountSuspended')}</span>
              </div>
              <Button variant="ghost" size="icon" className="h-8 w-8 hover:bg-red-100 dark:hover:bg-red-900/30" onClick={() => setShowBanDialog(false)}>
                <X className="h-4 w-4" />
              </Button>
            </div>
            <div className="p-4 space-y-4">
              <div className="space-y-2">
                <label className="text-xs font-medium text-muted-foreground">{t('accountCard.account')}</label>
                <div className="text-sm font-medium">{getDisplayName(account)}</div>
              </div>
              <div className="space-y-2">
                <label className="text-xs font-medium text-muted-foreground">{t('errors.errorDetails')}</label>
                <div className="text-xs font-mono bg-muted/50 p-3 rounded-lg border break-all whitespace-pre-wrap max-h-[200px] overflow-y-auto">
                  {account.lastError}
                </div>
              </div>
              <div className="flex items-center justify-between pt-2">
                <a 
                  href="https://support.aws.amazon.com/#/contacts/kiro" 
                  target="_blank" 
                  rel="noopener noreferrer"
                  className="text-xs text-primary hover:underline flex items-center gap-1"
                  onClick={(e) => e.stopPropagation()}
                >
                  <ExternalLink className="h-3 w-3" />
                  {t('accountCard.contactSupport')}
                </a>
                <Button size="sm" variant="outline" onClick={() => setShowBanDialog(false)}>
                  {t('accountCard.close')}
                </Button>
              </div>
            </div>
          </div>
        </div>,
        document.body
      )}

      {/* 订阅管理弹窗 */}
      {showSubscriptionDialog && createPortal(
        <div className="fixed inset-0 z-50 flex items-center justify-center">
          <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" onClick={() => { setShowSubscriptionDialog(false); setIsFirstTimeUser(false); setSubscriptionError(null); setSubscriptionSuccess(null) }} />
          <div className="relative bg-background rounded-xl shadow-2xl w-full max-w-2xl m-4 animate-in fade-in zoom-in-95 duration-200 border overflow-hidden">
            <div className="p-4 border-b flex items-center justify-between bg-gradient-to-r from-primary/10 to-purple-500/10">
              <div className="flex items-center gap-2 text-primary">
                <CreditCard className="h-5 w-5" />
                <span className="font-bold">{t(isFirstTimeUser ? 'accountCard.chooseYourPlan' : 'accountCard.subscriptionPlans')}</span>
              </div>
              <Button variant="ghost" size="icon" className="h-8 w-8" onClick={() => { setShowSubscriptionDialog(false); setIsFirstTimeUser(false); setSubscriptionError(null); setSubscriptionSuccess(null) }}>
                <X className="h-4 w-4" />
              </Button>
            </div>
            <div className="p-4 space-y-4">
              {isFirstTimeUser ? (
                <div className="text-xs text-muted-foreground mb-2 bg-amber-500/10 text-amber-600 dark:text-amber-400 p-2 rounded-lg">
                  {t('accountCard.pleaseSelectPlan')}
                </div>
              ) : (
                <div className="text-xs text-muted-foreground mb-2">
                  {t('accountCard.currentSubscription')}
                  <span className="font-medium text-foreground">{account.subscription.title || account.subscription.type}</span>
                </div>
              )}
              
              {subscriptionError && (
                <div className="text-xs bg-red-500/10 text-red-600 dark:text-red-400 p-2 rounded-lg flex items-center gap-2">
                  <AlertCircle className="h-4 w-4 shrink-0" />
                  <span>{subscriptionError}</span>
                </div>
              )}
              
              {subscriptionSuccess && (
                <div className="text-xs bg-green-500/10 text-green-600 dark:text-green-400 p-2 rounded-lg flex items-center gap-2">
                  <Check className="h-4 w-4 shrink-0" />
                  <span>{subscriptionSuccess}</span>
                </div>
              )}
              
              <div className="grid grid-cols-2 gap-3">
                {subscriptionPlans.map((plan) => {
                  const isCurrent = plan.name === account.subscription.type || plan.description.title === account.subscription.title
                  const isLoading = paymentLoading && selectedPlan === plan.qSubscriptionType
                  return (
                    <div
                      key={plan.name}
                      className={cn(
                        'relative p-4 rounded-lg border-2 transition-all cursor-pointer hover:shadow-md',
                        isCurrent ? 'border-primary bg-primary/5' : 'border-border hover:border-primary/50',
                        isLoading && 'opacity-70 cursor-wait'
                      )}
                      onClick={() => !isCurrent && handleSelectPlan(plan.qSubscriptionType)}
                    >
                      {isCurrent && (
                        <div className="absolute -top-2 -right-2 bg-primary text-primary-foreground text-[10px] px-2 py-0.5 rounded-full font-medium">
                          {t('accountCard.current')}
                        </div>
                      )}
                      <div className="flex items-center gap-2 mb-2">
                        <Sparkles className={cn('h-4 w-4', plan.pricing.amount === 0 ? 'text-green-500' : 'text-amber-500')} />
                        <span className="font-bold text-sm">{plan.description.title}</span>
                      </div>
                      <div className="text-2xl font-bold mb-2">
                        {plan.pricing.amount === 0 ? t('accountCard.free') : `$${plan.pricing.amount}`}
                        {plan.pricing.amount > 0 && <span className="text-xs font-normal text-muted-foreground">/{plan.description.billingInterval}</span>}
                      </div>
                      <ul className="space-y-1.5">
                        {plan.description.features.slice(0, 4).map((feature, idx) => (
                          <li key={idx} className="text-xs text-muted-foreground flex items-start gap-1.5">
                            <Check className="h-3 w-3 text-green-500 mt-0.5 shrink-0" />
                            <span>{feature}</span>
                          </li>
                        ))}
                      </ul>
                      {!isCurrent && (
                        <Button 
                          size="sm" 
                          className="w-full mt-3" 
                          variant={plan.pricing.amount === 0 ? 'outline' : 'default'}
                          disabled={isLoading}
                        >
                          {isLoading ? (
                            <><Loader2 className="h-3 w-3 mr-1 animate-spin" />{t('accountCard.loading')}</>
                          ) : (
                            t('accountCard.select')
                          )}
                        </Button>
                      )}
                    </div>
                  )
                })}
              </div>

              <div className="flex items-center justify-between pt-3 border-t">
                <Button 
                  size="sm" 
                  variant="outline" 
                  onClick={handleManageSubscription}
                  disabled={paymentLoading}
                  className="text-xs"
                >
                  {paymentLoading && !selectedPlan ? (
                    <><Loader2 className="h-3 w-3 mr-1 animate-spin" />{t('accountCard.loading')}</>
                  ) : (
                    <><ExternalLink className="h-3 w-3 mr-1" />{t('accountCard.manageBilling')}</>
                  )}
                </Button>
                <Button size="sm" variant="ghost" onClick={() => { setShowSubscriptionDialog(false); setIsFirstTimeUser(false); setSubscriptionError(null); setSubscriptionSuccess(null) }}>
                  {t('accountCard.close')}
                </Button>
              </div>
            </div>
          </div>
        </div>,
        document.body
      )}
    </Card>
  )
})
