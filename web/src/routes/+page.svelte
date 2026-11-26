<script lang="ts">
  import config from "../../config";
  import ObfuscatedEmail from "#lib/ObfuscatedEmail.svelte";
</script>

<svelte:head>
  <title>recent-messages</title>
</svelte:head>

<h1>recent-messages Home</h1>

<section>
  <h4>What is this?</h4>
  <p>
    Normally, on Twitch chat, you can't see any messages that were sent before
    you joined a certain channel's chat. This service fills that gap. It
    continuously listens to a large number of channels - and when somebody wants
    to open a channel's chat, their chat client can use this service to fetch a
    list of recent messages.
  </p>
  <p>
    This service is already integrated into a number of Twitch chat clients,
    such as <a href="https://chatterino.com/">Chatterino</a> (Windows, macOS,
    Linux) and
    <a
      href="https://play.google.com/store/apps/details?id=com.flxrs.dankchat&hl=de"
    >
      DankChat
    </a>
    (Android app).
  </p>
</section>

<section>
  <h4>How does it work?</h4>
  <p>
    Any chat client can integrate with this service using the provided
    <a href="/api">API</a>. Whenever somebody requests a channel's recent
    messages via the API, the service will open up that channel's chat and start
    listening and collecting messages. It will stay connected to that channel as
    long as somebody keeps requesting the history for that channel.
  </p>
  <p>
    The service temporarily saves messages received from all connected channels,
    for a maximum time of {config.messages_expire_after}. Additionally, on
    channels with a lot of messages, the service will not store more than
    {config.max_buffer_size} messages at once (old messages will be deleted to make
    room in case the limit is reached).
  </p>
  <p>
    If a channel's message history is not requested by anyone within a timespan
    of {config.channels_expire_after}, the service will automatically stop
    listening for messages in that channel. As the messages reach the expiry
    mark of {config.messages_expire_after} mentioned above, they will then also be
    removed.
  </p>
</section>

<section>
  <h4>What can I do here?</h4>
  <p>
    As a channel owner, you can use the <a href="/settings">Settings</a> page to disable
    this service for your channel, or just purge the currently stored recent messages
    as a one-time thing.
  </p>
  <p>
    If you are a developer looking to integrate a chat client with this service,
    see the <a href="/api">API documentation</a>.
  </p>
</section>

<section>
  <h4>Contact and Owner information</h4>
  <p>{config.service_owner_info} You can contact me, if you need:</p>
  <ul>
    {#if config.issues_url != null}
      <li>
        If it's about some general issue with the service, please use the
        <a href={config.issues_url}>GitHub issue tracker</a>.
      </li>
    {/if}
    <li>
      <!-- the rot13 thing is to prevent plain email scraping from GitHub/hosted JS files, to reduce spam :) -->
      For everything else, send an email:
      <ObfuscatedEmail rot13={config.general_contact_email_rot13} />
    </li>
  </ul>
</section>

<section>
  <h4>About the service</h4>
  <p>
    Everything about this service is free and open source. You can find the
    source code <a href={config.repo_url}>on GitHub</a>.
    <br />
    This web service is written in Rust and licensed under the GNU Affero General
    Public License (<a href={`${config.repo_url}/blob/master/LICENSE`}>
      GNU AGPL
    </a>) version 3 or later.
  </p>
  <p>
    This version of the recent-messages service is a rewrite of the
    <a href="https://github.com/robotty/recent-messages">
      "version 1" of the service
    </a>.
  </p>
</section>

{#if config.enable_donation_section}
  <section>
    <h4 id="donate">Donate</h4>
    <p>
      I made and run this project in my free time, and I don't want to collect
      money for doing that. However <em>running</em> the service requires a server,
      which is not free. I currently pay about 10€ per month to run this service (that's
      just the server costs). If you are feeling generous, you can help pay for the
      server costs by becoming a monthly sponsor through GitHub Sponsors, or by giving
      a one-time donation:
    </p>
    <div class="btn-group">
      <a
        href="https://github.com/sponsors/RAnders00"
        target="_blank"
        rel="noopener noreferrer"
        class="btn btn-primary"
      >
        Become a GitHub sponsor
      </a>
      <a
        href="https://paypal.me/RAnders00"
        target="_blank"
        rel="noopener noreferrer"
        class="btn btn-outline"
      >
        Donate using PayPal
      </a>
      <a
        href="https://streamelements.com/randers/tip"
        target="_blank"
        rel="noopener noreferrer"
        class="btn btn-outline"
      >
        Donate using StreamElements
      </a>
    </div>
  </section>
  <section>
    <h4>Rent servers at netcup</h4>
    <p>
      You can also, at no additional cost, use my referral code when renting out
      servers from my hosting provider <strong>netcup</strong>, you save 5€ on
      your first order, and you are giving me a small comission:
      <a href="https://www.netcup.eu" target="_blank" rel="noopener noreferrer">
        Visit netcup
      </a>. Use promo code <code>36nc15963703760</code> on your first order to get
      5€ off.
    </p>
  </section>
{/if}
