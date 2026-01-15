import { describe, it, expect } from 'vitest'
import { generateMockWorkers, generateMockDatasets, generateMockMetrics, generateMockData } from '../lib/mockData'

describe('generateMockWorkers', () => {
  it('generates the correct number of workers', () => {
    expect(generateMockWorkers(4)).toHaveLength(4)
    expect(generateMockWorkers(8)).toHaveLength(8)
  })
  
  it('generates workers with required fields', () => {
    const workers = generateMockWorkers(1)
    const worker = workers[0]
    
    expect(worker).toHaveProperty('id')
    expect(worker).toHaveProperty('ip')
    expect(worker).toHaveProperty('port')
    expect(worker).toHaveProperty('status')
    expect(worker).toHaveProperty('gpuCount')
    expect(worker).toHaveProperty('lastHeartbeat')
    expect(worker).toHaveProperty('assignedShards')
    expect(worker).toHaveProperty('currentEpoch')
  })
  
  it('generates valid status values', () => {
    const workers = generateMockWorkers(100)
    const validStatuses = ['active', 'idle', 'failed', 'unknown']
    
    workers.forEach(worker => {
      expect(validStatuses).toContain(worker.status)
    })
  })
})

describe('generateMockDatasets', () => {
  it('generates datasets with required fields', () => {
    const datasets = generateMockDatasets()
    
    expect(datasets.length).toBeGreaterThan(0)
    
    datasets.forEach(dataset => {
      expect(dataset).toHaveProperty('id')
      expect(dataset).toHaveProperty('name')
      expect(dataset).toHaveProperty('totalSamples')
      expect(dataset).toHaveProperty('shardSize')
      expect(dataset).toHaveProperty('shardCount')
      expect(dataset).toHaveProperty('format')
      expect(dataset).toHaveProperty('shuffle')
      expect(dataset).toHaveProperty('registeredAt')
    })
  })
  
  it('calculates shard count correctly', () => {
    const datasets = generateMockDatasets()
    
    datasets.forEach(dataset => {
      const expectedShards = Math.ceil(dataset.totalSamples / dataset.shardSize)
      expect(dataset.shardCount).toBe(expectedShards)
    })
  })
})

describe('generateMockMetrics', () => {
  it('generates metrics within expected ranges', () => {
    const metrics = generateMockMetrics()
    
    expect(metrics.checkpointThroughput).toBeGreaterThanOrEqual(480)
    expect(metrics.checkpointThroughput).toBeLessThanOrEqual(520)
    
    expect(metrics.coordinatorRps).toBeGreaterThanOrEqual(9500)
    expect(metrics.coordinatorRps).toBeLessThanOrEqual(10500)
    
    expect(metrics.barrierLatencyP99).toBeGreaterThanOrEqual(42)
    expect(metrics.barrierLatencyP99).toBeLessThanOrEqual(58)
  })
})

describe('generateMockData', () => {
  it('generates complete mock data set', () => {
    const data = generateMockData()
    
    expect(data).toHaveProperty('workers')
    expect(data).toHaveProperty('datasets')
    expect(data).toHaveProperty('checkpoints')
    expect(data).toHaveProperty('barriers')
    
    expect(Array.isArray(data.workers)).toBe(true)
    expect(Array.isArray(data.datasets)).toBe(true)
    expect(Array.isArray(data.checkpoints)).toBe(true)
    expect(Array.isArray(data.barriers)).toBe(true)
  })
})
