import { useState } from 'react'
import { Database, Loader2, Plus, Shuffle, Trash2, X } from 'lucide-react'
import { useDashboardStore } from '../store'
import { cn, formatNumber, formatRelativeTime } from '../lib/utils'

interface RegisterDatasetDialogProps {
  onClose: () => void
  onSubmit: (req: {
    name: string
    path: string
    format: string
    total_samples?: number
    shard_size: number
    shuffle: boolean
  }) => Promise<void>
}

function RegisterDatasetDialog({ onClose, onSubmit }: RegisterDatasetDialogProps) {
  const [name, setName] = useState('')
  const [path, setPath] = useState('')
  const [format, setFormat] = useState('parquet')
  const [totalSamples, setTotalSamples] = useState('100000')
  const [shardSize, setShardSize] = useState('10000')
  const [shuffle, setShuffle] = useState(true)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    setSubmitting(true)
    setError(null)
    try {
      const parsedTotal = totalSamples.trim() ? parseInt(totalSamples, 10) : undefined
      const parsedShard = parseInt(shardSize, 10)
      await onSubmit({
        name: name.trim(),
        path: path.trim(),
        format,
        total_samples: Number.isFinite(parsedTotal) ? parsedTotal : undefined,
        shard_size: Number.isFinite(parsedShard) && parsedShard > 0 ? parsedShard : 1000,
        shuffle,
      })
      onClose()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to register dataset')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <div className="fixed inset-0 bg-black/70 backdrop-blur-sm flex items-center justify-center z-50 p-4" onClick={onClose} role="presentation">
      <div
        className="bg-[#141417] border border-white/10 rounded-[12px] shadow-pop p-6 w-full max-w-md"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-label="Register dataset"
      >
        <div className="flex items-start justify-between gap-3 mb-1">
          <div>
            <p className="eyebrow mb-1">Storage</p>
            <h3 className="h-display text-[17px]">Register dataset</h3>
          </div>
          <button
            type="button"
            onClick={onClose}
            className="p-1.5 rounded-lg text-zinc-500 hover:text-white hover:bg-white/[0.06] transition-colors"
            aria-label="Close dialog"
          >
            <X className="w-4 h-4" aria-hidden="true" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4 mt-4">
          {error && (
            <p className="text-[12.5px] text-red-300 bg-red-500/[0.08] border border-red-500/20 rounded-lg px-3 py-2">
              {error}
            </p>
          )}

          <div>
            <label className="label" htmlFor="dataset-name">Name</label>
            <input
              id="dataset-name"
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              className="input"
              placeholder="imagenet-train"
              required
            />
          </div>

          <div>
            <label className="label" htmlFor="dataset-path">Path</label>
            <input
              id="dataset-path"
              type="text"
              value={path}
              onChange={(e) => setPath(e.target.value)}
              className="input font-mono"
              placeholder="/data/imagenet/train"
              required
            />
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <label className="label" htmlFor="dataset-format">Format</label>
              <select
                id="dataset-format"
                value={format}
                onChange={(e) => setFormat(e.target.value)}
                className="input"
              >
                <option value="parquet">parquet</option>
                <option value="tfrecord">tfrecord</option>
                <option value="jsonl">jsonl</option>
                <option value="csv">csv</option>
                <option value="webdataset">webdataset</option>
              </select>
            </div>
            <div>
              <label className="label" htmlFor="dataset-shard-size">Shard size</label>
              <input
                id="dataset-shard-size"
                type="number"
                min={1}
                value={shardSize}
                onChange={(e) => setShardSize(e.target.value)}
                className="input tabular"
                required
              />
            </div>
          </div>

          <div>
            <label className="label" htmlFor="dataset-total-samples">Total samples (optional)</label>
            <input
              id="dataset-total-samples"
              type="number"
              min={0}
              value={totalSamples}
              onChange={(e) => setTotalSamples(e.target.value)}
              className="input tabular"
            />
          </div>

          <label className="flex items-center gap-2 text-[13px] text-zinc-300 cursor-pointer select-none">
            <input
              type="checkbox"
              checked={shuffle}
              onChange={(e) => setShuffle(e.target.checked)}
              className="w-3.5 h-3.5 rounded accent-zinc-100"
            />
            Enable deterministic shard shuffling
          </label>

          <div className="flex gap-2.5 pt-2">
            <button type="button" onClick={onClose} className="btn-ghost flex-1 pressable">
              Cancel
            </button>
            <button type="submit" disabled={submitting} className="btn-primary flex-1 pressable">
              {submitting ? <Loader2 className="w-4 h-4 animate-spin" aria-hidden="true" /> : 'Register'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}

export function DatasetList() {
  const { datasets, selectedDatasetId, setSelectedDatasetId, registerDataset, deleteDataset } =
    useDashboardStore()
  const [showRegister, setShowRegister] = useState(false)
  const [deletingId, setDeletingId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const handleDelete = async (e: React.MouseEvent, id: string) => {
    e.stopPropagation()
    setDeletingId(id)
    setError(null)
    try {
      await deleteDataset(id)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to delete dataset')
    } finally {
      setDeletingId(null)
    }
  }

  return (
    <div className="card overflow-hidden">
      <div className="flex items-center justify-between gap-3 px-5 pt-5 pb-3">
        <div>
          <p className="eyebrow mb-1">Storage</p>
          <h2 className="h-display text-[15px]">Datasets</h2>
        </div>
        <div className="flex items-center gap-2.5">
          {datasets.length > 0 && (
            <span className="text-[11px] text-zinc-500 tabular">{datasets.length} registered</span>
          )}
          <button
            type="button"
            onClick={() => setShowRegister(true)}
            className="btn-ghost !py-1.5 !px-2.5 text-xs pressable"
            aria-label="Register dataset"
          >
            <Plus className="w-3.5 h-3.5" aria-hidden="true" />
            Register
          </button>
        </div>
      </div>

      {error && (
        <p className="mx-5 mb-3 text-[12px] text-red-300 bg-red-500/[0.08] border border-red-500/20 rounded-lg px-3 py-2">
          {error}
        </p>
      )}

      {datasets.length === 0 ? (
        <p className="text-[13px] text-zinc-500 text-center px-5 pb-6 pt-2">No datasets registered</p>
      ) : (
        <div className="row-divider border-t border-white/[0.06]" role="list" aria-label="Dataset list">
          {datasets.map((dataset) => {
            const isSelected = selectedDatasetId === dataset.id
            const isDeleting = deletingId === dataset.id

            return (
              <div
                key={dataset.id}
                onClick={() => setSelectedDatasetId(dataset.id)}
                className={cn(
                  'px-5 py-3.5 hover:bg-white/[0.03] transition-colors duration-[150ms] cursor-pointer group',
                  isSelected && 'bg-white/[0.04]'
                )}
                role="listitem"
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="flex items-center gap-2 min-w-0">
                    <Database className="w-3.5 h-3.5 text-zinc-400 shrink-0" aria-hidden="true" />
                    <span className="text-[13.5px] font-semibold text-zinc-100 truncate tracking-[-0.005em]">
                      {dataset.name}
                    </span>
                  </div>
                  <div className="flex items-center gap-2 shrink-0">
                    {dataset.shuffle && (
                      <span className="flex items-center gap-1 text-[11px] text-zinc-500">
                        <Shuffle className="w-3 h-3" aria-hidden="true" />
                        shuffle
                      </span>
                    )}
                    <button
                      type="button"
                      onClick={(e) => handleDelete(e, dataset.id)}
                      disabled={isDeleting}
                      className="p-1 rounded-md text-zinc-600 hover:text-red-300 hover:bg-red-500/[0.08] opacity-70 group-hover:opacity-100 focus-visible:opacity-100 transition-all duration-[150ms] disabled:opacity-40"
                      title="Delete dataset"
                      aria-label={`Delete dataset ${dataset.name}`}
                    >
                      {isDeleting ? (
                        <Loader2 className="w-3.5 h-3.5 animate-spin" aria-hidden="true" />
                      ) : (
                        <Trash2 className="w-3.5 h-3.5" aria-hidden="true" />
                      )}
                    </button>
                  </div>
                </div>

                <div className="grid grid-cols-3 gap-2 mt-2.5">
                  <div>
                    <p className="text-[11px] text-zinc-500">Samples</p>
                    <p className="text-[13px] text-zinc-200 font-semibold tabular">
                      {formatNumber(dataset.totalSamples)}
                    </p>
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
            )
          })}
        </div>
      )}

      {showRegister && (
        <RegisterDatasetDialog
          onClose={() => setShowRegister(false)}
          onSubmit={async (req) => {
            await registerDataset(req)
          }}
        />
      )}
    </div>
  )
}
