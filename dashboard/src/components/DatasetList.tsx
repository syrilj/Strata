import { Database, Shuffle } from 'lucide-react'
import { useDashboardStore } from '../store'
import { formatNumber, formatRelativeTime } from '../lib/utils'

export function DatasetList() {
  const { datasets } = useDashboardStore()
  
  if (datasets.length === 0) {
    return (
      <div className="card p-5">
        <h2 className="text-sm font-medium text-white mb-4">Datasets</h2>
        <p className="text-sm text-zinc-500 text-center py-4">No datasets registered</p>
      </div>
    )
  }
  
  return (
    <div className="card p-5">
      <h2 className="text-sm font-medium text-white mb-4">Datasets</h2>
      
      <div className="space-y-3" role="list" aria-label="Dataset list">
        {datasets.map((dataset) => (
          <div
            key={dataset.id}
            className="p-3 rounded-lg bg-zinc-800/30"
            role="listitem"
          >
            <div className="flex items-center justify-between mb-2">
              <div className="flex items-center gap-2">
                <Database className="w-4 h-4 text-blue-400" aria-hidden="true" />
                <span className="text-sm font-medium text-zinc-200">{dataset.name}</span>
              </div>
              {dataset.shuffle && (
                <span className="flex items-center gap-1 text-xs text-zinc-500">
                  <Shuffle className="w-3 h-3" aria-hidden="true" />
                  shuffle
                </span>
              )}
            </div>
            
            <div className="grid grid-cols-3 gap-2 text-xs">
              <div>
                <span className="text-zinc-500">Samples</span>
                <p className="text-zinc-300">{formatNumber(dataset.totalSamples)}</p>
              </div>
              <div>
                <span className="text-zinc-500">Shards</span>
                <p className="text-zinc-300">{dataset.shardCount}</p>
              </div>
              <div>
                <span className="text-zinc-500">Format</span>
                <p className="text-zinc-300">{dataset.format}</p>
              </div>
            </div>
            
            <p className="text-xs text-zinc-600 mt-2">
              Registered {formatRelativeTime(dataset.registeredAt)}
            </p>
          </div>
        ))}
      </div>
    </div>
  )
}
