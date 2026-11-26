<script lang="ts">
  import { onMount } from "svelte";
  import { afterNavigate } from "$app/navigation";
  import { page } from "$app/state";
  import { auth } from "#lib/auth.svelte";
  import Icon from "#lib/Icons.svelte";

  let menuOpen = $state(false);
  let dropdownOpen = $state(false);
  let dropdownElement = $state<HTMLDivElement | null>(null);

  afterNavigate(() => {
    menuOpen = false;
    dropdownOpen = false;
  });

  function handleWindowClick(event: MouseEvent) {
    if (
      dropdownOpen &&
      dropdownElement &&
      event.target instanceof Node &&
      !dropdownElement.contains(event.target)
    ) {
      dropdownOpen = false;
    }
  }

  function handleWindowKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      dropdownOpen = false;
      menuOpen = false;
    }
  }

  let mounted = $state(false);
  onMount(() => {
    mounted = true;
  });

  const returnToParam = $derived(
    encodeURIComponent(
      mounted
        ? page.url.pathname + page.url.search + page.url.hash
        : page.url.pathname,
    ),
  );
</script>

<svelte:window onclick={handleWindowClick} onkeydown={handleWindowKeydown} />

<header class="site-header">
  <nav class="container navbar" aria-label="Main navigation">
    <div class="navbar-top">
      <a class="brand" href="/">recent-messages</a>
      <button
        type="button"
        class="nav-toggle"
        aria-expanded={menuOpen}
        aria-controls="main-nav-menu"
        aria-label="Toggle navigation"
        onclick={() => (menuOpen = !menuOpen)}
      >
        <Icon name="menu" />
      </button>
    </div>

    <div id="main-nav-menu" class="nav-collapse" class:open={menuOpen}>
      <ul class="nav-links">
        <li>
          <a
            class="nav-link"
            href="/"
            aria-current={page.url.pathname === "/" ? "page" : undefined}
          >
            Home
          </a>
        </li>
        <li>
          <a
            class="nav-link"
            href="/settings"
            aria-current={page.url.pathname === "/settings"
              ? "page"
              : undefined}
          >
            Settings
          </a>
        </li>
        <li>
          <a
            class="nav-link"
            href="/api"
            aria-current={page.url.pathname === "/api" ||
            page.url.pathname.startsWith("/api/")
              ? "page"
              : undefined}
          >
            API
          </a>
        </li>
        <li>
          <a
            class="nav-link"
            href="/privacy"
            aria-current={page.url.pathname === "/privacy" ? "page" : undefined}
          >
            Privacy
          </a>
        </li>
      </ul>

      <div class="nav-auth">
        {#if auth.current.type === "missing"}
          {#if page.url.pathname === "/authorized"}
            <span class="nav-status">Logging you in...</span>
          {:else}
            <a
              class="nav-link"
              href={`/login?returnTo=${returnToParam}`}
              data-sveltekit-preload-data="off"
            >
              <Icon name="login" />
              Login
            </a>
          {/if}
        {:else if auth.current.type === "loading"}
          <span class="nav-status">Logging you in...</span>
        {:else if auth.current.userDetailsValidating}
          <span class="nav-status">Checking login status...</span>
        {:else}
          <div class="dropdown" bind:this={dropdownElement}>
            <button
              type="button"
              class="dropdown-trigger"
              aria-expanded={dropdownOpen}
              aria-haspopup="menu"
              onclick={() => (dropdownOpen = !dropdownOpen)}
            >
              <img
                class="profile-image"
                src={auth.current.userProfileImageUrl}
                alt=""
              />
              <span>{auth.current.userName}</span>
              <Icon name="chevron-down" />
            </button>
            {#if dropdownOpen}
              <div class="dropdown-menu" role="menu">
                <a class="dropdown-item" href="/settings" role="menuitem">
                  Settings
                </a>
                <a
                  class="dropdown-item"
                  href={`/logout?returnTo=${returnToParam}`}
                  role="menuitem"
                  data-sveltekit-preload-data="off"
                >
                  <Icon name="logout" />
                  Log Out
                </a>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  </nav>
</header>
