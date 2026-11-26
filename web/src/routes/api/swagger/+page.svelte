<script lang="ts">
  import { onMount } from "svelte";
  import SwaggerUIBundle from "swagger-ui-dist/swagger-ui-es-bundle.js";
  import "swagger-ui-dist/swagger-ui.css";
  import { getOpenApiSpec } from "#lib/openapi";

  let container = $state<HTMLDivElement | null>(null);

  onMount(() => {
    if (!container) return;

    SwaggerUIBundle({
      domNode: container,
      spec: getOpenApiSpec(),
      deepLinking: true,
      docExpansion: "list",
      defaultModelsExpandDepth: 1,
    });

    return () => {
      container?.replaceChildren();
    };
  });
</script>

<svelte:head>
  <title>API Reference (Swagger UI) – recent-messages</title>
</svelte:head>

<div class="swagger-page">
  <div class="swagger-surface">
    <div bind:this={container}></div>
  </div>
</div>

<style>
  .swagger-page {
    max-width: 76rem;
    margin: 1.5rem auto 3rem;
    padding: 0 1.25rem;
  }

  .swagger-surface {
    background: #ffffff;
    color: #3b4151;
    color-scheme: light;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.75rem 0 1.5rem;
  }
</style>
