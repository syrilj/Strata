import { useMemo, useState } from 'react'
import { Search, Download, RefreshCw } from 'lucide-react'
import { useDashboardStore } from '../store'
import { cn } from '../lib/utils'

export function SystemLogs() {
  const { logs, logsAutoRefresh, setLogsAutoRefresh, refreshLogs } = useDashboardStore()
  const [searchTerm, setSearchTerm] = useState('')
  const [levelFilter, setLevelFilter] = useState<string>('all')
  const [sourceFilter, setSourceFilter] = useState<string>('all')
  const [refreshing, setRefreshing] = useState(false)

  const filteredLogs = useMemo(() => {
    let filtered = logs

    if (searchTerm) {
      const needle = searchTerm.toLowerCase()
      filtered = filtered.filter(
        (log) =>
          log.message.toLowerCase().includes(needle) ||
          (log.source || '').toLowerCase().includes(needle)
      )
    }

    if (levelFilter !== 'all') {
      filtered = filtered.filter((log) => log.level === levelFilter)
    }

    if (sourceFilter !== 'all') {
      filtered = filtered.filter((log) => log.source === sourceFilter)
    }

    return filtered
  }, [logs, searchTerm, levelFilter, sourceFilter])

  const getLevelPill = (level: string) => {
    switch (level) {
      case 'error':
        return 'text-red-300 bg-red-500/[0.08] border-red-500/20'
      case 'warn':
        return 'text-amber-200 bg-amber-500/[0.08] border-amber-500/20'
      case 'info':
        return 'text-zinc-200 bg-white/[0.05] border-white/10'
      case 'debug':
        return 'text-zinc-500 bg-transparent border-white/[0.08]'
      default:
        return 'text-zinc-400 bg-white/[0.04] border-white/10'
    }
  }

  const formatTimestamp = (timestamp: number) => {
    return new Date(timestamp).toLocaleString()
  }

  const exportLogs = () => {
    const logText = filteredLogs
      .map((log) => `[${formatTimestamp(log.timestamp)}] ${log.level.toUpperCase()} [${log.source}] ${log.message}`)
      .join('\n')

    const blob = new Blob([logText], { type: 'text/plain' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `system-logs-${new Date().toISOString().split('T')[0]}.txt`
    a.click()
    URL.revokeObjectURL(url)
  }

  const handleRefresh = async () => {
    setRefreshing(true)
    try {
      await refreshLogs()
    } finally {
      setRefreshing(false)
    }
  }

  const uniqueSources = [...new Set(logs.map((log) => log.source).filter(Boolean))]

  return (
    <div className="space-y-5">
      <div className="flex flex-wrap items-end justify-between gap-3">
        <div>
          <p className="eyebrow mb-1">Observability</p>
          <h2 className="h-display text-[20px]">System logs</h2>
          <p className="text-[13px] text-zinc-500 mt-1 tabular">
            Showing {filteredLogs.length} of {logs.length} entries
          </p>
        </div>
        <div className="flex items-center gap-2">
          <label className="flex items-center gap-2 text-[13px] text-zinc-400 pr-1 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={logsAutoRefresh}
              onChange={(e) => setLogsAutoRefresh(e.target.checked)}
              className="w-3.5 h-3.5 rounded accent-zinc-100"
            />
            Auto-refresh
          </label>
          <button
            onClick={handleRefresh}
            className="btn-ghost !px-2.5 pressable"
            title="Refresh logs"
            aria-label="Refresh logs"
          >
            <RefreshCw className={cn('w-4 h-4', refreshing && 'animate-spin')} aria-hidden="true" />
          </button>
          <button
            onClick={exportLogs}
            className="btn-ghost !px-2.5 pressable"
            title="Export logs"
            aria-label="Export logs"
          >
            <Download className="w-4 h-4" aria-hidden="true" />
          </button>
        </div>
      </div>

      <div className="card p-4">
        <div className="grid grid-cols-1 md:grid-cols-[1fr_160px_180px] gap-2.5">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-zinc-600 pointer-events-none" aria-hidden="true" />
            <input
              type="text"
              placeholder="Search messages or sources…"
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="input !pl-9"
              aria-label="Search logs"
            />
          </div>

          <select
            value={levelFilter}
            onChange={(e) => setLevelFilter(e.target.value)}
            className="input"
            aria-label="Filter by level"
          >
            <option value="all">All levels</option>
            <option value="error">Error</option>
            <option value="warn">Warning</option>
            <option value="info">Info</option>
            <option value="debug">Debug</option>
          </select>

          <select
            value={sourceFilter}
            onChange={(e) => setSourceFilter(e.target.value)}
            className="input"
            aria-label="Filter by source"
          >
            <option value="all">All sources</option>
            {uniqueSources.map((source) => (
              <option key={source} value={source}>
                {source}
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="card overflow-hidden">
        <div className="max-h-[480px] overflow-auto">
          {filteredLogs.length === 0 ? (
            <div className="py-14 px-6 text-center">
              <p className="text-[14px] font-semibold text-zinc-200">No logs match your filters</p>
              <p className="text-[13px] text-zinc-500 mt-1">Try widening the search or clearing a filter.</p>
            </div>
          ) : (
            <div className="row-divider" role="log" aria-label="System logs">
              {filteredLogs.map((log) => (
                <div key={log.id} className="px-5 py-3 hover:bg-white/[0.02] transition-colors duration-[150ms]">
                  <div className="flex items-start gap-2.5">
                    <span
                      className={cn(
                        'px-2 py-0.5 rounded-md text-[11px] font-semibold uppercase tracking-[0.04em] border shrink-0 mt-0.5',
                        getLevelPill(log.level)
                      )}
                    >
                      {log.level}
                    </span>

                    <div className="flex-1 min-w-0">
                      <div className="flex flex-wrap items-center gap-x-2 gap-y-1 mb-1">
                        <span className="text-[11.5px] text-zinc-500 font-mono tabular">
                          {formatTimestamp(log.timestamp)}
                        </span>
                        <span className="text-[11px] text-zinc-400 bg-white/[0.05] border border-white/[0.07] px-1.5 py-0.5 rounded-md font-mono">
                          {log.source}
                        </span>
                        {log.taskId && (
                          <span className="text-[11px] text-zinc-300 bg-white/[0.05] border border-white/[0.07] px-1.5 py-0.5 rounded-md font-mono">
                            {log.taskId}
                          </span>
                        )}
                        {log.workerId && (
                          <span className="text-[11px] text-zinc-300 bg-white/[0.05] border border-white/[0.07] px-1.5 py-0.5 rounded-md font-mono">
                            {log.workerId}
                          </span>
                        )}
                      </div>

                      <p className="text-zinc-100 text-[13.5px] leading-relaxed break-words">{log.message}</p>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        {(['error', 'warn', 'info', 'debug'] as const).map((level) => {
          const count = logs.filter((log) => log.level === level).length
          return (
            <div key={level} className="card px-4 py-3.5">
              <p className="eyebrow capitalize">{level}</p>
              <p className="text-[20px] font-semibold text-white tracking-[-0.02em] tabular mt-1">{count}</p>
            </div>
          )
        })}
      </div>
    </div>
  )
}
