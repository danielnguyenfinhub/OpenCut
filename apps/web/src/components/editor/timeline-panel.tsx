import {
  formatDuration,
  layoutTimeline,
  rulerTicks,
  totalDuration,
  type Clip,
} from '#/lib/clips.ts'
import { cn } from '#/lib/utils.ts'

type TimelinePanelProps = {
  clips: readonly Clip[]
  selectedId: string | null
  onSelect: (id: string) => void
}

export function TimelinePanel({ clips, selectedId, onSelect }: TimelinePanelProps) {
  const total = totalDuration(clips)
  const blocks = layoutTimeline(clips)
  const ticks = rulerTicks(total)

  return (
    <section aria-label="Timeline" className="flex h-full flex-col">
      <header className="flex h-8 shrink-0 items-center justify-between border-b border-border px-3 text-muted-foreground">
        <span>Timeline</span>
        <span className="tabular-nums">{formatDuration(total)}</span>
      </header>
      {blocks.length === 0 ? (
        <p className="p-3 text-muted-foreground">Imported clips appear here in order.</p>
      ) : (
        <div className="flex flex-col gap-1 p-3">
          <div className="relative h-4 text-[0.625rem] text-muted-foreground tabular-nums">
            {ticks.map((tick) => (
              <span
                key={tick.seconds}
                className="absolute top-0 -translate-x-1/2 first:translate-x-0 last:-translate-x-full"
                style={{ left: `${tick.fraction * 100}%` }}
              >
                {tick.seconds}s
              </span>
            ))}
          </div>
          <div className="relative h-12">
            {blocks.map((block, index) => {
              const clip = clips[index]
              const isSelected = block.id === selectedId
              return (
                <button
                  type="button"
                  key={block.id}
                  aria-pressed={isSelected}
                  title={clip.name}
                  onClick={() => onSelect(block.id)}
                  className={cn(
                    'absolute inset-y-0 overflow-hidden rounded-sm border px-2 text-left',
                    isSelected
                      ? 'border-primary bg-accent text-accent-foreground'
                      : 'border-border bg-muted hover:bg-accent',
                  )}
                  style={{
                    left: `${block.start * 100}%`,
                    width: `max(4px, calc(${block.width * 100}% - 2px))`,
                  }}
                >
                  <span className="block truncate text-[13px]">{clip.name}</span>
                </button>
              )
            })}
          </div>
        </div>
      )}
    </section>
  )
}
