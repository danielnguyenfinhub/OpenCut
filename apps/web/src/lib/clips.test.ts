import { describe, expect, it } from 'vitest'

import {
  formatBytes,
  formatDuration,
  layoutTimeline,
  rulerTicks,
  totalDuration,
  type Clip,
} from './clips.ts'

const clip = (id: string, duration: number): Clip => ({
  id,
  name: `${id}.mp4`,
  url: `blob:${id}`,
  duration,
  width: 1920,
  height: 1080,
  size: 0,
  type: 'video/mp4',
})

describe('formatDuration', () => {
  it('shows minutes, seconds and tenths under an hour', () => {
    expect(formatDuration(65.34)).toBe('1:05.3')
    expect(formatDuration(0)).toBe('0:00.0')
  })

  it('switches to h:mm:ss from an hour up', () => {
    expect(formatDuration(3725)).toBe('1:02:05')
  })

  it('marks an unknown length instead of showing zero', () => {
    expect(formatDuration(Number.POSITIVE_INFINITY)).toBe('--:--')
    expect(formatDuration(Number.NaN)).toBe('--:--')
  })
})

describe('layoutTimeline', () => {
  it('sizes blocks by their share of the total duration', () => {
    expect(layoutTimeline([clip('a', 2), clip('b', 6)])).toEqual([
      { id: 'a', start: 0, width: 0.25 },
      { id: 'b', start: 0.25, width: 0.75 },
    ])
  })

  it('keeps a slot for an unknown-length clip but gives it no width', () => {
    expect(layoutTimeline([clip('a', Number.POSITIVE_INFINITY), clip('b', 4)])).toEqual([
      { id: 'a', start: 0, width: 0 },
      { id: 'b', start: 0, width: 1 },
    ])
  })

  it('returns nothing when there is nothing to show', () => {
    expect(layoutTimeline([])).toEqual([])
    expect(layoutTimeline([clip('a', Number.NaN)])).toEqual([])
  })
})

describe('totalDuration', () => {
  it('ignores clips whose length is unknown', () => {
    expect(totalDuration([clip('a', 2.5), clip('b', Number.POSITIVE_INFINITY)])).toBe(2.5)
  })
})

describe('rulerTicks', () => {
  it('picks the smallest step that fits the tick budget', () => {
    expect(rulerTicks(7).map((tick) => tick.seconds)).toEqual([0, 1, 2, 3, 4, 5, 6, 7])
    expect(rulerTicks(45).map((tick) => tick.seconds)).toEqual([
      0, 5, 10, 15, 20, 25, 30, 35, 40, 45,
    ])
  })

  it('places ticks as fractions of the total', () => {
    expect(rulerTicks(4).map((tick) => tick.fraction)).toEqual([0, 0.25, 0.5, 0.75, 1])
  })
})

describe('formatBytes', () => {
  it('rounds to a readable unit', () => {
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(3_355_443)).toBe('3.2 MB')
    expect(formatBytes(52_428_800)).toBe('50 MB')
  })
})
