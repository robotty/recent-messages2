<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/state";
  import config from "../../../config";
  import {
    auth,
    randomHex,
    sanitizeReturnTo,
    updateAuthState,
  } from "#lib/auth.svelte";

  onMount(() => {
    const csrfToken = randomHex(32); // 256 bits of entropy (32 * 8 bits)
    const returnTo = sanitizeReturnTo(page.url.searchParams.get("returnTo"));

    window.sessionStorage.setItem(
      "csrfState",
      JSON.stringify({
        token: csrfToken,
        expires: Date.now() + 10 * 60 * 1000, // 10 minutes
        returnTo,
      }),
    );

    updateAuthState({ type: "loading" });

    const authorizeUrl = `https://id.twitch.tv/oauth2/authorize?client_id=${encodeURIComponent(
      config.client_id,
    )}&redirect_uri=${encodeURIComponent(
      config.redirect_uri,
    )}&response_type=code&scope=&state=${encodeURIComponent(csrfToken)}`;
    window.location.replace(authorizeUrl);

    return () => {
      if (auth.current.type === "loading") {
        updateAuthState({ type: "missing" });
      }
    };
  });
</script>

<svelte:head>
  <title>Login – recent-messages</title>
</svelte:head>

<h1>Login</h1>
<div class="alert alert-primary" role="status">
  <h4 class="alert-heading">
    <span class="spinner" aria-hidden="true"></span>
    Logging in...
  </h4>
  <p>Sending you to Twitch...</p>
</div>
