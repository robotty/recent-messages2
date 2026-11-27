<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import "../app.css";
  import { initAuth } from "#lib/auth.svelte";
  import Nav from "#lib/Nav.svelte";

  let { children } = $props();

  onMount(() => {
    return initAuth();
  });

  const isFullBleed = $derived(page.url.pathname === "/api/swagger");
</script>

<Nav />
{#if isFullBleed}
  <div class="full-bleed">
    {@render children()}
  </div>
{:else}
  <main class="container">
    {@render children()}
  </main>
{/if}
