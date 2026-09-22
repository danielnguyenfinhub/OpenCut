import { useRef, useState } from 'react'
import { HugeiconsIcon } from '@hugeicons/react'
import { Film01Icon, Upload01Icon } from '@hugeicons/core-free-icons'

import { Button } from '#/components/ui/button.tsx'
import { ScrollArea } from '#/components/ui/scroll-area.tsx'
import { formatDuration, type Clip } from '#/lib/clips.ts'
import { cn } from '#/lib/utils.ts'

type MediaPanelProps = {
  clips: readonly Clip[]
  selectedId: string | null
  error: string | null
  onSelect: (id: string) => void
  onImport: (files: Iterable<File>) => void
}

export function MediaPanel({ clips, selectedId, error, onSelect, onImport }: MediaPanelProps) {
  const inputRef = useRef<HTMLInputElement>(null)
  const [isDragging, setDragging] = useState(false)

  return (
    <section
      aria-label="Media"
      className={cn('flex h-full flex-col', isDragging && 'bg-accent/40')}
      onDragOver={(event) => {
        event.preventDefault()
        setDragging(true)
      }}
      onDragLeave={() => setDragging(false)}
      onDrop={(event) => {
        event.preventDefault()
        setDragging(false)
        onImport(event.dataTransfer.files)
      }}
    >
      <header className="flex h-8 shrink-0 items-center justify-between border-b border-border pr-2 pl-3">
        <span className="text-muted-foreground">Media</span>
        <Button variant="outline" size="sm" onClick={() => inputRef.current?.click()}>
          <HugeiconsIcon icon={Upload01Icon} size={14} />
          Import video
        </Button>
        <input
          ref={inputRef}
          type="file"
          accept="video/*"
          multiple
          className="hidden"
          onChange={(event) => {
            if (event.target.files) onImport(event.target.files)
            event.target.value = ''
          }}
        />
      </header>
      {error && (
        <p role="alert" className="border-b border-border px-3 py-2 text-destructive">
          {error}
        </p>
      )}
      {clips.length === 0 ? (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 p-6 text-center text-muted-foreground">
          <HugeiconsIcon icon={Film01Icon} size={28} strokeWidth={1.5} />
          <p>Drop video files here, or import from your computer.</p>
        </div>
      ) : (
        <ScrollArea className="min-h-0 flex-1">
          <ul className="py-1">
            {clips.map((clip) => {
              const isSelected = clip.id === selectedId
              return (
                <li key={clip.id}>
                  <button
                    type="button"
                    aria-pressed={isSelected}
                    onClick={() => onSelect(clip.id)}
                    className={cn(
                      'flex w-full items-center gap-2 border-l-2 py-1.5 pr-3 pl-2.5 text-left',
                      isSelected
                        ? 'border-primary bg-accent text-accent-foreground'
                        : 'border-transparent hover:bg-muted',
                    )}
                  >
                    <span className="shrink-0 text-muted-foreground">
                      <HugeiconsIcon icon={Film01Icon} size={14} />
                    </span>
                    <span className="min-w-0 flex-1 truncate text-[13px]">{clip.name}</span>
                    <span className="text-muted-foreground tabular-nums">
                      {formatDuration(clip.duration)}
                    </span>
                  </button>
                </li>
              )
            })}
          </ul>
        </ScrollArea>
      )}
    </section>
  )
}
