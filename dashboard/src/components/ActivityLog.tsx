import { useDashboardStore } from '../store'
import { formatTimestamp, cn } from '../lib/utils'
import type { LogEntry } from '../types'

function LogLevelBadge({ level }: { level: LogEntry['level'] }) {
  const colors = {
    info: 'text-zinc-300 border-white/10 bg-white/[0.04]',
    warn: 'text-amber-300 border-amber-500/20 bg-amber-500/[0.07]',
    error: 'text-red-300 border-red-500/20 bg-red-500/[0.07]',
    debug: 'text-zinc-500 border-white/[0.07] bg-transparent',
  }

  return (
    <span className={cn('uppercase text-[10px] font-semibold tracking-[0.06em] px-1.5 py-0.5 rounded border shrink-0', colors[level])}>
      {level}
    </span>
  )
}

export function ActivityLog() {
  const { logs, clearLogs } = useDashboardStore()

  return (
    <div className="card overflow-hidden">
      <div className="flex items-baseline justify-between gap-3 px-6 pt-5 pb-3">
        <div>
          <p className="eyebrow mb-1">Events</p>
          <h2 className="h-display text-[15px]">Activity log</h2>
        </div>
        <button
          onClick={clearLogs}
          className="text-[12px] font-medium text-zinc-500 hover:text-zinc-200 active:scale-[0.97] transition-all duration-[150ms]"
          aria-label="Clear activity log"
        >
          Clear
        </button>
      </div>

      <div className="mx-6 mb-6">
        <div
          className="card-dark rounded-lg px-4 py-3 font-mono text-[12px] leading-relaxed max-h-40 overflow-y-auto"
          role="log"
          aria-live="polite"
          aria-label="Activity log entries"
        >
          {logs.length === 0 ? (
            <p className="text-zinc-600">Waiting for activity…</p>
          ) : (
            <div className="space-y-1.5">
              {logs.map((log) => (
                <div key={log.id} className="flex gap-2.5 items-baseline">
                  <time className="text-zinc-600 tabular-nums flex-shrink-0 text-[11px]">
                    {formatTimestamp(log.timestamp)}
                  </time>
                  <LogLevelBadge level={log.level} />
                  <span className="text-zinc-300 truncate">{log.message}</span>
                  {log.source && (
                    <span className="text-zinc-600 flex-shrink-0 text-[11px]">[{log.source}]</span>
                  )}
                </div>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  )
}
