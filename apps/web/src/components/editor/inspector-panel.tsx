import { formatBytes, formatDuration, type Clip } from '#/lib/clips.ts'

export function InspectorPanel({ clip }: { clip: Clip | null }) {
  return (
    <section aria-label="Inspector" className="flex h-full flex-col">
      <header className="flex h-8 shrink-0 items-center border-b border-border px-3 text-muted-foreground">
        Inspector
      </header>
      {clip ? (
        <dl className="grid grid-cols-[max-content_minmax(0,1fr)] gap-x-4 gap-y-2 p-3">
          <dt className="text-muted-foreground">Name</dt>
          <dd className="truncate" title={clip.name}>
            {clip.name}
          </dd>
          <dt className="text-muted-foreground">Duration</dt>
          <dd className="tabular-nums">{formatDuration(clip.duration)}</dd>
          <dt className="text-muted-foreground">Dimensions</dt>
          <dd className="tabular-nums">
            {clip.width} × {clip.height}
          </dd>
          <dt className="text-muted-foreground">File size</dt>
          <dd className="tabular-nums">{formatBytes(clip.size)}</dd>
          <dt className="text-muted-foreground">Type</dt>
          <dd className="truncate">{clip.type}</dd>
        </dl>
      ) : (
        <p className="p-3 text-muted-foreground">
          Nothing selected. Pick a clip in Media to see its details.
        </p>
      )}
    </section>
  )
}
