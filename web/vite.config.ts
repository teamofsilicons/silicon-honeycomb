import { defineConfig } from "vite";
import solid from "vite-plugin-solid";
export default defineConfig({
  plugins: [solid()],
  build: { target: "es2022" },
  server: {
    host: "127.0.0.1",
    hmr: { port: Number(process.env.PORT || 4173) + 20000 },
  },
});
