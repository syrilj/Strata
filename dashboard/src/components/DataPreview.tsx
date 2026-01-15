import { Database, FileText, TrendingUp, Activity } from 'lucide-react';
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

// Generate placeholder image with label using CSS gradient
const getPlaceholderStyle = (label: string, seed: number) => {
  // Create a deterministic color based on label
  const hash = label.split('').reduce((acc, char) => acc + char.charCodeAt(0), seed);
  const hue = hash % 360;
  const saturation = 60 + (hash % 30);
  const lightness = 45 + (hash % 15);
  
  return {
    background: `linear-gradient(135deg, hsl(${hue}, ${saturation}%, ${lightness}%) 0%, hsl(${(hue + 40) % 360}, ${saturation}%, ${lightness - 10}%) 100%)`,
  };
};

// Get emoji for label
const getEmojiForLabel = (label: string): string => {
  const emojiMap: Record<string, string> = {
    'tench': '🐟',
    'goldfish': '🐠',
    'shark': '🦈',
    'ray': '🐡',
    'stingray': '🐡',
    'rooster': '🐓',
    'hen': '🐔',
    'ostrich': '🦤',
    'bird': '🐦',
    'brambling': '🐦',
    'goldfinch': '🐦',
    'finch': '🐦',
    'junco': '🐦',
    'bunting': '🐦',
  };
  
  for (const [key, emoji] of Object.entries(emojiMap)) {
    if (label.toLowerCase().includes(key)) {
      return emoji;
    }
  }
  return '🖼️';
};

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
        label: 181.20, // Next day close price
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

  // ImageNet-style data with more samples
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
    'ostrich',
    'brambling',
    'goldfinch',
    'house finch',
    'junco',
    'indigo bunting',
  ];

  return imageLabels.slice(0, 9).map((label, idx) => ({
    id: `img_${String(idx + 1).padStart(4, '0')}`,
    features: {
      image_path: `/data/imagenet/train/n0144${String(idx).padStart(4, '0')}/image_${idx}.JPEG`,
      width: 224,
      height: 224,
      channels: 3,
      mean_rgb: [0.485 + Math.random() * 0.1, 0.456 + Math.random() * 0.1, 0.406 + Math.random() * 0.1],
      placeholderStyle: getPlaceholderStyle(label, idx),
      emoji: getEmojiForLabel(label),
    },
    label,
    shard_id: Math.floor(idx / 3),
    worker_id: `gpu-node-${(idx % 3) + 1}`,
  }));
};

export function DataPreview({ datasetId, samples, isLoading = false }: DataPreviewProps) {
  // Use provided samples if available, otherwise generate placeholders for visualization
  const displaySamples = samples || generateMockSamples(datasetId);
  const isStockData = datasetId.includes('stock') || datasetId.includes('AAPL');

  if (isLoading) {
    return (
      <div className="card p-6">
        <div className="flex items-center gap-2 mb-4">
          <Database className="h-5 w-5 text-zinc-400" />
          <h2 className="text-sm font-medium text-white">Data Preview</h2>
        </div>
        <div className="flex items-center justify-center h-32 text-zinc-500">
          Loading data samples...
        </div>
      </div>
    );
  }

  return (
    <div className="card p-6">
      <div className="flex items-center gap-2 mb-2">
        <Database className="h-5 w-5 text-zinc-400" />
        <h2 className="text-sm font-medium text-white">Data Preview - {datasetId}</h2>
      </div>
      <p className="text-xs text-zinc-500 mb-4">
        {isStockData 
          ? `Showing ${displaySamples.length} stock price samples with technical indicators`
          : `Showing ${displaySamples.length} image samples being processed by workers`
        }
      </p>

      <div className="space-y-4">
        {/* Data Statistics */}
        <div className="grid grid-cols-3 gap-4">
          <div className="flex items-center gap-2 p-3 bg-zinc-800/30 rounded-lg">
            <FileText className="h-4 w-4 text-blue-400" />
            <div>
              <div className="text-xs text-zinc-500">Samples</div>
              <div className="text-lg font-semibold text-zinc-200">{displaySamples.length}</div>
            </div>
          </div>
          <div className="flex items-center gap-2 p-3 bg-zinc-800/30 rounded-lg">
            <TrendingUp className="h-4 w-4 text-green-400" />
            <div>
              <div className="text-xs text-zinc-500">Shards</div>
              <div className="text-lg font-semibold text-zinc-200">
                {new Set(displaySamples.map((s) => s.shard_id)).size}
              </div>
            </div>
          </div>
          <div className="flex items-center gap-2 p-3 bg-zinc-800/30 rounded-lg">
            <Activity className="h-4 w-4 text-purple-400" />
            <div>
              <div className="text-xs text-zinc-500">Workers</div>
              <div className="text-lg font-semibold text-zinc-200">
                {new Set(displaySamples.map((s) => s.worker_id).filter(Boolean)).size}
              </div>
            </div>
          </div>
        </div>

        {/* Sample Data - Always use table view */}
        <div className="border border-zinc-800 rounded-lg overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-sm">
              <thead className="bg-zinc-800/50">
                <tr>
                  <th className="px-3 py-2 text-left text-xs font-medium text-zinc-400">Sample ID</th>
                  {isStockData ? (
                    <>
                      <th className="px-3 py-2 text-left text-xs font-medium text-zinc-400">Open</th>
                      <th className="px-3 py-2 text-left text-xs font-medium text-zinc-400">High</th>
                      <th className="px-3 py-2 text-left text-xs font-medium text-zinc-400">Low</th>
                      <th className="px-3 py-2 text-left text-xs font-medium text-zinc-400">Close</th>
                      <th className="px-3 py-2 text-left text-xs font-medium text-zinc-400">Volume</th>
                      <th className="px-3 py-2 text-left text-xs font-medium text-zinc-400">RSI</th>
                      <th className="px-3 py-2 text-left text-xs font-medium text-zinc-400">Target</th>
                    </>
                  ) : (
                    <>
                      <th className="px-3 py-2 text-left text-xs font-medium text-zinc-400">Label</th>
                      <th className="px-3 py-2 text-left text-xs font-medium text-zinc-400">Dimensions</th>
                      <th className="px-3 py-2 text-left text-xs font-medium text-zinc-400">Path</th>
                    </>
                  )}
                  <th className="px-3 py-2 text-left text-xs font-medium text-zinc-400">Shard</th>
                  <th className="px-3 py-2 text-left text-xs font-medium text-zinc-400">Worker</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-zinc-800">
                {displaySamples.map((sample) => (
                  <tr key={sample.id} className="hover:bg-zinc-800/30">
                    <td className="px-3 py-2 font-mono text-xs text-zinc-400">{sample.id}</td>
                    {isStockData ? (
                      <>
                        <td className="px-3 py-2 text-zinc-300">${sample.features.open.toFixed(2)}</td>
                        <td className="px-3 py-2 text-zinc-300">${sample.features.high.toFixed(2)}</td>
                        <td className="px-3 py-2 text-zinc-300">${sample.features.low.toFixed(2)}</td>
                        <td className="px-3 py-2 font-semibold text-zinc-200">
                          ${sample.features.close.toFixed(2)}
                        </td>
                        <td className="px-3 py-2 text-xs text-zinc-400">
                          {(sample.features.volume / 1_000_000).toFixed(1)}M
                        </td>
                        <td className="px-3 py-2">
                          <span
                            className={cn(
                              'px-2 py-0.5 rounded text-xs',
                              sample.features.rsi > 70
                                ? 'bg-red-500/20 text-red-400'
                                : 'bg-blue-500/20 text-blue-400'
                            )}
                          >
                            {sample.features.rsi.toFixed(1)}
                          </span>
                        </td>
                        <td className="px-3 py-2 font-semibold text-green-400">
                          ${sample.label.toFixed(2)}
                        </td>
                      </>
                    ) : (
                      <>
                        <td className="px-3 py-2 text-zinc-300">{sample.label}</td>
                        <td className="px-3 py-2 text-zinc-400">
                          {sample.features.width}×{sample.features.height}×{sample.features.channels}
                        </td>
                        <td className="px-3 py-2 font-mono text-xs text-zinc-500 max-w-xs truncate">
                          {sample.features.image_path}
                        </td>
                      </>
                    )}
                    <td className="px-3 py-2">
                      <span className="px-2 py-0.5 rounded bg-zinc-700/50 text-zinc-300 text-xs">
                        Shard {sample.shard_id}
                      </span>
                    </td>
                    <td className="px-3 py-2 text-xs text-zinc-500">{sample.worker_id || 'N/A'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>

        {/* Feature Distribution Info */}
        <div className="text-xs text-zinc-400 p-3 bg-zinc-800/30 rounded-lg">
          <strong className="text-zinc-300">💡 About this data:</strong> This shows {isStockData ? 'stock price' : 'image'} samples being processed in real-time.
          Each worker receives different shards of the dataset for distributed training.
          {isStockData
            ? ' Stock features include OHLC prices, volume, and technical indicators (MA, RSI) for next-day price prediction.'
            : ' Images are preprocessed to 224×224 pixels with normalized RGB channels for classification tasks.'}
        </div>
      </div>
    </div>
  );
}
