# recent-messages2 web

Houses the static front-end web app built with Svelte 5 and SvelteKit (`@sveltejs/adapter-static`).

Edit `config.ts`, then install dependencies and run:

```bash
npm ci
npm run dev
```

To type-check and create a production build in `dist`:

```bash
npm run check
npm run build
```

Static documentation routes (`/`, `/api`, `/privacy`, `/donation-thank-you`) are prerendered to HTML, and dynamic routes (`/login`, `/authorized`, `/logout`, `/settings`) fall back to `200.html`.

A sample nginx config file is available as `nginx-example.conf`.
