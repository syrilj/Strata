import type { Worker, Dataset, Checkpoint, SystemMetrics, LogEntry, BarrierStatus } from '../types'

const WORKER_NAMES = ['gpu-node-0', 'gpu-node-1', 'gpu-node-2', 'gpu-node-3', 'gpu-node-4', 'gpu-node-5', 'gpu-node-6', 'gpu-node-7']

export function generateMockWorkers(count: number = 8): Worker[] {
  return Array.from({ length: count }, (_, i) => ({
    id: WORKER_NAMES[i] || `worker-${i}`,
    ip: `10.0.${Math.floor(i / 256)}.${i % 256}`,
    port: 50052 + i,
    status: Math.random() > 0.1 ? 'active' as const : 'idle' as const,
    gpuCount: 8,
    lastHeartbeat: Date.now() - Math.floor(Math.random() * 5000),
    assignedShards: Math.floor(Math.random() * 50) + 100,
    currentEpoch: Math.floor(Math.random() * 5),
    currentStep: Math.floor(Math.random() * 1000),
    currentTask: 'training',
  }))
}

export function generateMockDatasets(): Dataset[] {
  return [
    {
      id: 'ds-imagenet',
      name: 'ImageNet-1K',
      totalSamples: 1281167,
      shardSize: 10000,
      shardCount: 129,
      format: 'tfrecord',
      shuffle: true,
      registeredAt: Date.now() - 3600000,
    },
    {
      id: 'ds-coco',
      name: 'COCO-2017',
      totalSamples: 118287,
      shardSize: 5000,
      shardCount: 24,
      format: 'parquet',
      shuffle: true,
      registeredAt: Date.now() - 1800000,
    },
    {
      id: 'ds-openwebtext',
      name: 'OpenWebText',
      totalSamples: 8013769,
      shardSize: 50000,
      shardCount: 161,
      format: 'jsonl',
      shuffle: true,
      registeredAt: Date.now() - 900000,
    },
  ]
}

export function generateMockCheckpoints(): Checkpoint[] {
  return Array.from({ length: 10 }, (_, i) => ({
    id: `ckpt-${i}`,
    step: (10 - i) * 1000,
    epoch: Math.floor((10 - i) / 2),
    size: Math.floor(Math.random() * 400) + 100,
    path: `/checkpoints/step_${(10 - i) * 1000}.bin`,
    createdAt: Date.now() - i * 300000,
    workerId: WORKER_NAMES[i % WORKER_NAMES.length],
    status: 'completed',
  }))
}

export function generateMockBarriers(): BarrierStatus[] {
  return [
    { id: 'barrier-epoch', name: 'Epoch Sync', arrived: 8, total: 8, status: 'complete', createdAt: Date.now() - 5000 },
    { id: 'barrier-ckpt', name: 'Checkpoint Barrier', arrived: 6, total: 8, status: 'waiting', createdAt: Date.now() - 1000 },
  ]
}

export function generateMockMetrics(): SystemMetrics {
  return {
    checkpointThroughput: 480 + Math.floor(Math.random() * 40),
    coordinatorRps: 9500 + Math.floor(Math.random() * 1000),
    activeWorkers: 7 + Math.floor(Math.random() * 2),
    totalWorkers: 8,
    barrierLatencyP99: 42 + Math.floor(Math.random() * 16),
    shardAssignmentTime: 6 + Math.floor(Math.random() * 4),
  }
}

export function generateMockData() {
  return {
    workers: generateMockWorkers(),
    datasets: generateMockDatasets(),
    checkpoints: generateMockCheckpoints(),
    barriers: generateMockBarriers(),
  }
}

const LOG_MESSAGES = [
  { level: 'info' as const, message: 'Heartbeat received from worker', source: 'coordinator' },
  { level: 'info' as const, message: 'Shard assignment completed', source: 'data-shard' },
  { level: 'info' as const, message: 'Checkpoint write started', source: 'checkpoint' },
  { level: 'info' as const, message: 'Barrier sync completed', source: 'coordinator' },
  { level: 'info' as const, message: 'gRPC connection established', source: 'coordinator' },
  { level: 'warn' as const, message: 'Worker heartbeat delayed', source: 'coordinator' },
  { level: 'info' as const, message: 'Epoch advanced', source: 'data-shard' },
]

export function generateMockLog(): Omit<LogEntry, 'id' | 'timestamp'> {
  return LOG_MESSAGES[Math.floor(Math.random() * LOG_MESSAGES.length)]
}
