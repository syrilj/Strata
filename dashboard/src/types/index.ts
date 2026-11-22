// Core types for the distributed training runtime dashboard

export interface GpuUsage {
  gpuId: number
  utilizationPercent: number
  memoryUsedBytes: number
  memoryTotalBytes: number
  temperatureCelsius: number
}

export interface WorkerResources {
  cpuPercent: number
  memoryUsedBytes: number
  diskReadBytes: number
  diskWriteBytes: number
  networkRxBytes: number
  networkTxBytes: number
  gpuUsage: GpuUsage[]
}

export type WorkerStatus =
  | 'active'
  | 'idle'
  | 'failed'
  | 'unknown'
  | 'training'
  | 'loading_data'
  | 'checkpointing'
  | 'recovering'

export interface Worker {
  id: string
  ip: string
  port: number
  status: WorkerStatus
  gpuCount: number
  lastHeartbeat: number
  assignedShards: number
  currentEpoch: number
  currentStep: number
  currentTask: string
  resources?: WorkerResources
}

export interface Dataset {
  id: string
  name: string
  path?: string
  totalSamples: number
  shardSize: number
  shardCount: number
  format: string
  shuffle: boolean
  registeredAt: number
}

export interface RegisterDatasetRequest {
  id?: string
  name: string
  path: string
  format: string
  total_samples?: number
  shard_size: number
  shuffle?: boolean
  seed?: number
}

export interface Checkpoint {
  id: string
  step: number
  epoch: number
  size: number
  path: string
  createdAt: number
  workerId: string
  status: 'completed' | 'in_progress' | 'failed'
}

export interface SystemMetrics {
  checkpointThroughput: number
  coordinatorRps: number
  activeWorkers: number
  totalWorkers: number
  barrierLatencyP99: number
  shardAssignmentTime: number
}

export interface BarrierStatus {
  id: string
  name: string
  arrived: number
  total: number
  status: 'waiting' | 'complete'
  createdAt: number
}

export interface LogEntry {
  id: string
  timestamp: number
  level: 'info' | 'warn' | 'error' | 'debug'
  message: string
  source: string
  taskId?: string
  workerId?: string
}

export interface CoordinatorStatus {
  connected: boolean
  address: string
  uptime: number
  version: string
  httpAddress?: string
  storageBackend?: string
  demoMode?: boolean
}

export interface Task {
  id: string
  name: string
  type: string
  status: 'running' | 'completed' | 'failed' | 'pending' | 'stopped'
  workerIds: string[]
  datasetId: string
  startedAt: number
  completedAt?: number
  progress: number
  logs: string[]
}

export interface CreateTaskRequest {
  name: string
  type: string
  dataset_id: string
  worker_count: number
  config: {
    epochs?: number
    batch_size?: number
    learning_rate?: number
    [key: string]: unknown
  }
}

export interface DataSample {
  id: string
  features: Record<string, unknown>
  label?: unknown
  shardId: number
  workerId?: string
}

export interface DatasetPreviewResponse {
  datasetId: string
  columns: string[]
  samples: DataSample[]
  totalSamples: number
  format: string
  note?: string
}

export interface DatasetShardInfo {
  shardId: number
  workerId?: string
  sampleCount: number
  status?: string
}

export interface DatasetShardsResponse {
  datasetId: string
  totalShards: number
  shardSize: number
  shards: DatasetShardInfo[]
}

export interface ThroughputDataPoint {
  timestamp: number
  checkpoints: number
  requests: number
}

export interface DashboardStatePayload {
  coordinator: CoordinatorStatus
  workers: Worker[]
  datasets: Dataset[]
  checkpoints: Checkpoint[]
  barriers: BarrierStatus[]
  metrics: SystemMetrics
  tasks: Task[]
  logs: LogEntry[]
}
