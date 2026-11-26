<script lang="ts">
  import { page } from "$app/state";
</script>

<svelte:head>
  <title>
    {page.status === 404 ? "Not Found" : `Error ${page.status}`} – recent-messages
  </title>
</svelte:head>

{#if page.status === 404}
  <h1>Not Found</h1>
  <p>The page you were trying to access does not exist.</p>
{:else}
  <h1>Error {page.status}</h1>
  <p>{page.error?.message ?? "An unexpected error occurred."}</p>
{/if}
<p><a href="/">Return to the home page</a></p>
