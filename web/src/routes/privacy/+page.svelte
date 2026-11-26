<script lang="ts">
  import config from "../../../config";
  import ObfuscatedEmail from "#lib/ObfuscatedEmail.svelte";

  // Remember to update privacy_last_updated_on in config.ts when making changes here!
</script>

<svelte:head>
  <title>Privacy Policy – recent-messages</title>
</svelte:head>

<h1>Privacy Policy</h1>

<p>
  This privacy policy will explain how I use your personal data I collect from
  you when you use my website and service.
</p>

<h4>
  What data do I collect? How do I collect your data? How will I use your data?
  How do I store your data?
</h4>
<p>I collect the following data:</p>
<ul>
  <li>
    Individual chat messages sent to Twitch.tv channels that this service is
    actively listening to.
    <br />
    These chat messages are received from a public Twitch.tv messaging API (https://dev.twitch.tv/docs/irc).
    <br />
    The service only collects messages for a channel given that any user has requested
    the messages for that channel within the last {config.channels_expire_after}.
    <br />
    The service automatically stops listening to the messages sent to a Twitch.tv
    channel within the next 30 minutes once the channel's historical messages have
    not been accessed by anyone for {config.channels_expire_after}. A stored
    chat message is automatically permanently deleted within the next 30 minutes
    once it becomes older than {config.messages_expire_after}.
  </li>
  <li>
    Your basic Twitch.tv profile data (Twitch username, Twitch user ID, Link to
    profile picture).
    <br />
    This information is collected from the public Twitch.tv API (https://dev.twitch.tv/docs/api/).
    <br />
    This information is only collected if and when you complete the Login feature
    of the website.
    <br />
    User name and ID are only used to confirm your authorization as the owner of a
    given Twitch.tv channel (authorization).
    <br />
    The link to the profile picture is only used for the website to be able to show
    you your own profile picture. The actual profile picture image is not downloaded
    or stored at any time.
    <br />
    This information is stored on my server. Additionally, the information is stored
    in your browser's local storage to allow you to revisit the website without having
    to log in again. The information is automatically permanently deleted from your
    browser's local storage in case you visit the website after not having visited
    the website for {config.sessions_expire_after}. On the server-side, the
    information automatically becomes expired and inaccessible once
    {config.sessions_expire_after} have passed without you having visited the website
    with the same browser. Additionally, the information is also permanently from
    both the server as well as the browser's local storage if you explicitly "Log
    out" on the website.
  </li>
</ul>

<p>
  I justify my collection of chat messages (first point above) with Article 6
  Section 1 Point (e) of the GDPR: "Processing shall be lawful only if and to
  the extent that at least one of the following applies: [...] (e) processing is
  necessary for the performance of a task carried out in the public interest or
  in the exercise of official authority vested in the controller".
</p>
<p>
  I justify my collection of basic Twitch.tv user data (second point above)
  because this information is strictly necessary to provide the user with the
  service and to automatically identify them.
</p>

<h4>How do I store your data?</h4>
<p>{config.privacy_how_do_i_store_your_data}</p>

<h4>What are your data protection rights?</h4>
<p>
  I would like to make sure you are fully aware of all of your data protection
  rights. Every user is entitled to the following:
</p>
<ul>
  <li>
    <em>The right to access</em> – You have the right to request me for copies of
    your personal data. I may charge you a small fee for this service.
  </li>
  <li>
    <em>The right to rectification</em> – You have the right to request that I correct
    any information you believe is inaccurate. You also have the right to request
    me to complete the information you believe is incomplete.
  </li>
  <li>
    <em>The right to erasure</em> – You have the right to request that I erase your
    personal data, under certain conditions.
  </li>
  <li>
    <em>The right to restrict processing</em> – You have the right to request that
    I restrict the processing of your personal data, under certain conditions.
  </li>
  <li>
    <em>The right to object to processing</em> – You have the right to object to my
    processing of your personal data, under certain conditions.
  </li>
  <li>
    <em>The right to data portability</em> – You have the right to request that I
    transfer the data that I have collected to another organization, or directly to
    you, under certain conditions.
  </li>
</ul>

<p>
  If you make a request, I have one month to respond to you. If you would like
  to exercise any of these rights, please contact me at my email:
  <ObfuscatedEmail rot13={config.privacy_contact_email_rot13} linked={false} />
</p>
<p>
  Regardless of these rights, this service offers an automated opt-out service
  that allows you to delete the historical messages for your own Twitch.tv
  channel(s), as well as to blacklist your channel so that the service will not
  collect messages from it anymore. Please visit the
  <a href="/settings">Settings</a> page linked above to use this feature.
</p>

<h4>Privacy policies of other websites</h4>
<p>
  This website contains links to other websites. My privacy policy applies only
  to my website, so if you click on a link to another website, you should read
  their privacy policy.
</p>

<h4>Changes to our privacy policy</h4>
<p>
  I keep my privacy policy under regular review and place any updates on this
  web page. This privacy policy was last updated on
  {config.privacy_last_updated_on}.
</p>

<h4>How to contact me</h4>
<p>
  If you have any questions about my privacy policy, the data I hold on you, or
  you would like to exercise one of your data protection rights, please do not
  hesitate to contact me. Email me at:
  <ObfuscatedEmail rot13={config.privacy_contact_email_rot13} linked={false} />
</p>
