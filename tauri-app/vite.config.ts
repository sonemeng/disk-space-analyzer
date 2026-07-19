import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

export default defineConfig({
  // Tauri production loads assets via custom protocol; absolute `/assets/...` causes blank white window
  base: './',
  plugins: [vue()],
  clearScreen: false,
  resolve: {
    alias: {
      '@': '/src',
    },
  },
  server: {
    port: 5173,
    strictPort: true,
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: 'esnext',
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
})
