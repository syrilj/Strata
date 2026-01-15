import { useDashboardStore } from '../store'
import { formatTimestamp } from '../lib/utils'
import { useEffect, useState } from 'react'

export function Header() {
  const { coordinator } = useDashboardStore()
  const [time, setTime] = useState(formatTimestamp(Date.now()))
  
  useEffect(() => {
    const interval = setInterval(() => {
      setTime(formatTimestamp(Date.now()))
    }, 1000)
    return () => clearInterval(interval)
  }, [])
  
  return (
    <header className="flex items-center justify-between mb-8">
      <div>
        <h1 className="text-xl font-semibold text-white">Distributed Training Runtime</h1>
        <p className="text-sm text-zinc-500 mt-1">
          High-performance data loading, checkpointing & worker coordination
        </p>
      </div>
      
      <div className="flex items-center gap-4">
        <div className="flex items-center gap-2 px-3 py-1.5 rounded-full bg-zinc-800/50 text-sm">
          <span 
            className={`w-2 h-2 rounded-full ${coordinator.connected ? 'bg-emerald-500' : 'bg-red-500'}`}
            aria-hidden="true"
          />
          <span className="text-zinc-400">
            {coordinator.connected ? 'System Ready' : 'Disconnected'}
          </span>
        </div>
        
        <time className="text-xs text-zinc-600 tabular-nums" dateTime={new Date().toISOString()}>
          {time}
        </time>
      </div>
    </header>
  )
}
