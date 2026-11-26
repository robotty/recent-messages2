<script lang="ts">
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/state";
  import {
    createAuth,
    sanitizeReturnTo,
    updateAuthState,
  } from "#lib/auth.svelte";

  type AuthorizedViewState =
    | { type: "loadToken" }
    | { type: "error"; message: string; returnTo: string };

  let viewState = $state<AuthorizedViewState>({ type: "loadToken" });

  onMount(() => {
    const ownCsrfStateRaw = window.sessionStorage.getItem("csrfState");
    window.sessionStorage.removeItem("csrfState");

    if (ownCsrfStateRaw == null) {
      viewState = {
        type: "error",
        message: "No CSRF token found in browser storage",
        returnTo: "/",
      };
      return;
    }

    let expectedCsrfToken: unknown;
    let returnTo = "/";
    let expires: unknown;
    try {
      const parsed = JSON.parse(ownCsrfStateRaw) as Record<string, unknown>;
      expectedCsrfToken = parsed.token;
      returnTo = sanitizeReturnTo(
        typeof parsed.returnTo === "string" ? parsed.returnTo : null,
      );
      expires = parsed.expires;
    } catch {
      viewState = {
        type: "error",
        message: "Invalid CSRF state in browser storage",
        returnTo: "/",
      };
      return;
    }

    if (typeof expires !== "number" || Date.now() > expires) {
      viewState = {
        type: "error",
        message:
          "Login attempt expired. (You took too long to complete the login)",
        returnTo,
      };
      return;
    }

    const params = page.url.searchParams;
    const realCsrfToken = params.get("state");
    if (realCsrfToken == null || realCsrfToken === "") {
      viewState = {
        type: "error",
        message: "State parameter not correctly present on request",
        returnTo,
      };
      return;
    }

    if (realCsrfToken !== expectedCsrfToken) {
      viewState = {
        type: "error",
        message: "CSRF tokens do not match",
        returnTo,
      };
      return;
    }

    const errorCode = params.get("error");
    if (errorCode != null) {
      if (errorCode === "access_denied") {
        // User pressed cancel, don't show them an error, just return them to where they came from.
        void goto(returnTo, { replaceState: true });
        return;
      }
      let errorMessage = `Authorization completed with error code ${errorCode}`;
      const errorDescription = params.get("error_description");
      if (errorDescription != null) {
        errorMessage += ` (Description: ${errorDescription})`;
      }
      viewState = {
        type: "error",
        message: errorMessage,
        returnTo,
      };
      return;
    }

    const code = params.get("code");
    if (code == null || code === "") {
      viewState = {
        type: "error",
        message: "Missing code parameter, or not correctly specified",
        returnTo,
      };
      return;
    }

    updateAuthState({ type: "loading" });

    void (async () => {
      try {
        const newAuthState = await createAuth(code);
        updateAuthState(newAuthState);
        await goto(returnTo, { replaceState: true });
      } catch (err) {
        console.error("API Request to create authorization failed", err);
        updateAuthState({ type: "missing" });
        viewState = {
          type: "error",
          message: "API Request to create authorization failed",
          returnTo,
        };
      }
    })();
  });
</script>

<svelte:head>
  <title>Login – recent-messages</title>
</svelte:head>

<h1>Login</h1>
{#if viewState.type === "loadToken"}
  <div class="alert alert-primary" role="status">
    <h4 class="alert-heading">
      <span class="spinner" aria-hidden="true"></span>
      Logging in...
    </h4>
    <p>Completing login...</p>
  </div>
{:else}
  <div class="alert alert-danger" role="alert">
    <h4 class="alert-heading">Failed to log you in!</h4>
    <p>
      There was an unexpected error while trying to log you in. (Technical error
      details: {viewState.message})
    </p>
    <hr />
    <p>Click below to go back to where you came from.</p>
    <a class="btn btn-primary" href={viewState.returnTo}>Go back</a>
  </div>
{/if}
