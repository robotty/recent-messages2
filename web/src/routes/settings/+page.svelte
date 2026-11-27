<script lang="ts">
  import { onMount } from "svelte";
  import config from "../../../config";
  import { auth, loadStoredAuth } from "#lib/auth.svelte";
  import Icon from "#lib/Icons.svelte";

  let ignored = $state(false);
  let loadingIgnored = $state(false);
  let loadingIgnoredFailed = $state(false);
  let savingIgnored = $state(false);
  let savingIgnoredSuccess = $state(false);
  let savingIgnoredFailed = $state(false);
  let currentlyPurging = $state(false);
  let purgeSuccess = $state(false);
  let purgeFailed = $state(false);

  let authLoaded = $state(false);

  onMount(() => {
    loadStoredAuth();
    authLoaded = true;
  });

  $effect(() => {
    if (!authLoaded) return;

    const current = auth.current;
    if (current.type !== "present") {
      return;
    }

    const accessToken = current.accessToken;
    const controller = new AbortController();

    loadingIgnored = true;
    loadingIgnoredFailed = false;

    void (async () => {
      try {
        const response = await fetch(`${config.api_base_url}/ignored`, {
          method: "GET",
          headers: {
            Authorization: `Bearer ${accessToken}`,
            Accept: "application/json",
          },
          signal: controller.signal,
        });
        if (!response.ok) {
          throw new Error(response.statusText);
        }

        const json = (await response.json()) as { ignored: boolean };
        ignored = json.ignored;
        loadingIgnored = false;
      } catch (err) {
        if (controller.signal.aborted) return;
        console.error("Failed to load `ignored` status of channel", err);
        loadingIgnored = false;
        loadingIgnoredFailed = true;
      }
    })();

    return () => {
      controller.abort();
    };
  });

  async function updateIgnored(event: Event) {
    if (auth.current.type !== "present") return;

    const target = event.currentTarget as HTMLInputElement;
    const previousSetting = ignored;
    const newSetting = target.checked;

    ignored = newSetting;
    savingIgnored = true;
    savingIgnoredFailed = false;
    savingIgnoredSuccess = false;
    purgeSuccess = false;
    purgeFailed = false;

    try {
      const response = await fetch(`${config.api_base_url}/ignored`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${auth.current.accessToken}`,
          Accept: "application/json",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ ignored: newSetting }),
      });
      if (!response.ok) {
        throw new Error(response.statusText);
      }
      savingIgnored = false;
      savingIgnoredSuccess = true;
    } catch (err) {
      console.error("Failed to save `ignored` status of channel", err);
      ignored = previousSetting;
      savingIgnored = false;
      savingIgnoredFailed = true;
    }
  }

  async function purgeMessages() {
    if (auth.current.type !== "present") return;

    currentlyPurging = true;
    savingIgnoredFailed = false;
    savingIgnoredSuccess = false;
    purgeFailed = false;
    purgeSuccess = false;

    try {
      const response = await fetch(`${config.api_base_url}/purge`, {
        method: "POST",
        headers: {
          Authorization: `Bearer ${auth.current.accessToken}`,
          Accept: "application/json",
        },
      });
      if (!response.ok) {
        throw new Error(response.statusText);
      }
      currentlyPurging = false;
      purgeSuccess = true;
    } catch (err) {
      console.error("Failed to purge messages in channel", err);
      currentlyPurging = false;
      purgeFailed = true;
    }
  }
</script>

<svelte:head>
  <title>Settings – recent-messages</title>
</svelte:head>

<h1>Settings</h1>

{#if auth.current.type === "present"}
  <section>
    <p>You can make the following privacy settings for your own channel:</p>
  </section>

  <section class="card">
    <div class="setting-item">
      <div class="setting-control">
        <label
          class="switch-label"
          class:disabled={savingIgnored ||
            loadingIgnored ||
            loadingIgnoredFailed}
          for="ignored"
        >
          <input
            id="ignored"
            type="checkbox"
            role="switch"
            class="switch-input"
            disabled={savingIgnored || loadingIgnored || loadingIgnoredFailed}
            checked={ignored}
            onchange={updateIgnored}
            aria-describedby="ignored-help-block"
          />
          <span>Blacklist my channel</span>
        </label>

        {#if savingIgnored}
          <span class="status-text status-primary">
            <span class="spinner" aria-hidden="true"></span>
            Saving changes…
          </span>
        {:else if loadingIgnored}
          <span class="status-text status-primary">
            <span class="spinner" aria-hidden="true"></span>
            Loading settings…
          </span>
        {/if}

        {#if savingIgnoredSuccess}
          <span class="status-text status-success">
            <Icon name="check" /> Saved.
          </span>
        {/if}
        {#if savingIgnoredFailed}
          <span class="status-text status-danger">
            <Icon name="x" /> Failed to save. Please try again.
          </span>
        {/if}
        {#if loadingIgnoredFailed}
          <span class="status-text status-danger">
            <Icon name="x" /> Failed to load current setting. Please try again.
          </span>
        {/if}
      </div>

      <p class="setting-help" id="ignored-help-block">
        Removes your channel entirely from the service. No messages will be
        recorded and nobody will be able to load any recent messages for your
        channel. Additionally, messages currently stored for your channel will
        be deleted.
      </p>
    </div>

    <div class="setting-item">
      <div class="setting-control">
        <span class="tooltip-wrap">
          <button
            type="button"
            class="btn btn-danger"
            aria-describedby={ignored
              ? "purge-help-block purge-disabled-tooltip"
              : "purge-help-block"}
            onclick={purgeMessages}
            disabled={ignored ||
              currentlyPurging ||
              loadingIgnored ||
              loadingIgnoredFailed}
          >
            Purge messages
          </button>
          {#if ignored}
            <span
              id="purge-disabled-tooltip"
              class="tooltip-bubble"
              role="tooltip"
            >
              You cannot purge messages for your channel because you have set
              your channel to be blacklisted. Since your channel is blacklisted,
              no messages are stored for it.
            </span>
          {/if}
        </span>

        {#if currentlyPurging}
          <span class="status-text status-primary">
            <span class="spinner" aria-hidden="true"></span>
            Purging messages…
          </span>
        {/if}
        {#if purgeSuccess}
          <span class="status-text status-success">
            <Icon name="check" /> Messages have been purged.
          </span>
        {/if}
        {#if purgeFailed}
          <span class="status-text status-danger">
            <Icon name="x" /> Failed to purge messages. Please try again.
          </span>
        {/if}
      </div>

      <p class="setting-help" id="purge-help-block">
        Remove all messages currently stored for your channel. This is a
        one-time action. Purging messages cannot be undone.
      </p>
    </div>
  </section>
{:else if auth.current.type === "loading" || !authLoaded}
  <div class="alert alert-primary" role="status">
    <h4 class="alert-heading">
      <span class="spinner" aria-hidden="true"></span>
      Loading...
    </h4>
    <p>Checking authentication status...</p>
  </div>
{:else}
  <div class="alert alert-warning" role="alert">
    <h4 class="alert-heading">Not logged in</h4>
    <p>
      You are currently not logged in. Use the button below or the link on the
      navigation bar to log in.
    </p>
    <a class="btn btn-primary" href="/login?returnTo=%2Fsettings">
      <Icon name="login" />
      Log in
    </a>
  </div>
{/if}
