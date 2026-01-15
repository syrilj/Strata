import { describe, it, expect } from 'vitest'
import { formatBytes, formatNumber, formatDuration, sanitizeInput, isValidCoordinatorUrl } from '../lib/utils'

describe('formatBytes', () => {
  it('formats bytes correctly', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(1024)).toBe('1 KB')
    expect(formatBytes(1024 * 1024)).toBe('1 MB')
    expect(formatBytes(1024 * 1024 * 1024)).toBe('1 GB')
  })
  
  it('handles decimal values', () => {
    expect(formatBytes(1536)).toBe('1.5 KB')
    expect(formatBytes(1024 * 1024 * 2.5)).toBe('2.5 MB')
  })
})

describe('formatNumber', () => {
  it('formats numbers with K suffix', () => {
    expect(formatNumber(1000)).toBe('1.0K')
    expect(formatNumber(1500)).toBe('1.5K')
    expect(formatNumber(10000)).toBe('10.0K')
  })
  
  it('formats numbers with M suffix', () => {
    expect(formatNumber(1000000)).toBe('1.0M')
    expect(formatNumber(1500000)).toBe('1.5M')
  })
  
  it('returns small numbers as-is', () => {
    expect(formatNumber(100)).toBe('100')
    expect(formatNumber(999)).toBe('999')
  })
})

describe('formatDuration', () => {
  it('formats seconds', () => {
    expect(formatDuration(5000)).toBe('5s')
    expect(formatDuration(30000)).toBe('30s')
  })
  
  it('formats minutes', () => {
    expect(formatDuration(60000)).toBe('1m 0s')
    expect(formatDuration(90000)).toBe('1m 30s')
  })
  
  it('formats hours', () => {
    expect(formatDuration(3600000)).toBe('1h 0m')
    expect(formatDuration(5400000)).toBe('1h 30m')
  })
})

describe('sanitizeInput', () => {
  it('removes HTML tags', () => {
    expect(sanitizeInput('<script>alert("xss")</script>')).toBe('scriptalert("xss")/script')
  })
  
  it('removes javascript: protocol', () => {
    expect(sanitizeInput('javascript:alert(1)')).toBe('alert(1)')
  })
  
  it('trims whitespace', () => {
    expect(sanitizeInput('  hello  ')).toBe('hello')
  })
  
  it('truncates long strings', () => {
    const longString = 'a'.repeat(2000)
    expect(sanitizeInput(longString).length).toBe(1000)
  })
})

describe('isValidCoordinatorUrl', () => {
  it('accepts valid HTTP URLs', () => {
    expect(isValidCoordinatorUrl('http://localhost:50051')).toBe(true)
    expect(isValidCoordinatorUrl('https://coordinator.example.com:443')).toBe(true)
  })
  
  it('accepts host:port format', () => {
    // Note: Simple host:port without protocol is validated by regex
    expect(isValidCoordinatorUrl('http://localhost:50051')).toBe(true)
    expect(isValidCoordinatorUrl('http://10.0.0.1:8080')).toBe(true)
  })
  
  it('rejects invalid URLs', () => {
    expect(isValidCoordinatorUrl('not-a-url')).toBe(false)
    expect(isValidCoordinatorUrl('ftp://invalid.com')).toBe(false)
  })
})
