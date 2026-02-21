import { useState, useEffect } from 'react'
import { createPortal } from 'react-dom'
import { X, Download, RefreshCw, Sparkles, CheckCircle } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Progress } from '@/components/ui/progress'
import { useTranslation } from '@/hooks/useTranslation'

interface UpdateInfo {
  version: string
  releaseDate?: string
  releaseNotes?: string
}

interface DownloadProgress {
  percent: number
  bytesPerSecond: number
  transferred: number
  total: number
}

type UpdateStatus = 'idle' | 'checking' | 'available' | 'downloading' | 'downloaded' | 'error'

export function UpdateDialog() {
  const [open, setOpen] = useState(false)
  const [status, setStatus] = useState<UpdateStatus>('idle')
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null)
  const [progress, setProgress] = useState<DownloadProgress | null>(null)
  const [error, setError] = useState<string | null>(null)
  const { t } = useTranslation()

  useEffect(() => {
    // 监听更新事件
    const unsubChecking = window.api.onUpdateChecking(() => {
      setStatus('checking')
    })

    const unsubAvailable = window.api.onUpdateAvailable((info) => {
      setUpdateInfo(info)
      setStatus('available')
      setOpen(true) // 有更新时自动打开弹窗
    })

    const unsubNotAvailable = window.api.onUpdateNotAvailable(() => {
      setStatus('idle')
    })

    const unsubProgress = window.api.onUpdateDownloadProgress((prog) => {
      setProgress(prog)
      setStatus('downloading')
    })

    const unsubDownloaded = window.api.onUpdateDownloaded((info) => {
      setUpdateInfo(info)
      setStatus('downloaded')
    })

    const unsubError = window.api.onUpdateError((err) => {
      setError(err)
      setStatus('error')
    })

    return () => {
      unsubChecking()
      unsubAvailable()
      unsubNotAvailable()
      unsubProgress()
      unsubDownloaded()
      unsubError()
    }
  }, [])

  const handleDownload = async () => {
    setStatus('downloading')
    setProgress(null)
    await window.api.downloadUpdate()
  }

  const handleInstall = () => {
    window.api.installUpdate()
  }

  const handleClose = () => {
    if (status !== 'downloading') {
      setOpen(false)
    }
  }

  const formatBytes = (bytes: number) => {
    if (bytes < 1024) return `${bytes} B`
    if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
    return `${(bytes / 1024 / 1024).toFixed(1)} MB`
  }

  const formatSpeed = (bytesPerSecond: number) => {
    return `${formatBytes(bytesPerSecond)}/s`
  }

  if (!open) return null

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <div className="absolute inset-0 bg-black/50" onClick={handleClose} />
      
      <div className="relative bg-background rounded-xl shadow-2xl w-full max-w-md m-4 animate-in zoom-in-95 duration-200 border overflow-hidden">
        {/* 头部 */}
        <div className="bg-gradient-to-r from-primary/10 to-primary/5 p-6 border-b">
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="w-12 h-12 rounded-xl bg-primary/20 flex items-center justify-center">
                <Sparkles className="h-6 w-6 text-primary" />
              </div>
              <div>
                <h2 className="text-lg font-bold">{t('update.newVersionTitle')}</h2>
                {updateInfo && (
                  <p className="text-sm text-muted-foreground">v{updateInfo.version}</p>
                )}
              </div>
            </div>
            {status !== 'downloading' && (
              <button
                onClick={handleClose}
                className="p-2 hover:bg-muted rounded-lg transition-colors"
              >
                <X className="h-5 w-5" />
              </button>
            )}
          </div>
        </div>

        {/* 内容 */}
        <div className="p-6 space-y-4">
          {status === 'available' && (
            <>
              <p className="text-sm text-muted-foreground">
                {t('update.newVersionDesc')}
              </p>
              {updateInfo?.releaseNotes && (
                <div 
                  className="bg-muted/50 rounded-lg p-3 max-h-32 overflow-y-auto text-xs text-muted-foreground prose prose-sm prose-neutral dark:prose-invert max-w-none [&_ul]:list-disc [&_ul]:pl-4 [&_li]:my-0.5 [&_h1]:text-sm [&_h2]:text-xs [&_h3]:text-xs [&_p]:my-1 [&_hr]:my-2"
                  dangerouslySetInnerHTML={{ __html: updateInfo.releaseNotes }}
                />
              )}
              <div className="flex gap-3">
                <Button variant="outline" className="flex-1" onClick={handleClose}>
                  {t('update.remindLater')}
                </Button>
                <Button className="flex-1" onClick={handleDownload}>
                  <Download className="h-4 w-4 mr-2" />
                  {t('update.downloadNow')}
                </Button>
              </div>
            </>
          )}

          {status === 'downloading' && (
            <>
              <div className="space-y-2">
                <div className="flex items-center justify-between text-sm">
                  <span className="text-muted-foreground">{t('update.downloading')}</span>
                  <span className="font-mono">{progress ? `${progress.percent.toFixed(1)}%` : '0%'}</span>
                </div>
                <Progress value={progress?.percent ?? 0} className="h-2" />
                {progress && (
                  <div className="flex justify-between text-xs text-muted-foreground">
                    <span>{formatBytes(progress.transferred)} / {formatBytes(progress.total)}</span>
                    <span>{formatSpeed(progress.bytesPerSecond)}</span>
                  </div>
                )}
              </div>
              <div className="flex items-center gap-2 text-sm text-muted-foreground">
                <RefreshCw className="h-4 w-4 animate-spin" />
                <span>{t('update.doNotClose')}</span>
              </div>
            </>
          )}

          {status === 'downloaded' && (
            <>
              <div className="flex items-center gap-3 text-green-600">
                <CheckCircle className="h-6 w-6" />
                <span className="font-medium">{t('update.downloadComplete')}</span>
              </div>
              <p className="text-sm text-muted-foreground">
                {t('update.downloadCompleteDesc')}
              </p>
              <div className="flex gap-3">
                <Button variant="outline" className="flex-1" onClick={handleClose}>
                  {t('update.installLater')}
                </Button>
                <Button className="flex-1" onClick={handleInstall}>
                  <RefreshCw className="h-4 w-4 mr-2" />
                  {t('update.restartNow')}
                </Button>
              </div>
            </>
          )}

          {status === 'error' && (
            <>
              <p className="text-sm text-destructive">
                {t('update.updateCheckFailed', { error: error || 'Unknown error' })}
              </p>
              <Button variant="outline" className="w-full" onClick={handleClose}>
                {t('update.close')}
              </Button>
            </>
          )}
        </div>
      </div>
    </div>,
    document.body
  )
}
