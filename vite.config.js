import { defineConfig } from 'vite'

export default defineConfig({
  server: {
    port: 5173,
    host: '127.0.0.1',
    watch: {
      ignored: [
        '**/flatpak-build/**',
        '**/.flatpak-builder/**',
        '**/flatpak-repo/**',
        '**/target/**',
        '**/src-tauri/target/**',
        '**/node_modules/**',
      ],
    },
  },
  build: {
    outDir: './dist',
    emptyOutDir: true,
  },
})
