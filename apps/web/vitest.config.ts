import { defineConfig } from 'vitest/config'

// Deliberately plugin-free. vite.config.ts loads @cloudflare/vite-plugin, which rejects the
// resolve.external that vitest sets on the ssr environment and aborts before any test runs.
// Unit tests here cover plain TypeScript, so they need none of the app plugins.
export default defineConfig({
  test: {
    include: ['src/**/*.test.ts'],
  },
})
