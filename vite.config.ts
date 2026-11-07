// Not used by the app build (that's electron.vite.config.ts).
// It exists only so the shadcn CLI detects Vite: `pnpm dlx shadcn@latest add <component>`.
import { resolve } from 'node:path'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import tailwindcss from '@tailwindcss/vite'

export default defineConfig({
  resolve: { alias: { '@': resolve(__dirname, 'src/renderer/src') } },
  plugins: [react(), tailwindcss()]
})
