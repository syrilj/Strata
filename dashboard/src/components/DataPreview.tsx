import { useEffect, useMemo, useState } from 'react'
import { Database, FileText, Layers, RefreshCw, Server } from 'lucide-react'
import { api } from '../lib/api'
import { formatNumber } from '../lib/utils'
import type { DataSample, DatasetPreviewResponse, DatasetShardsResponse } from '../types'

interface DataPreviewProps {
  datasetId: string
  samples?: DataSample[]
  isLoading?: boolean
}

function formatCellValue(value: unknown): string {
  if (value === null || value === undefined) return '—'
  if (typeof value === 'number') {
    return Number.isInteger(value) ? value.toLocaleString() : value.toFixed(4).replace(/\.?0+$/, '')
  }
  if (typeof value === 'boolean') return value ? 'true' : 'false'
  if (typeof value === 'string') return value
  try {
    return JSON.stringify(value)
  } catch {
    return String(value)
  }
}

export function DataPreview({ datasetId, samples: propSamples, isLoading: propLoading = false }: DataPreviewProps) {
  const [preview, setPreview] = useState<DatasetPreviewResponse | null>(null)
  const [shards, setShards] = useState<DatasetShardsResponse | null>(null)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const loadPreview = async () => {
    if (!datasetId) return
    setLoading(true)
    setError(null)
    try {
      const [previewRes, shardsRes] = await Promise.all([
        api.getDatasetPreview(datasetId),
        api.getDatasetShards(datasetId).catch(() => null),
      ])
      setPreview(previewRes)
      setShards(shardsRes)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load dataset preview')
      setPreview(null)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    if (propSamples || !datasetId) return
    void loadPreview()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [datasetId, propSamples])

  const displaySamples = useMemo(
    () => propSamples ?? preview?.samples ?? [],
    [propSamples, preview?.samples]
  )
  const columns = useMemo(() => {
    if (preview?.columns && preview.columns.length > 0 && !propSamples) {
      return preview.columns
    }
    return Array.from(new Set(displaySamples.flatMap((s) => Object.keys(s.features || {}))))
  }, [preview?.columns, propSamples, displaySamples])

  const hasLabelColumn = useMemo(
    () => displaySamples.some((s) => s.label !== undefined && s.label !== null),
    [displaySamples]
  )

  const isLoading = propLoading || loading

  if (!datasetId) {
    return (
      <div className="card p-6">
        <p className="eyebrow mb-1">Samples</p>
        <h2 className="h-display text-[15px]">Data preview</h2>
        <p className="text-[13px] text-zinc-500 mt-3">Register or select a dataset to inspect samples and shard distribution.</p>
      </div>
    )
  }

  if (isLoading) {
    return (
      <div className="card p-6">
        <p className="eyebrow mb-1">Samples</p>
        <h2 className="h-display text-[15px]">Data preview</h2>
        <div className="mt-4 space-y-2" aria-label="Loading">
          {[0, 1, 2].map((i) => (
            <div key={i} className="skeleton h-10 w-full" />
          ))}
        </div>
      </div>
    )
  }

  const totalShards =
    shards?.totalShards ?? new Set(displaySamples.map((s) => s.shardId)).size
  const uniqueWorkers =
    shards && shards.shards.length > 0
      ? new Set(shards.shards.map((s) => s.workerId).filter(Boolean)).size
      : new Set(displaySamples.map((s) => s.workerId).filter(Boolean)).size

  return (
    <div className="card overflow-hidden">
      <div className="px-6 pt-6 pb-4">
        <div className="flex items-baseline justify-between gap-3">
          <div className="min-w-0">
            <p className="eyebrow mb-1">Samples</p>
            <h2 className="h-display text-[15px] truncate font-mono">Data preview — {datasetId}</h2>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <span className="text-[11px] text-zinc-500 tabular">{displaySamples.length} rows</span>
            {!propSamples && (
              <button
                type="button"
                onClick={loadPreview}
                className="p-1.5 rounded-lg text-zinc-500 hover:text-zinc-200 hover:bg-white/[0.06] transition-colors"
                title="Refresh preview"
                aria-label="Refresh dataset preview"
              >
                <RefreshCw className="w-3.5 h-3.5" aria-hidden="true" />
              </button>
            )}
          </div>
        </div>
        <p className="text-[13px] text-zinc-500 mt-1.5 leading-relaxed">
          {preview?.format
            ? `Format: ${preview.format} · ${formatNumber(preview.totalSamples)} total samples`
            : 'Live sample rows and shard assignments from coordinator'}
        </p>
      </div>

      <div className="px-6 pb-6 space-y-4">
        <div className="grid grid-cols-3 gap-2.5">
          {[
            { icon: FileText, label: 'Preview rows', value: displaySamples.length },
            { icon: Layers, label: 'Shards', value: totalShards },
            { icon: Server, label: 'Assigned workers', value: uniqueWorkers },
          ].map(({ icon: Icon, label, value }) => (
            <div key={label} className="flex items-center gap-2.5 px-3 py-2.5 rounded-lg bg-white/[0.02] border border-white/[0.06]">
              <Icon className="h-4 w-4 text-zinc-400 shrink-0" aria-hidden="true" />
              <div className="min-w-0">
                <p className="text-[11px] text-zinc-500 font-medium">{label}</p>
                <p className="text-[15px] font-semibold text-zinc-100 tabular leading-tight">{value}</p>
              </div>
            </div>
          ))}
        </div>

        {error && (
          <div className="px-4 py-3 rounded-[10px] bg-red-500/[0.06] border border-red-500/20 text-[12.5px] text-red-200">
            {error}
          </div>
        )}

        {displaySamples.length === 0 ? (
          <div className="border border-white/[0.07] rounded-[10px] p-5 bg-white/[0.02] space-y-3">
            <p className="text-[13px] font-medium text-zinc-200">
              No inline preview rows available for this dataset.
            </p>
            <p className="text-[12.5px] text-zinc-500 leading-relaxed">
              {preview?.note ||
                'The coordinator holds dataset metadata and shard assignments; sample rows appear when the dataset path is accessible locally as JSONL/CSV/Parquet.'}
            </p>
            {shards && shards.shards.length > 0 && (
              <div className="pt-2 border-t border-white/[0.06]">
                <p className="text-[11px] font-semibold uppercase tracking-[0.08em] text-zinc-500 mb-2">
                  Shard distribution ({shards.totalShards} shards · {formatNumber(shards.shardSize)} samples/shard)
                </p>
                <div className="grid grid-cols-2 sm:grid-cols-3 md:grid-cols-4 gap-2">
                  {shards.shards.slice(0, 12).map((shard) => (
                    <div
                      key={shard.shardId}
                      className="px-2.5 py-2 rounded-lg bg-[#0e0e10] border border-white/[0.06] text-[11.5px]"
                    >
                      <div className="flex items-center justify-between gap-2">
                        <span className="font-mono text-zinc-200">S{shard.shardId}</span>
                        <span className="text-zinc-500 tabular">{shard.sampleCount}</span>
                      </div>
                      <p className="text-zinc-500 font-mono truncate mt-0.5">{shard.workerId || 'unassigned'}</p>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        ) : (
          <div className="border border-white/[0.07] rounded-[10px] overflow-hidden">
            <div className="overflow-x-auto">
              <table className="w-full text-sm">
                <thead className="bg-white/[0.02] border-b border-white/[0.07]">
                  <tr>
                    <th className="table-head">Sample ID</th>
                    {columns.map((col) => (
                      <th key={col} className="table-head font-mono">
                        {col}
                      </th>
                    ))}
                    {hasLabelColumn && <th className="table-head">Label</th>}
                    <th className="table-head">Shard</th>
                    <th className="table-head">Worker</th>
                  </tr>
                </thead>
                <tbody className="divide-y divide-white/[0.06]">
                  {displaySamples.map((sample) => (
                    <tr key={sample.id} className="hover:bg-white/[0.02] transition-colors duration-[150ms]">
                      <td className="px-3 py-2.5 font-mono text-[12px] text-zinc-400 whitespace-nowrap">
                        <span className="inline-flex items-center gap-1.5">
                          <Database className="w-3 h-3 text-zinc-600" aria-hidden="true" />
                          {sample.id}
                        </span>
                      </td>
                      {columns.map((col) => (
                        <td
                          key={col}
                          className="px-3 py-2.5 text-[12.5px] text-zinc-300 tabular max-w-[220px] truncate font-mono"
                          title={formatCellValue(sample.features?.[col])}
                        >
                          {formatCellValue(sample.features?.[col])}
                        </td>
                      ))}
                      {hasLabelColumn && (
                        <td className="px-3 py-2.5 text-[12.5px] font-semibold text-emerald-300 tabular whitespace-nowrap">
                          {formatCellValue(sample.label)}
                        </td>
                      )}
                      <td className="px-3 py-2.5 whitespace-nowrap">
                        <span className="px-1.5 py-0.5 rounded-md bg-white/[0.04] border border-white/[0.07] text-zinc-300 text-[11.5px] font-mono tabular">
                          S{sample.shardId}
                        </span>
                      </td>
                      <td className="px-3 py-2.5 text-[12px] text-zinc-500 font-mono whitespace-nowrap">
                        {sample.workerId || '—'}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
