<script lang="ts">
  import { onMount } from "svelte";

  let { rot13: encoded, linked = true }: { rot13: string; linked?: boolean } =
    $props();

  function decodeRot13(s: string): string {
    return s.replace(/[A-Za-z]/g, (c) => {
      const base = c <= "Z" ? 65 : 97;
      return String.fromCharCode(((c.charCodeAt(0) - base + 13) % 26) + base);
    });
  }

  let decoded = $state<string | null>(null);

  onMount(() => {
    decoded = decodeRot13(encoded);
  });
</script>

{#if decoded === null}
  <span class="email-placeholder">[enable JavaScript to view email]</span>
{:else if linked}
  <a href={`mailto:${decoded}`}>{decoded}</a>
{:else}
  {decoded}
{/if}
