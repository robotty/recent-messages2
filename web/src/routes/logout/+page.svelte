<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import {
    auth,
    loadStoredAuth,
    revokeAuth,
    sanitizeReturnTo,
    updateAuthState,
  } from "#lib/auth.svelte";

  onMount(() => {
    loadStoredAuth();
    const current = auth.current;
    if (current.type === "present") {
      void revokeAuth(current.accessToken)
        .then(() => {
          console.log("Successfully finished revoking token");
        })
        .catch((err: unknown) => {
          console.error("Failed to revoke login token:", err);
        });
    }

    updateAuthState({ type: "missing" });

    const returnTo = sanitizeReturnTo(page.url.searchParams.get("returnTo"));
    void goto(returnTo, { replaceState: true });
  });
</script>

<svelte:head>
  <title>Logging out – recent-messages</title>
</svelte:head>

<h1>Log Out</h1>
<p>Logging you out...</p>
