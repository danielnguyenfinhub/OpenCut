export type Clip = {
  id: string
  name: string
  /** Object URL for the imported file. Lives until the page reloads. */
  url: string
  /** Seconds. May be Infinity or NaN when the container does not declare a length. */
  duration: number
  width: number
  height: number
  /** Bytes. */
  size: number
  /** MIME type as reported by the browser. */
  type: string
}

export type TimelineBlock = {
  id: string
  /** Fraction of the total duration at which the block starts, 0..1. */
  start: number
  /** Fraction of the total duration the block covers, 0..1. */
  width: number
}

export type RulerTick = {
  seconds: number
  /** Position along the ruler, 0..1. */
  fraction: number
}

export function isVideoFile(file: File): boolean {
  return file.type.startsWith('video/')
}

/** Lets the browser parse the file's metadata to learn its duration and dimensions. */
export function probeVideo(file: File): Promise<Clip> {
  // ponytail: object URLs are never revoked; revoke on remove once clips can be removed.
  const url = URL.createObjectURL(file)
  return new Promise((resolve, reject) => {
    const video = document.createElement('video')
    video.preload = 'metadata'
    video.onloadedmetadata = () => {
      resolve({
        id: crypto.randomUUID(),
        name: file.name,
        url,
        duration: video.duration,
        width: video.videoWidth,
        height: video.videoHeight,
        size: file.size,
        type: file.type,
      })
    }
    video.onerror = () => {
      URL.revokeObjectURL(url)
      reject(new Error(`${file.name} could not be read as a video.`))
    }
    video.src = url
  })
}

const knownSeconds = (seconds: number): number =>
  Number.isFinite(seconds) && seconds > 0 ? seconds : 0

/** m:ss.t under an hour, h:mm:ss from an hour up, --:-- when the length is unknown. */
export function formatDuration(seconds: number): string {
  if (!Number.isFinite(seconds) || seconds < 0) return '--:--'
  const whole = Math.floor(seconds)
  const hours = Math.floor(whole / 3600)
  const minutes = Math.floor((whole % 3600) / 60)
  const secs = String(whole % 60).padStart(2, '0')
  if (hours > 0) return `${hours}:${String(minutes).padStart(2, '0')}:${secs}`
  const tenths = Math.floor((seconds - whole) * 10)
  return `${minutes}:${secs}.${tenths}`
}

const BYTE_UNITS = ['KB', 'MB', 'GB'] as const

export function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 0) return '--'
  if (bytes < 1024) return `${bytes} B`
  const exponent = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), BYTE_UNITS.length)
  const value = bytes / 1024 ** exponent
  return `${value < 10 ? value.toFixed(1) : Math.round(value)} ${BYTE_UNITS[exponent - 1]}`
}

export function totalDuration(clips: readonly Clip[]): number {
  return clips.reduce((sum, clip) => sum + knownSeconds(clip.duration), 0)
}

/** Lays clips end to end. A clip with an unknown length keeps its slot but gets no width. */
export function layoutTimeline(clips: readonly Clip[]): TimelineBlock[] {
  const durations = clips.map((clip) => knownSeconds(clip.duration))
  const total = durations.reduce((sum, seconds) => sum + seconds, 0)
  if (total === 0) return []
  const starts = durations.reduce<number[]>(
    (acc, _seconds, index) => [...acc, index === 0 ? 0 : acc[index - 1] + durations[index - 1]],
    [],
  )
  return clips.map((clip, index) => ({
    id: clip.id,
    start: starts[index] / total,
    width: durations[index] / total,
  }))
}

const TICK_STEPS = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600] as const

/** Picks the smallest whole-second step that keeps the tick count within budget. */
export function rulerTicks(totalSeconds: number, maxTicks = 10): RulerTick[] {
  if (!Number.isFinite(totalSeconds) || totalSeconds <= 0) return []
  const step =
    TICK_STEPS.find((candidate) => totalSeconds / candidate <= maxTicks) ??
    TICK_STEPS[TICK_STEPS.length - 1]
  const count = Math.floor(totalSeconds / step) + 1
  return Array.from({ length: count }, (_, index) => ({
    seconds: index * step,
    fraction: (index * step) / totalSeconds,
  }))
}
