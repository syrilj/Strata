import { beforeEach, describe, expect, it, vi } from 'vitest'
import { CoordinatorApi, DEFAULT_BASE_URL, STORAGE_KEY } from '../lib/api'
import { useDashboardStore } from '../store'

describe('CoordinatorApi', () => {
  beforeEach(() => {
    window.localStorage.clear()
    vi.restoreAllMocks()
  })

  it('persists and resets baseUrl in localStorage', () => {
    const client = new CoordinatorApi()
    expect(client.getBaseUrl()).toBe(DEFAULT_BASE_URL)

    client.setBaseUrl('http://localhost:51051/api/')
    expect(client.getBaseUrl()).toBe('http://localhost:51051/api')
    expect(window.localStorage.getItem(STORAGE_KEY)).toBe('http://localhost:51051/api')

    const reloaded = new CoordinatorApi()
    expect(reloaded.getBaseUrl()).toBe('http://localhost:51051/api')

    reloaded.resetBaseUrl()
    expect(reloaded.getBaseUrl()).toBe(DEFAULT_BASE_URL)
    expect(window.localStorage.getItem(STORAGE_KEY)).toBeNull()
  })

  it('maps both snake_case and camelCase dashboard payloads', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 200,
      text: async () =>
        JSON.stringify({
          coordinator: {
            connected: true,
            address: 'localhost:50051',
            http_address: '0.0.0.0:51051',
            storage_backend: 's3',
            demo_mode: false,
            uptime: 120,
            version: '0.2.0',
          },
          workers: [
            {
              id: 'w-1',
              ip: '10.0.0.1',
              port: 50052,
              status: 'training',
              gpu_count: 4,
              last_heartbeat: 1700000000000,
              assigned_shards: 8,
              current_epoch: 2,
              current_step: 45,
              current_task: 'train_step',
              resources: {
                cpu_percent: 64.5,
                memory_used_bytes: 1024,
                disk_read_bytes: 200,
                disk_write_bytes: 100,
                network_rx_bytes: 50,
                network_tx_bytes: 25,
                gpu_usage: [
                  {
                    gpu_id: 0,
                    utilization_percent: 91,
                    memory_used_bytes: 8000,
                    memory_total_bytes: 16000,
                    temperature_celsius: 68,
                  },
                ],
              },
            },
          ],
          datasets: [
            {
              id: 'ds-1',
              name: 'train-data',
              totalSamples: 1000,
              shardSize: 250,
              shardCount: 4,
              format: 'parquet',
              shuffle: true,
              registeredAt: 1700000000000,
            },
          ],
          checkpoints: [
            {
              id: 'ckpt-1',
              step: 500,
              epoch: 1,
              size: 1048576,
              path: '/ckpts/step_500.pt',
              created_at: 1700000000000,
              worker_id: 'w-1',
              status: 'completed',
            },
          ],
          barriers: [],
          metrics: {
            checkpoint_throughput: 125,
            coordinator_rps: 420,
            active_workers: 1,
            total_workers: 1,
            barrier_latency_p99: 12,
            shard_assignment_time: 4,
          },
          tasks: [
            {
              id: 'task-1',
              name: 'job-1',
              type: 'custom',
              status: 'stopped',
              worker_ids: ['w-1'],
              dataset_id: 'ds-1',
              started_at: 1700000000000,
              progress: 50,
              logs: ['started'],
            },
          ],
          logs: [],
        }),
    })
    vi.stubGlobal('fetch', fetchMock)

    const client = new CoordinatorApi('/api')
    const state = await client.getDashboard()

    expect(state.coordinator.storageBackend).toBe('s3')
    expect(state.coordinator.demoMode).toBe(false)
    expect(state.coordinator.httpAddress).toBe('0.0.0.0:51051')
    expect(state.workers[0].status).toBe('training')
    expect(state.workers[0].resources?.gpuUsage[0].utilizationPercent).toBe(91)
    expect(state.datasets[0].totalSamples).toBe(1000)
    expect(state.checkpoints[0].size).toBe(1048576)
    expect(state.metrics.coordinatorRps).toBe(420)
    expect(state.tasks[0].status).toBe('stopped')
  })

  it('calls dataset, worker, and checkpoint endpoints with expected methods', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: true,
      status: 204,
      text: async () => '',
    })
    vi.stubGlobal('fetch', fetchMock)

    const client = new CoordinatorApi('/api')
    await client.deregisterWorker('worker 1')
    await client.deleteDataset('ds/1')
    await client.deleteCheckpoint('ckpt-1')

    expect(fetchMock).toHaveBeenNthCalledWith(1, '/api/workers/worker%201', { method: 'DELETE' })
    expect(fetchMock).toHaveBeenNthCalledWith(2, '/api/datasets/ds%2F1', { method: 'DELETE' })
    expect(fetchMock).toHaveBeenNthCalledWith(3, '/api/checkpoints/ckpt-1', { method: 'DELETE' })
  })
})

describe('useDashboardStore', () => {
  beforeEach(() => {
    vi.restoreAllMocks()
    useDashboardStore.setState({
      workers: [],
      datasets: [],
      checkpoints: [],
      barriers: [],
      logs: [],
      tasks: [],
      throughputHistory: [],
      logsClearedAt: null,
      logsAutoRefresh: true,
      selectedDatasetId: null,
      lastError: null,
      lastUpdated: null,
      retryCount: 0,
    })
  })

  it('preserves clearLogs cutoff across subsequent fetchLiveData polls', async () => {
    const now = Date.now()
    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.endsWith('/metrics/history')) {
        return {
          ok: true,
          status: 200,
          text: async () => JSON.stringify([]),
        }
      }
      return {
        ok: true,
        status: 200,
        text: async () =>
          JSON.stringify({
            coordinator: { connected: true, address: 'localhost:50051', uptime: 10, version: '0.1.0' },
            workers: [],
            datasets: [{ id: 'ds-alpha', name: 'Alpha', total_samples: 10, shard_size: 5, shard_count: 2, format: 'jsonl', shuffle: false, registered_at: now }],
            checkpoints: [],
            barriers: [],
            metrics: { checkpoint_throughput: 10, coordinator_rps: 20, active_workers: 0, total_workers: 0, barrier_latency_p99: 1, shard_assignment_time: 1 },
            tasks: [],
            logs: [
              { id: 'old-log', timestamp: now - 60_000, level: 'info', message: 'old', source: 'coordinator' },
              { id: 'new-log', timestamp: now + 60_000, level: 'info', message: 'new', source: 'coordinator' },
            ],
          }),
      }
    })
    vi.stubGlobal('fetch', fetchMock)

    useDashboardStore.getState().clearLogs()
    await useDashboardStore.getState().fetchLiveData()

    const state = useDashboardStore.getState()
    expect(state.logs.map((l) => l.id)).toEqual(['new-log'])
    expect(state.throughputHistory).toHaveLength(1)
    expect(state.selectedDatasetId).toBe('ds-alpha')
  })

  it('does not overwrite logs during polling when logsAutoRefresh is false', async () => {
    const existing = [
      { id: 'kept', timestamp: Date.now(), level: 'info' as const, message: 'kept log', source: 'ui' },
    ]
    useDashboardStore.setState({ logs: existing, logsAutoRefresh: false })

    const fetchMock = vi.fn(async (input: RequestInfo | URL) => {
      const url = String(input)
      if (url.endsWith('/metrics/history')) {
        return { ok: true, status: 200, text: async () => JSON.stringify([]) }
      }
      return {
        ok: true,
        status: 200,
        text: async () =>
          JSON.stringify({
            coordinator: { connected: true, address: 'localhost:50051', uptime: 10, version: '0.1.0' },
            workers: [],
            datasets: [],
            checkpoints: [],
            barriers: [],
            metrics: { checkpoint_throughput: 0, coordinator_rps: 0, active_workers: 0, total_workers: 0, barrier_latency_p99: 0, shard_assignment_time: 0 },
            tasks: [],
            logs: [{ id: 'incoming', timestamp: Date.now() + 1000, level: 'warn', message: 'incoming', source: 'coordinator' }],
          }),
      }
    })
    vi.stubGlobal('fetch', fetchMock)

    await useDashboardStore.getState().fetchLiveData()
    expect(useDashboardStore.getState().logs).toEqual(existing)
  })
})
