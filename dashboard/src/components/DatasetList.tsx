import { Database, Shuffle } from 'lucide-react'
import { useDashboardStore } from '../store'
import { formatNumber, formatRelativeTime } from '../lib/utils'

export function DatasetList() {
  const { datasets } = useDashboardStore()

  return (
    <div className="card overflow-hidden">
      <div className="flex items-baseline justify-between gap-3 px-5 pt-5 pb-3">
        <div>
          <p className="eyebrow mb-1">Storage</p>
          <h2 className="h-display text-[15px]">Datasets</h2>
        </div>
        {datasets.length > 0 && (
          <span className="text-[11px] text-zinc-500 tabular">{datasets.length} registered</span>
        )}
      </div>

      {datasets.length === 0 ? (
        <p className="text-[13px] text-zinc-500 text-center px-5 pb-6 pt-2">No datasets registered</p>
      ) : (
        <div className="row-divider border-t border-white/[0.06]" role="list" aria-label="Dataset list">
          {datasets.map((dataset) => (
            <div key={dataset.id} className="px-5 py-3.5 hover:bg-white/[0.02] transition-colors duration-[150ms]" role="listitem">
              <div className="flex items-center justify-between gap-2">
                <div className="flex items-center gap-2 min-w-0">
                  <Database className="w-3.5 h-3.5 text-zinc-400 shrink-0" aria-hidden="true" />
                  <span className="text-[13.5px] font-semibold text-zinc-100 truncate tracking-[-0.005em]">{dataset.name}</span>
                </div>
                {dataset.shuffle && (
                  <span className="flex items-center gap-1 text-[11px] text-zinc-500 shrink-0">
                    <Shuffle className="w-3 h-3" aria-hidden="true" />
                    shuffle
                  </span>
                )}
              </div>

              <div className="grid grid-cols-3 gap-2 mt-2.5">
                <div>
                  <p className="text-[11px] text-zinc-500">Samples</p>
                  <p className="text-[13px] text-zinc-200 font-semibold tabular">{formatNumber(dataset.totalSamples)}</p>
                </div>
                <div>
                  <p className="text-[11px] text-zinc-500">Shards</p>
                  <p className="text-[13px] text-zinc-200 font-semibold tabular">{dataset.shardCount}</p>
                </div>
                <div>
                  <p className="text-[11px] text-zinc-500">Format</p>
                  <p className="text-[13px] text-zinc-200 font-mono">{dataset.format}</p>
                </div>
              </div>

              <p className="text-[11px] text-zinc-600 mt-2 tabular">
                Registered {formatRelativeTime(dataset.registeredAt)}
              </p>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
