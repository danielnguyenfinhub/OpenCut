import { createFileRoute } from '@tanstack/react-router'

import { EditorShell } from '#/components/editor/editor-shell.tsx'

export const Route = createFileRoute('/editor')({
  component: EditorShell,
  head: () => ({ meta: [{ title: 'Editor | OpenCut' }] }),
})
