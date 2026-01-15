import { useState, useEffect } from 'react'
import { Search, Filter, Download, RefreshCw } from 'lucide-react'
import { useDashboardStore } from '../store'
import { api } from '../lib/api'

interface LogEntry {
  id: string
  timestamp: number
  level: 'info' | 'warn' | 'error' | 'debug'
  message: string
  source: string
  task_id?: string
  worker_id?: string
}

export function SystemLogs() {
  const { logs } = useDashboardStore()
  const [filteredLogs, setFilteredLogs] = useState<LogEntry[]>([])
  const [searchTerm, setSearchTerm] = useState('')
  const [levelFilter, setLevelFilter] = useState<string>('all')
  const [sourceFilter, setSourceFilter] = useState<string>('all')
  const [autoRefresh, setAutoRefresh] = useState(true)

  useEffect(() => {
    let filtered = logs

    if (searchTerm) {
      filtered = filtered.filter(log =>
        log.message.toLowerCase().includes(searchTerm.toLowerCase()) ||
        (log.source || '').toLowerCase().includes(searchTerm.toLowerCase())
      )
    }

    if (levelFilter !== 'all') {
      filtered = filtered.filter(log => log.level === levelFilter)
    }

    if (sourceFilter !== 'all') {
      filtered = filtered.filter(log => log.source === sourceFilter)
    }

    setFilteredLogs(filtered)
  }, [logs, searchTerm, levelFilter, sourceFilter])

  const getLevelColor = (level: string) => {
    switch (level) {
      case 'error':
        return 'text-red-400 bg-red-500/10'
      case 'warn':
        return 'text-yellow-400 bg-yellow-500/10'
      case 'info':
        return 'text-blue-400 bg-blue-500/10'
      case 'debug':
        return 'text-gray-400 bg-gray-500/10'
      default:
        return 'text-zinc-400 bg-zinc-500/10'
    }
  }

  const formatTimestamp = (timestamp: number) => {
    return new Date(timestamp).toLocaleString()
  }

  const exportLogs = () => {
    const logText = filteredLogs
      .map(log => `[${formatTimestamp(log.timestamp)}] ${log.level.toUpperCase()} [${log.source}] ${log.message}`)
      .join('\n')
    
    const blob = new Blob([logText], { type: 'text/plain' })
    const url = URL.createObjectURL(blob)
    const a = document.createElement('a')
    a.href = url
    a.download = `system-logs-${new Date().toISOString().split('T')[0]}.txt`
    a.click()
    URL.revokeObjectURL(url)
  }

  const refreshLogs = async () => {
    try {
      const newLogs = await api.getLogs(500)
      useDashboardStore.getState().setLogs(newLogs)
    } catch (error) {
      if (import.meta.env.DEV) {
        console.error('Failed to refresh logs:', error)
      }
    }
  }

  const uniqueSources = [...new Set(logs.map(log => log.source).filter(Boolean))]

  return (
    <div className="space-y-6">
      {/* Header */}
      <div className="flex justify-between items-center">
        <h2 className="text-lg font-semibold text-white">System Activity Logs</h2>
        <div className="flex items-center gap-2">
          <label className="flex items-center gap-2 text-sm text-zinc-400">
            <input
              type="checkbox"
              checked={autoRefresh}
              onChange={(e) => setAutoRefresh(e.target.checked)}
              className="rounded"
            />
            Auto-refresh
          </label>
          <button
            onClick={refreshLogs}
            className="p-2 text-zinc-400 hover:text-white"
            title="Refresh logs"
          >
            <RefreshCw className="w-4 h-4" />
          </button>
          <button
            onClick={exportLogs}
            className="p-2 text-zinc-400 hover:text-white"
            title="Export logs"
          >
            <Download className="w-4 h-4" />
          </button>
        </div>
      </div>

      {/* Filters */}
      <div className="card p-4">
        <div className="grid grid-cols-1 md:grid-cols-4 gap-4">
          <div className="relative">
            <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 w-4 h-4 text-zinc-500" />
            <input
              type="text"
              placeholder="Search logs..."
              value={searchTerm}
              onChange={(e) => setSearchTerm(e.target.value)}
              className="w-full pl-10 pr-3 py-2 bg-zinc-700 border border-zinc-600 rounded text-white text-sm"
            />
          </div>

          <select
            value={levelFilter}
            onChange={(e) => setLevelFilter(e.target.value)}
            className="px-3 py-2 bg-zinc-700 border border-zinc-600 rounded text-white text-sm"
          >
            <option value="all">All Levels</option>
            <option value="error">Error</option>
            <option value="warn">Warning</option>
            <option value="info">Info</option>
            <option value="debug">Debug</option>
          </select>

          <select
            value={sourceFilter}
            onChange={(e) => setSourceFilter(e.target.value)}
            className="px-3 py-2 bg-zinc-700 border border-zinc-600 rounded text-white text-sm"
          >
            <option value="all">All Sources</option>
            {uniqueSources.map(source => (
              <option key={source} value={source}>{source}</option>
            ))}
          </select>

          <div className="text-sm text-zinc-400 flex items-center">
            Showing {filteredLogs.length} of {logs.length} logs
          </div>
        </div>
      </div>

      {/* Logs */}
      <div className="card">
        <div className="p-4 border-b border-zinc-700">
          <h3 className="font-medium text-white">Recent Activity</h3>
        </div>
        
        <div className="max-h-96 overflow-auto">
          {filteredLogs.length === 0 ? (
            <div className="p-8 text-center text-zinc-500">
              <Filter className="w-8 h-8 mx-auto mb-2" />
              <p>No logs match your filters</p>
            </div>
          ) : (
            <div className="divide-y divide-zinc-700">
              {filteredLogs.map((log) => (
                <div key={log.id} className="p-4 hover:bg-zinc-800/50">
                  <div className="flex items-start gap-3">
                    <span className={`px-2 py-1 rounded text-xs font-medium ${getLevelColor(log.level)}`}>
                      {log.level.toUpperCase()}
                    </span>
                    
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2 mb-1">
                        <span className="text-sm text-zinc-400">
                          {formatTimestamp(log.timestamp)}
                        </span>
                        <span className="text-xs text-zinc-500 bg-zinc-700 px-2 py-1 rounded">
                          {log.source}
                        </span>
                        {log.task_id && (
                          <span className="text-xs text-blue-400 bg-blue-500/10 px-2 py-1 rounded">
                            Task: {log.task_id}
                          </span>
                        )}
                        {log.worker_id && (
                          <span className="text-xs text-green-400 bg-green-500/10 px-2 py-1 rounded">
                            Worker: {log.worker_id}
                          </span>
                        )}
                      </div>
                      
                      <p className="text-white text-sm break-words">
                        {log.message}
                      </p>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Log Statistics */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        {['error', 'warn', 'info', 'debug'].map(level => {
          const count = logs.filter(log => log.level === level).length
          return (
            <div key={level} className="card p-4">
              <div className="flex items-center justify-between">
                <span className="text-sm text-zinc-400 capitalize">{level}</span>
                <span className={`text-lg font-semibold ${getLevelColor(level).split(' ')[0]}`}>
                  {count}
                </span>
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}