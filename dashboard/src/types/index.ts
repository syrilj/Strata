// Core types for the distributed training runtime dashboard

export interface Worker {
  id: string
  ip: string
  port: number
  status: 'active' | 'idle' | 'failed' | 'unknown'
  gpuCount: number
  lastHeartbeat: number
  assignedShards: number
  currentEpoch: number
  currentStep: number
  currentTask: string
}

export interface Dataset {
  id: string
  name: string
  totalSamples: number
  shardSize: number
  shardCount: number
  format: string
  shuffle: boolean
  registeredAt: number
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
}
