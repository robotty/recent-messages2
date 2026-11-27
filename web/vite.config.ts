import adapter from "@sveltejs/adapter-static";
import { sveltekit } from "@sveltejs/kit/vite";
import { defineConfig } from "vite";

export default defineConfig({
  plugins: [
    sveltekit({
      adapter: adapter({
        pages: "dist",
        assets: "dist",
        fallback: "200.html",
        precompress: false,
        strict: true,
      }),
    }),
  ],
});
