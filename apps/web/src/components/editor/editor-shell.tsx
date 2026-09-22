import { useCallback, useState } from 'react'

import {
  ResizableHandle,
  ResizablePanel,
  ResizablePanelGroup,
} from '#/components/ui/resizable.tsx'
import { isVideoFile, probeVideo, type Clip } from '#/lib/clips.ts'
import { InspectorPanel } from './inspector-panel.tsx'
import { MediaPanel } from './media-panel.tsx'
import { PreviewPanel } from './preview-panel.tsx'
import { TimelinePanel } from './timeline-panel.tsx'

const messageOf = (reason: unknown): string =>
  reason instanceof Error ? reason.message : String(reason)

export function EditorShell() {
  const [clips, setClips] = useState<readonly Clip[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const importFiles = useCallback(async (files: Iterable<File>) => {
    const all = Array.from(files)
    const videos = all.filter(isVideoFile)
    const results = await Promise.allSettled(videos.map(probeVideo))
    const added = results.flatMap((result) =>
      result.status === 'fulfilled' ? [result.value] : [],
    )
    const skipped = all.length - videos.length
    const problems = [
      ...(skipped > 0 ? [`${skipped} file(s) skipped: only video files can be imported.`] : []),
      ...results.flatMap((result) =>
        result.status === 'rejected' ? [messageOf(result.reason)] : [],
      ),
    ]
    setError(problems.length > 0 ? problems.join(' ') : null)
    if (added.length === 0) return
    setClips((previous) => [...previous, ...added])
    setSelectedId((previous) => previous ?? added[0].id)
  }, [])

  const selected = clips.find((clip) => clip.id === selectedId) ?? null

  return (
    <div className="flex h-dvh flex-col bg-background font-sans text-xs text-foreground">
      <ResizablePanelGroup orientation="vertical">
        <ResizablePanel defaultSize="67%" minSize="40%">
          <ResizablePanelGroup orientation="horizontal">
            <ResizablePanel defaultSize="25%" minSize={160}>
              <MediaPanel
                clips={clips}
                selectedId={selectedId}
                error={error}
                onSelect={setSelectedId}
                onImport={importFiles}
              />
            </ResizablePanel>
            <ResizableHandle />
            <ResizablePanel defaultSize="50%" minSize={240}>
              <PreviewPanel clip={selected} />
            </ResizablePanel>
            <ResizableHandle />
            <ResizablePanel defaultSize="25%" minSize={160}>
              <InspectorPanel clip={selected} />
            </ResizablePanel>
          </ResizablePanelGroup>
        </ResizablePanel>
        <ResizableHandle />
        <ResizablePanel defaultSize="33%" minSize={120}>
          <TimelinePanel clips={clips} selectedId={selectedId} onSelect={setSelectedId} />
        </ResizablePanel>
      </ResizablePanelGroup>
    </div>
  )
}
