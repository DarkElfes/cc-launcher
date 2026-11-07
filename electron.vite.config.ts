import { resolve } from 'node:path'
import { defineConfig } from 'electron-vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  main: {
    define: {
      // Read-only GitHub token for downloading updates, taken from the environment of the shell that
      // runs the build (see docs/RELEASING.md). It is never written to a source file or committed.
      // Empty in normal dev builds.
      __BUNDLED_UPDATE_TOKEN__: JSON.stringify(process.env.CCL_UPDATE_TOKEN?.trim() ?? '')
    }
  },
  preload: {},
  renderer: {
    resolve: {
      alias: { '@': resolve(__dirname, 'src/renderer/src') }
    },
    plugins: [react(), tailwindcss()]
  }
})
