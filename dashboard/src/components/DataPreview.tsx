import { Database, FileText, Layers, Server } from 'lucide-react';
import { cn } from '../lib/utils';

interface DataSample {
  id: string;
  features: Record<string, any>;
  label?: any;
  shard_id: number;
  worker_id?: string;
}

interface DataPreviewProps {
  datasetId: string;
  samples?: DataSample[];
  isLoading?: boolean;
}

// Generate placeholder sample visualizations for demonstration
// Note: The coordinator stores dataset metadata (size, shards, format) but not actual samples.
// This generates representative placeholder data to show what the dataset structure looks like.
const generateMockSamples = (datasetId: string): DataSample[] => {
  if (datasetId.includes('stock') || datasetId.includes('AAPL')) {
    return [
      {
        id: 'sample_0001',
        features: {
          open: 178.32,
          high: 180.15,
          low: 177.89,
          close: 179.45,
          volume: 52_340_000,
          ma_7: 177.23,
          ma_30: 175.89,
          rsi: 62.4,
        },
        label: 181.20,
        shard_id: 0,
        worker_id: 'gpu-node-1',
      },
      {
        id: 'sample_0002',
        features: {
          open: 179.45,
          high: 182.30,
          low: 179.10,
          close: 181.90,
          volume: 48_920_000,
          ma_7: 178.45,
          ma_30: 176.12,
          rsi: 68.2,
        },
        label: 183.50,
        shard_id: 0,
        worker_id: 'gpu-node-1',
      },
      {
        id: 'sample_0003',
        features: {
          open: 181.90,
          high: 183.75,
          low: 180.50,
          close: 182.15,
          volume: 51_230_000,
          ma_7: 179.89,
          ma_30: 176.78,
          rsi: 71.5,
        },
        label: 180.90,
        shard_id: 1,
        worker_id: 'gpu-node-2',
      },
    ];
  }

  const imageLabels = [
    'tench (fish)',
    'goldfish',
    'great white shark',
    'tiger shark',
    'hammerhead shark',
    'electric ray',
    'stingray',
    'rooster',
    'hen',
  ];

  return imageLabels.map((label, idx) => ({
    id: `img_${String(idx + 1).padStart(4, '0')}`,
    features: {
      image_path: `/data/imagenet/train/n0144${String(idx).padStart(4, '0')}/image_${idx}.JPEG`,
      width: 224,
      height: 224,
      channels: 3,
      mean_rgb: [0.485 + Math.random() * 0.1, 0.456 + Math.random() * 0.1, 0.406 + Math.random() * 0.1],
    },
    label,
    shard_id: Math.floor(idx / 3),
    worker_id: `gpu-node-${(idx % 3) + 1}`,
  }));
};

export function DataPreview({ datasetId, samples, isLoading = false }: DataPreviewProps) {
  const displaySamples = samples || generateMockSamples(datasetId);
  const isStockData = datasetId.includes('stock') || datasetId.includes('AAPL');

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
    );
  }

  return (
    <div className="card overflow-hidden">
      <div className="px-6 pt-6 pb-4">
        <div className="flex items-baseline justify-between gap-3">
          <div className="min-w-0">
            <p className="eyebrow mb-1">Samples</p>
            <h2 className="h-display text-[15px] truncate font-mono">Data preview — {datasetId}</h2>
          </div>
          <span className="text-[11px] text-zinc-500 tabular shrink-0">{displaySamples.length} rows</span>
        </div>
        <p className="text-[13px] text-zinc-500 mt-1.5 leading-relaxed">
          {isStockData
            ? 'Stock price samples with technical indicators'
            : 'Image samples being processed by workers'}
        </p>
      </div>

      <div className="px-6 pb-6 space-y-4">
        <div className="grid grid-cols-3 gap-2.5">
          {[
            { icon: FileText, label: 'Samples', value: displaySamples.length },
            { icon: Layers, label: 'Shards', value: new Set(displaySamples.map((s) => s.shard_id)).size },
            { icon: Server, label: 'Workers', value: new Set(displaySamples.map((s) => s.worker_id).filter(Boolean)).size },
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

        <div className="border border-white/[0.07] rounded-[10px] overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="bg-white/[0.02] border-b border-white/[0.07]">
                <tr>
                  <th className="table-head">Sample ID</th>
                  {isStockData ? (
                    <>
                      <th className="table-head">Open</th>
                      <th className="table-head">High</th>
                      <th className="table-head">Low</th>
                      <th className="table-head">Close</th>
                      <th className="table-head">Volume</th>
                      <th className="table-head">RSI</th>
                      <th className="table-head">Target</th>
                    </>
                  ) : (
                    <>
                      <th className="table-head">Label</th>
                      <th className="table-head">Dimensions</th>
                      <th className="table-head">Path</th>
                    </>
                  )}
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
                    {isStockData ? (
                      <>
                        <td className="px-3 py-2.5 text-zinc-300 tabular whitespace-nowrap">${sample.features.open.toFixed(2)}</td>
                        <td className="px-3 py-2.5 text-zinc-300 tabular whitespace-nowrap">${sample.features.high.toFixed(2)}</td>
                        <td className="px-3 py-2.5 text-zinc-300 tabular whitespace-nowrap">${sample.features.low.toFixed(2)}</td>
                        <td className="px-3 py-2.5 font-semibold text-zinc-100 tabular whitespace-nowrap">
                          ${sample.features.close.toFixed(2)}
                        </td>
                        <td className="px-3 py-2.5 text-[12px] text-zinc-400 tabular whitespace-nowrap">
                          {(sample.features.volume / 1_000_000).toFixed(1)}M
                        </td>
                        <td className="px-3 py-2.5 whitespace-nowrap">
                          <span
                            className={cn(
                              'px-1.5 py-0.5 rounded-md text-[11.5px] font-semibold tabular border',
                              sample.features.rsi > 70
                                ? 'bg-amber-500/[0.08] text-amber-200 border-amber-500/20'
                                : 'bg-white/[0.04] text-zinc-300 border-white/10'
                            )}
                          >
                            {sample.features.rsi.toFixed(1)}
                          </span>
                        </td>
                        <td className="px-3 py-2.5 font-semibold text-emerald-300 tabular whitespace-nowrap">
                          ${sample.label.toFixed(2)}
                        </td>
                      </>
                    ) : (
                      <>
                        <td className="px-3 py-2.5 text-zinc-200 text-[13px] max-w-[180px] truncate">{sample.label}</td>
                        <td className="px-3 py-2.5 text-zinc-400 tabular text-[12.5px] whitespace-nowrap">
                          {sample.features.width}×{sample.features.height}×{sample.features.channels}
                        </td>
                        <td className="px-3 py-2.5 font-mono text-[11.5px] text-zinc-500 max-w-[220px] truncate">
                          {sample.features.image_path}
                        </td>
                      </>
                    )}
                    <td className="px-3 py-2.5 whitespace-nowrap">
                      <span className="px-1.5 py-0.5 rounded-md bg-white/[0.04] border border-white/[0.07] text-zinc-300 text-[11.5px] font-mono tabular">
                        S{sample.shard_id}
                      </span>
                    </td>
                    <td className="px-3 py-2.5 text-[12px] text-zinc-500 font-mono whitespace-nowrap">{sample.worker_id || '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>

        <div className="text-[12.5px] text-zinc-500 px-4 py-3.5 bg-white/[0.02] border border-white/[0.06] rounded-[10px] leading-relaxed">
          <span className="text-zinc-200 font-semibold">About this data — </span>
          {isStockData
            ? 'OHLC prices, volume, and technical indicators (MA, RSI) for next-day price prediction. Each worker receives a different shard.'
            : 'Images are preprocessed to 224×224 with normalized RGB channels. Each worker receives a different shard.'}
        </div>
      </div>
    </div>
  );
}
