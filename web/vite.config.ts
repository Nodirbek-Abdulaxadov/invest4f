import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// Dev: /api so'rovlari Rust serverga (127.0.0.1:8787) proksi qilinadi.
export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: { '/api': 'http://127.0.0.1:8787' },
  },
})
