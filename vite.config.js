import { defineConfig } from "vite";
import { resolve } from "path";

export default defineConfig({
  // Root directory where index.html and source files reside
  root: resolve(__dirname, "static"),
  
  // Base public path set to relative path './' for portable zero-config nested deployment
  base: "./",
  
  server: {
    fs: {
      // Allow serving assets from the parent directory (such as pkg/ for fce_core.js)
      allow: [".."]
    }
  },
  
  build: {
    // Output directory relative to the config file
    outDir: resolve(__dirname, "dist"),
    emptyOutDir: true,
    
    // Target modern Javascript runtime environments supporting modern WASM imports and ES Modules
    target: "esnext"
  }
});
