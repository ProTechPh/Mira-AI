import { useState, useEffect, useCallback } from 'react'
import { 
  Play, Square, RefreshCw, Copy, Check, Shield, Activity, 
  AlertCircle, Globe, Loader2, FileText, Download, Key, 
  Fingerprint, Server
} from 'lucide-react'
import { 
  Button, Card, CardContent, CardDescription, CardHeader, 
  CardTitle, Input, Label, Switch, Badge 
} from '../ui'
import { useTranslation } from '../../hooks/useTranslation'
import { cn } from '../../lib/utils'
import { useAccountsStore } from '@/store/accounts'

interface KProxyConfig {
  enabled: boolean
  port: number
  host: string
  mitmDomains: string[]
  deviceId?: string
  autoStart: boolean
  logRequests: boolean
}

interface KProxyStats {
  totalRequests: number
  mitmRequests: number
  bypassRequests: number
  modifiedRequests: number
  startTime: number
  lastRequestTime: number
}

interface CACertInfo {
  certPath: string
  fingerprint: string
  validFrom: string
  validTo: string
}

export function MProxyPanel() {
  const { t } = useTranslation()
  const isEn = t('common.unknown') === 'Unknown'
  const { machineIdConfig, setMachineIdConfig } = useAccountsStore()
  
  const [isRunning, setIsRunning] = useState(false)
  const [isInitialized, setIsInitialized] = useState(false)
  const [isInitializing, setIsInitializing] = useState(false)
  const [config, setConfig] = useState<KProxyConfig>({
    enabled: false,
    port: 8899,
    host: '127.0.0.1',
    mitmDomains: ['amazonaws.com', 'amazon.com'],
    autoStart: false,
    logRequests: true
  })
  const [stats, setStats] = useState<KProxyStats | null>(null)
  const [caInfo, setCaInfo] = useState<CACertInfo | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)
  const [deviceIdCopied, setDeviceIdCopied] = useState(false)
  const [recentRequests, setRecentRequests] = useState<Array<{
    timestamp: number
    host: string
    method: string
    path: string
    isMitm: boolean
    deviceIdReplaced: boolean
  }>>([])
  const [caInstalled, setCaInstalled] = useState<boolean | null>(null)

  // 检查 CA 证书是否已安装
  const checkCaInstalled = useCallback(async () => {
    try {
      const result = await window.api.kproxyCheckCaCertInstalled()
      setCaInstalled(result.installed)
    } catch {
      setCaInstalled(null)
    }
  }, [])

  // 初始化 K-Proxy
  const initKProxy = useCallback(async () => {
    if (isInitialized || isInitializing) return
    setIsInitializing(true)
    setError(null)
    
    try {
      const result = await window.api.kproxyInit()
      if (result.success) {
        setIsInitialized(true)
        if (result.caInfo) {
          setCaInfo(result.caInfo)
        }
        // 获取状态
        const status = await window.api.kproxyGetStatus()
        if (status.config) {
          setConfig(status.config as KProxyConfig)
        }
        if (status.stats) {
          setStats(status.stats as KProxyStats)
        }
        setIsRunning(status.running)
      } else {
        setError(result.error || 'Failed to initialize K-Proxy')
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Init failed')
    } finally {
      setIsInitializing(false)
    }
  }, [isInitialized, isInitializing])

  // 初始化
  useEffect(() => {
    initKProxy()
    checkCaInstalled()
  }, [initKProxy, checkCaInstalled])

  // 监听事件
  useEffect(() => {
    const unsubRequest = window.api.onKproxyRequest((info) => {
      setRecentRequests(prev => [{
        timestamp: info.timestamp,
        host: info.host,
        method: info.method,
        path: info.path,
        isMitm: info.isMitm,
        deviceIdReplaced: info.deviceIdReplaced
      }, ...prev].slice(0, 50))
    })

    const unsubStatus = window.api.onKproxyStatusChange((status) => {
      setIsRunning(status.running)
    })

    const unsubError = window.api.onKproxyError((err) => {
      setError(err)
    })

    return () => {
      unsubRequest()
      unsubStatus()
      unsubError()
    }
  }, [])

  // 启动/停止
  const toggleProxy = async () => {
    setError(null)
    try {
      if (isRunning) {
        const result = await window.api.kproxyStop()
        if (!result.success) {
          setError(result.error || 'Failed to stop')
        }
      } else {
        if (
          machineIdConfig.autoSwitchOnAccountChange ||
          machineIdConfig.bindMachineIdToAccount ||
          machineIdConfig.useBindedMachineId
        ) {
          // Hard enforce: 启动 M-Proxy 前自动关闭 Machine ID 自动化
          setMachineIdConfig({
            autoSwitchOnAccountChange: false,
            bindMachineIdToAccount: false,
            useBindedMachineId: false
          })
        }
        const result = await window.api.kproxyStart(config)
        if (!result.success) {
          setError(result.error || 'Failed to start')
        }
      }
      // 刷新状态
      const status = await window.api.kproxyGetStatus()
      setIsRunning(status.running)
      if (status.stats) {
        setStats(status.stats as KProxyStats)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Operation failed')
    }
  }

  // 更新配置
  const updateConfig = async (updates: Partial<KProxyConfig>) => {
    const newConfig = { ...config, ...updates }
    setConfig(newConfig)
    try {
      await window.api.kproxyUpdateConfig(updates)
    } catch (err) {
      console.error('Failed to update config:', err)
    }
  }

  // 生成设备 ID
  const generateDeviceId = async () => {
    try {
      const result = await window.api.kproxyGenerateDeviceId()
      if (result.success && result.deviceId) {
        await updateConfig({ deviceId: result.deviceId })
        await window.api.kproxySetDeviceId(result.deviceId)
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to generate device ID')
    }
  }

  // 复制代理地址
  const copyProxyAddress = () => {
    const address = `${config.host}:${config.port}`
    navigator.clipboard.writeText(address)
    setCopied(true)
    setTimeout(() => setCopied(false), 2000)
  }

  // 复制设备 ID
  const copyDeviceId = () => {
    if (config.deviceId) {
      navigator.clipboard.writeText(config.deviceId)
      setDeviceIdCopied(true)
      setTimeout(() => setDeviceIdCopied(false), 2000)
    }
  }

  // 导出 CA 证书
  const exportCaCert = async () => {
    try {
      const result = await window.api.kproxyExportCaCert()
      if (!result.success) {
        setError(result.error || 'Export failed')
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Export failed')
    }
  }

  // 格式化时间
  const formatTime = (timestamp: number) => {
    return new Date(timestamp).toLocaleTimeString()
  }

  if (!isInitialized) {
    // 初始化中或尚未开始初始化时显示加载状态
    if (isInitializing || !error) {
      return (
        <div className="flex flex-col items-center justify-center h-64 gap-4">
          <Loader2 className="h-8 w-8 animate-spin text-primary" />
          <p className="text-muted-foreground">
            {t('mproxy.initializing')}
          </p>
        </div>
      )
    }
    // 只有明确出错时才显示错误状态
    return (
      <div className="flex flex-col items-center justify-center h-64 gap-4">
        <AlertCircle className="h-8 w-8 text-destructive" />
        <p className="text-destructive">{error}</p>
        <Button onClick={initKProxy}>
          <RefreshCw className="h-4 w-4 mr-2" />
          {t('mproxy.retry')}
        </Button>
      </div>
    )
  }

  return (
    <div className="space-y-4">
      {/* 错误提示 */}
      {error && (
        <div className="bg-destructive/10 text-destructive px-4 py-2 rounded-md flex items-center gap-2">
          <AlertCircle className="h-4 w-4" />
          <span className="text-sm">{error}</span>
          <Button variant="ghost" size="sm" className="ml-auto h-6 px-2" onClick={() => setError(null)}>
            ✕
          </Button>
        </div>
      )}

      {/* 主控制卡片 */}
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Shield className="h-5 w-5 text-primary" />
              <CardTitle className="text-lg">M-Proxy MITM</CardTitle>
              <Badge variant={isRunning ? 'default' : 'secondary'} className={cn(
                "ml-2",
                isRunning && "bg-green-500 hover:bg-green-600"
              )}>
                {isRunning ? (
                  <span className="flex items-center gap-1">
                    <span className="relative flex h-2 w-2">
                      <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-white opacity-75"></span>
                      <span className="relative inline-flex rounded-full h-2 w-2 bg-white"></span>
                    </span>
                    {t('mproxy.running')}
                  </span>
                ) : t('mproxy.stopped')}
              </Badge>
            </div>
            <Button
              onClick={toggleProxy}
              variant={isRunning ? 'destructive' : 'default'}
              size="sm"
            >
              {isRunning ? (
                <>
                  <Square className="h-4 w-4 mr-1" />
                  {t('mproxy.stop')}
                </>
              ) : (
                <>
                  <Play className="h-4 w-4 mr-1" />
                  {t('mproxy.start')}
                </>
              )}
            </Button>
          </div>
          <CardDescription>
            {isEn 
              ? 'MITM proxy for replacing Machine ID in Kiro requests' 
              : 'MITM 代理，用于替换 Kiro 请求中的 Machine ID'}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* 代理地址 */}
          <div className="flex items-center gap-2">
            <Server className="h-4 w-4 text-muted-foreground" />
            <span className="text-sm text-muted-foreground">{t('mproxy.proxy')}</span>
            <code className="bg-muted px-2 py-1 rounded text-sm font-mono">
              {config.host}:{config.port}
            </code>
            <Button variant="ghost" size="sm" className="h-7 px-2" onClick={copyProxyAddress}>
              {copied ? <Check className="h-3 w-3 text-green-500" /> : <Copy className="h-3 w-3" />}
            </Button>
          </div>

          {/* 配置项 */}
          <div className="grid grid-cols-2 gap-4">
            <div className="space-y-2">
              <Label>{t('mproxy.port')}</Label>
              <Input
                type="number"
                value={config.port}
                onChange={(e) => updateConfig({ port: parseInt(e.target.value) || 8899 })}
                disabled={isRunning}
                className="h-8"
              />
            </div>
            <div className="space-y-2">
              <Label>{t('mproxy.host')}</Label>
              <Input
                value={config.host}
                onChange={(e) => updateConfig({ host: e.target.value })}
                disabled={isRunning}
                className="h-8"
              />
            </div>
          </div>

          {/* 开关选项 */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <FileText className="h-4 w-4 text-muted-foreground" />
              <Label>{t('mproxy.logRequests')}</Label>
            </div>
            <Switch
              checked={config.logRequests}
              onCheckedChange={(checked) => updateConfig({ logRequests: checked })}
            />
          </div>

          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Play className="h-4 w-4 text-muted-foreground" />
              <Label>{t('mproxy.autoStart')}</Label>
            </div>
            <Switch
              checked={config.autoStart}
              onCheckedChange={(checked) => updateConfig({ autoStart: checked })}
            />
          </div>
        </CardContent>
      </Card>

      {/* 设备 ID 卡片 */}
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center gap-2">
            <Fingerprint className="h-5 w-5 text-primary" />
            <CardTitle className="text-lg">{t('mproxy.deviceId')}</CardTitle>
          </div>
          <CardDescription>
            {isEn 
              ? 'Machine ID to replace in requests (64 hex characters)' 
              : '替换请求中的 Machine ID（64位十六进制）'}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3">
          <div className="flex items-center gap-2">
            <Input
              value={config.deviceId || ''}
              onChange={(e) => {
                updateConfig({ deviceId: e.target.value })
                if (e.target.value.length === 64) {
                  window.api.kproxySetDeviceId(e.target.value)
                }
              }}
              placeholder={t('mproxy.enterOrGenerate')}
              className="font-mono text-xs h-8"
            />
            <Button variant="outline" size="sm" className="h-8" onClick={generateDeviceId}>
              <Key className="h-3 w-3 mr-1" />
              {t('mproxy.generate')}
            </Button>
            {config.deviceId && (
              <Button variant="ghost" size="sm" className="h-8 px-2" onClick={copyDeviceId}>
                {deviceIdCopied ? <Check className="h-3 w-3 text-green-500" /> : <Copy className="h-3 w-3" />}
              </Button>
            )}
          </div>
          {config.deviceId && (
            <p className="text-xs text-muted-foreground">
              {config.deviceId.length === 64 
                ? t('mproxy.validDeviceIdFormat')
                : t('mproxy.invalidLength', { length: config.deviceId.length })}
            </p>
          )}
        </CardContent>
      </Card>

      {/* CA 证书卡片 */}
      <Card>
        <CardHeader className="pb-3">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Shield className="h-5 w-5 text-primary" />
              <CardTitle className="text-lg">{t('mproxy.caCertificate')}</CardTitle>
            </div>
            <div className="flex gap-2">
              {caInstalled === false ? (
                <Button variant="default" size="sm" onClick={async () => {
                  try {
                    const result = await window.api.kproxyInstallCaCert()
                    if (result.success) {
                      setCaInstalled(true)
                      alert(result.message || t('mproxy.certificateInstalled'))
                    } else {
                      alert(result.error || t('mproxy.failedToInstall'))
                    }
                  } catch (e) {
                    alert(e instanceof Error ? e.message : String(e))
                  }
                }}>
                  {t('mproxy.install')}
                </Button>
              ) : caInstalled === true ? (
                <Button variant="destructive" size="sm" onClick={async () => {
                  try {
                    const result = await window.api.kproxyUninstallCaCert()
                    if (result.success) {
                      setCaInstalled(false)
                      alert(result.message || t('mproxy.certificateUninstalled'))
                    } else {
                      alert(result.error || t('mproxy.failedToUninstall'))
                    }
                  } catch (e) {
                    alert(e instanceof Error ? e.message : String(e))
                  }
                }}>
                  {t('mproxy.uninstall')}
                </Button>
              ) : (
                <Button variant="outline" size="sm" disabled>
                  {t('mproxy.checking')}
                </Button>
              )}
              <Button variant="outline" size="sm" onClick={exportCaCert}>
                <Download className="h-3 w-3 mr-1" />
                {t('mproxy.export')}
              </Button>
            </div>
          </div>
          <CardDescription>
            {isEn 
              ? 'Install this certificate to trust M-Proxy MITM' 
              : '安装此证书以信任 M-Proxy MITM 代理'}
          </CardDescription>
        </CardHeader>
        {caInfo && (
          <CardContent className="space-y-2 text-sm">
            <div className="flex items-center gap-2">
              <span className="text-muted-foreground">{t('mproxy.fingerprint')}</span>
              <code className="bg-muted px-2 py-0.5 rounded text-xs font-mono truncate max-w-[300px]">
                {caInfo.fingerprint}
              </code>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-muted-foreground">{t('mproxy.valid')}</span>
              <span className="text-xs">
                {new Date(caInfo.validFrom).toLocaleDateString()} - {new Date(caInfo.validTo).toLocaleDateString()}
              </span>
            </div>
          </CardContent>
        )}
      </Card>

      {/* 统计卡片 */}
      {stats && (
        <Card>
          <CardHeader className="pb-3">
            <div className="flex items-center gap-2">
              <Activity className="h-5 w-5 text-primary" />
              <CardTitle className="text-lg">{t('mproxy.statistics')}</CardTitle>
            </div>
          </CardHeader>
          <CardContent>
            <div className="grid grid-cols-4 gap-4 text-center">
              <div>
                <div className="text-2xl font-bold">{stats.totalRequests}</div>
                <div className="text-xs text-muted-foreground">{t('mproxy.total')}</div>
              </div>
              <div>
                <div className="text-2xl font-bold text-blue-500">{stats.mitmRequests}</div>
                <div className="text-xs text-muted-foreground">{isEn ? 'MITM' : 'MITM'}</div>
              </div>
              <div>
                <div className="text-2xl font-bold text-green-500">{stats.modifiedRequests}</div>
                <div className="text-xs text-muted-foreground">{t('mproxy.modified')}</div>
              </div>
              <div>
                <div className="text-2xl font-bold text-gray-500">{stats.bypassRequests}</div>
                <div className="text-xs text-muted-foreground">{t('mproxy.bypass')}</div>
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      {/* 最近请求 */}
      {recentRequests.length > 0 && (
        <Card>
          <CardHeader className="pb-3">
            <div className="flex items-center gap-2">
              <Globe className="h-5 w-5 text-primary" />
              <CardTitle className="text-lg">{t('mproxy.recentRequests')}</CardTitle>
            </div>
          </CardHeader>
          <CardContent>
            <div className="space-y-1 max-h-48 overflow-y-auto">
              {recentRequests.slice(0, 10).map((req, idx) => (
                <div key={idx} className="flex items-center gap-2 text-xs py-1 border-b last:border-0">
                  <span className="text-muted-foreground w-16">{formatTime(req.timestamp)}</span>
                  <Badge variant={req.isMitm ? 'default' : 'secondary'} className="text-[10px] px-1 py-0">
                    {req.isMitm ? 'MITM' : 'PASS'}
                  </Badge>
                  {req.deviceIdReplaced && (
                    <Badge variant="outline" className="text-[10px] px-1 py-0 text-green-600 border-green-600">
                      ID
                    </Badge>
                  )}
                  <span className="font-mono truncate flex-1">{req.host}</span>
                </div>
              ))}
            </div>
          </CardContent>
        </Card>
      )}

      {/* 使用说明 */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-lg">{t('mproxy.usageGuide')}</CardTitle>
        </CardHeader>
        <CardContent className="text-sm text-muted-foreground space-y-2">
          <p>1. {t('mproxy.guide1')}</p>
          <p>2. {t('mproxy.guide2')} <code className="bg-muted px-1 rounded">{config.host}:{config.port}</code></p>
          <p>3. {t('mproxy.guide3')}</p>
          <p>4. {t('mproxy.guide4')}</p>
        </CardContent>
      </Card>
    </div>
  )
}
