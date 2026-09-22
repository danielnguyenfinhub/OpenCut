import { formatDuration, type Clip } from '#/lib/clips.ts'

export function PreviewPanel({ clip }: { clip: Clip | null }) {
  return (
    <section aria-label="Preview" className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1 items-center justify-center bg-black p-4">
        {clip ? (
          // biome-ignore lint/a11y/useMediaCaption: footage the user just imported has no caption track
          <video key={clip.id} src={clip.url} controls playsInline className="max-h-full max-w-full" />
        ) : (
          <p className="text-muted-foreground">Nothing to preview yet. Import a video to see it here.</p>
        )}
      </div>
      <footer className="flex h-8 shrink-0 items-center justify-between border-t border-border px-3 text-muted-foreground">
        <span className="min-w-0 truncate">{clip?.name ?? 'Preview'}</span>
        <span className="tabular-nums">{clip ? formatDuration(clip.duration) : '--:--'}</span>
      </footer>
    </section>
  )
}
