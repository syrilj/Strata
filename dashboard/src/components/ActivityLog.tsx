import { useDashboardStore } from '../store'
import { formatTimestamp, cn } from '../lib/utils'
import type { LogEntry } from '../types'

function LogLevelBadge({ level }: { level: LogEntry['level'] }) {
  const colors = {
    info: 'text-blue-400',
    warn: 'text-amber-400',
    error: 'text-red-400',
    debug: 'text-zinc-500',
  }
  
  return <span className={cn('uppercase text-[10px] font-medium', colors[level])}>{level}</span>
}

export function ActivityLog() {
  const { logs, clearLogs } = useDashboardStore()
  
  return (
    <div className="card p-5 mt-6">
      <div className="flex items-center justify-between mb-3">
        <h2 className="text-sm font-medium text-white">Activity Log</h2>
        <button
          onClick={clearLogs}
          className="text-xs text-zinc-600 hover:text-zinc-400 transition-colors"
          aria-label="Clear activity log"
        >
          Clear
        </button>
      </div>
      
      <div
        className="card-dark rounded-lg p-4 font-mono text-xs max-h-32 overflow-y-auto"
        role="log"
        aria-live="polite"
        aria-label="Activity log entries"
      >
        {logs.length === 0 ? (
          <p className="text-zinc-600">Waiting for activity...</p>
        ) : (
          <div className="space-y-1">
            {logs.map((log) => (
              <div key={log.id} className="flex gap-3">
                <time className="text-zinc-600 tabular-nums flex-shrink-0">
                  {formatTimestamp(log.timestamp)}
                </time>
                <LogLevelBadge level={log.level} />
                <span className="text-zinc-400 truncate">{log.message}</span>
                {log.source && (
                  <span className="text-zinc-600 flex-shrink-0">[{log.source}]</span>
                )}
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
