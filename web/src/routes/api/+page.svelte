<script lang="ts">
  import config from "../../../config";
</script>

<svelte:head>
  <title>API – recent-messages</title>
</svelte:head>

<h1>API</h1>

<div class="btn-group" style="margin-bottom: 1.5rem;">
  <a class="btn btn-primary" href="/api/swagger">
    Interactive API Reference (Swagger UI)
  </a>
  <a
    class="btn btn-outline"
    href="/openapi.json"
    target="_blank"
    rel="noopener noreferrer"
  >
    OpenAPI Spec (JSON)
  </a>
</div>

<section>
  <h4>General information</h4>
  <p>
    This service exposes an API at the base URL
    <code>{config.api_base_url}/</code>.
  </p>
  <p>
    CORS is enabled for the entire API, for all origins. You are free to use
    this API from client-side web apps and applications.
  </p>
  <p>
    If you are developing a user-facing application and planning to integrate
    with this service, you must follow these basic guidelines:
    <br />
    If feasible, you should always prefer to get user's consent via a opt-in before
    the integration is enabled for them. On the Opt-in dialog, include a short paragraph
    about what the service does and what feature(s) it provides in your application,
    and give a link to this website (
    <code>https://recent-messages.robotty.de/</code>).
    <a href="https://imgur.com/a/VjPxPBE">
      Here is a good example that you can follow.
    </a>
  </p>
  <p>
    If it's not realistic/possible to use an opt-in system, you must at the very
    least have an opt-out settings section with the same sort of information
    that would otherwise be shown on the opt-in dialog (info text, opt-out
    toggle, link to website).
  </p>
  <p>
    If in doubt, please contact me about your integration and ask for
    help/confirmation before going forward. Contact details can be found on the
    <a href="/">home page</a>.
  </p>
</section>

<h4>Endpoints</h4>
<section>
  <h5>Get recent messages</h5>
  <p>
    <code>GET {config.api_base_url}/recent-messages/:channel_login</code>
  </p>

  <h6>Path parameters:</h6>
  <ul>
    <li>
      <code>channel_login</code>: Twitch login name of the channel messages
      should be returned for
    </li>
  </ul>

  <h6>Query parameters:</h6>
  <ul>
    <li>
      <code>?hide_moderation_messages=true/false</code>: Omits
      <code>CLEARCHAT</code> and <code>CLEARMSG</code> messages from the
      response. Optional, defaults to <code>false</code>.
    </li>
    <li>
      <code>?hide_moderated_messages=true/false</code>: Omits all messages from
      the response that have been deleted by a <code>CLEARCHAT</code> or
      <code>CLEARMSG</code> message. Optional, defaults to <code>false</code>.
    </li>
    <li>
      <p>
        <code>?clearchat_to_notice=true/false</code>: Converts
        <code>CLEARCHAT</code> messages into <code>NOTICE</code> messages with a user-presentable
        message.
      </p>
      <p>
        Examples:
        <br />
        <code>
          @historical=1;msg-id=rm-clearchat;rm-received-ts=1596058443362
          :tmi.twitch.tv NOTICE #randers :Chat has been cleared by a moderator.
        </code>
        <br />
        <code>
          @historical=1;msg-id=rm-timeout;rm-received-ts=1596058460738
          :tmi.twitch.tv NOTICE #randers :ed0mer has been timed out for 10m 30s.
        </code>
        <br />
        <code>
          @historical=1;msg-id=rm-permaban;rm-received-ts=1596058421611
          :tmi.twitch.tv NOTICE #pajlada :a_bad_user has been permanently
          banned.
        </code>
      </p>
      <p>
        The <code>msg-id</code> will be set to <code>rm-clearchat</code>,
        <code>rm-timeout</code> or <code>rm-permaban</code>, corresponding with
        the type of message.
      </p>
      <p>
        This option was originally introduced as a "quick fix" to get
        recent-messages integration to work with Chatterino. This option is now
        only retained to keep compatibility with old versions of Chatterino. You
        should try and avoid using this option if possible.
      </p>
      <p>
        Optional, defaults to <code>false</code>.
      </p>
    </li>
    <li>
      <code>?limit=n</code>: Limit the number of messages returned. If more than
      <code>n</code> messages are available for the requested channel, the
      response is limited to the <code>n</code> newest messages. Optional,
      defaults to no limit (up to {config.max_buffer_size} messages). Can be combined
      with <code>?before=</code> and <code>?after=</code>, the three
      requirements combine using a logical AND.
      <br />
      <strong>
        It is strongly recommended that you set this to some reasonable number
        based on your application, e.g. 50 or 100 tends to be more than enough
        for general-purpose chat clients.
      </strong>
    </li>
    <li>
      <code>?before=n</code>: Only return messages that were received before (<code>
        &lt;
      </code>) this timestamp (in milliseconds since the unix epoch, this refers
      to the
      <code>rm-received-rs</code> timestamp). Optional, can be combined with
      <code>?limit=</code> and <code>?after=</code>, the three requirements
      combine using a logical AND.
    </li>
    <li>
      <code>?after=n</code>: Only return messages that were received after (<code>
        &gt;
      </code>) this timestamp (in milliseconds since the unix epoch, this refers
      to the
      <code>rm-received-rs</code> timestamp). Optional, can be combined with
      <code>?limit=</code> and <code>?before=</code>, the three requirements
      combine using a logical AND.
    </li>
  </ul>

  <h6>Response format:</h6>
  <pre><code
      >{`{
    "messages": [
        "@badge-info=;badges=glhf-pledge/1;color=;display-name=purplereddish;emotes=;flags=;historical=1;id=dbb10be8-581e-4f22-ba12-2001e088529d;mod=0;rm-received-ts=1596061057185;room-id=71092938;subscriber=0;tmi-sent-ts=1596061056790;turbo=0;user-id=260969632;user-type= :purplereddish!purplereddish@purplereddish.tmi.twitch.tv PRIVMSG #xqcow LULW",
        "@badge-info=subscriber/9;badges=subscriber/9;client-nonce=33d3d9d8e82c331dd877b6c5f4ed52ba;color=#FF0000;display-name=daraz1505;emotes=;flags=;historical=1;id=a36cddd8-06ba-4aed-b693-f503adb97010;mod=0;rm-received-ts=1596061057504;room-id=71092938;subscriber=1;tmi-sent-ts=1596061057059;turbo=0;user-id=171679991;user-type= :daraz1505!daraz1505@daraz1505.tmi.twitch.tv PRIVMSG #xqcow :monkaW ????????",
        "@badge-info=subscriber/1;badges=subscriber/0,glhf-pledge/1;color=#FF69B4;display-name=prospector07;emote-only=1;emotes=1035681:0-3,5-8,10-13;flags=;historical=1;id=559b9ccd-11b5-472c-83ce-e16a66fc7533;mod=0;rm-received-ts=1596061057607;room-id=71092938;subscriber=1;tmi-sent-ts=1596061057234;turbo=0;user-id=531980156;user-type= :prospector07!prospector07@prospector07.tmi.twitch.tv PRIVMSG #xqcow :xqcH xqcH xqcH",
        "@badge-info=subscriber/2;badges=subscriber/2,glhf-pledge/1;client-nonce=299f6ffc0dabe15f806cb4844a2fdebc;color=#9ACD32;display-name=siriwhatsmyname;emotes=;flags=;historical=1;id=fa0856e3-45d7-496e-97bd-93f941fd96ce;mod=0;rm-received-ts=1596061058007;room-id=71092938;subscriber=1;tmi-sent-ts=1596061057535;turbo=0;user-id=208278150;user-type= :siriwhatsmyname!siriwhatsmyname@siriwhatsmyname.tmi.twitch.tv PRIVMSG #xqcow :WEEBSOUT WEEBSOUT WEEBSOUT WEEBSOUT WEEBSOUT WEEBSOUT",
        "@badge-info=;badges=;client-nonce=b45518444921abc86bcca74e6f961c31;color=#FF0000;display-name=Scarrov;emotes=;flags=;historical=1;id=05836fc5-76e0-45de-b0be-779c223b160b;mod=0;rm-received-ts=1596061058008;room-id=71092938;subscriber=0;tmi-sent-ts=1596061057664;turbo=0;user-id=195960862;user-type= :scarrov!scarrov@scarrov.tmi.twitch.tv PRIVMSG #xqcow ????????",
        "@historical=1;rm-received-ts=1596061229295;room-id=71092938;slow=5 :tmi.twitch.tv ROOMSTATE #xqcow",
        "@historical=1;msg-id=slow_on;rm-received-ts=1596061229296 :tmi.twitch.tv NOTICE #xqcow :This room is now in slow mode. You may send messages every 5 seconds.",
        "@badge-info=subscriber/29;badges=subscriber/24;color=#7AC2A7;display-name=gw_ua;emotes=;flags=;historical=1;id=3391449d-3427-490f-836b-f5b8c1c98b93;mod=0;rm-deleted=1;rm-received-ts=1596059993412;room-id=71092938;subscriber=1;tmi-sent-ts=1596059993026;turbo=0;user-id=81302568;user-type= :gw_ua!gw_ua@gw_ua.tmi.twitch.tv PRIVMSG #xqcow :gn i guess",
        "@historical=1;login=gw_ua;rm-received-ts=1596061327989;room-id=;target-msg-id=3391449d-3427-490f-836b-f5b8c1c98b93;tmi-sent-ts=1596061327703 :tmi.twitch.tv CLEARMSG #xqcow :gn i guess",
    ],
    "error": null,
    "error_code": null
}`}</code
    ></pre>

  <p>
    Returns up to {config.max_buffer_size} messages. Messages are ordered oldest-to-newest.
    Messages are retured in raw IRC format, without trailing newline(s). The API returns
    <code>PRIVMSG</code>, <code>CLEARCHAT</code>, <code>CLEARMSG</code>,
    <code>USERNOTICE</code>, <code>NOTICE</code> and <code>ROOMSTATE</code> messages.
  </p>
  <p>
    All messages returned carry the <code>historical=1</code> tag.
  </p>
  <p>
    In addition to the IRC tags originally sent by the Twitch IRC server, all
    messages additionally carry the <code>rm-received-ts</code> tag. Its format
    is similar to the <code>tmi-sent-ts</code> tag sent by Twitch on some
    message types (the value is a timestamp, it contains the number of
    milliseconds since Jan 01 1970 00:00:00 UTC, the unix epoch). The
    difference/advantage of the <code>rm-received-ts</code> tag is that it is
    present on <em>all</em> messages, allowing clients to use it like one would use
    the current clock time when receiving normal messages.
  </p>
  <p>
    Messages that were deleted by some moderation action additionally carry the
    <code>rm-deleted=1</code> tag. Note that although typically one would only
    consider <code>PRIVMSG</code> and <code>USERNOTICE</code> messages to be
    something that "can be deleted", the <code>rm-deleted=1</code> can actually be
    placed on every type of message, as a result of a moderator clearing the entire
    chat.
  </p>
  <p>
    Please also note that my implementation may re-order the tags. For this
    reason, you must not rely on IRCv3 tags in the message to be in any
    particular order. Please see
    <a href="https://ircv3.net/specs/extensions/message-tags">the standard</a>
    for further caveats, especially these: "The ordering of tags is not meaningful.",
    and: "Implementations MUST interpret empty tag values (e.g.
    <code>foo=</code>) as equivalent to missing tag values (e.g.
    <code>foo</code>). Specifications MUST NOT differentiate meaning between
    tags with empty and missing values."
  </p>
  <p>
    Lastly, please ensure that you are parsing the listed messages with an
    <a href="https://www.rfc-editor.org/rfc/rfc2812#section-2.3.1">
      RFC 2812
    </a>-compliant IRC message parser. You must especially not rely on the last
    parameter being marked as "trailing", since the "<code>:</code>" character
    marking the last parameter of the IRC message will be omitted should it not
    be necessary to form a valid message. (Quoting from the spec: "After
    extracting the parameter list, all parameters are equal whether matched by
    &lt;middle&gt; or &lt;trailing&gt;. &lt;trailing&gt; is just a syntactic
    trick to allow SPACE within the parameter.") For example, this is a message
    as you might find it in the API response of this service:
  </p>
  <pre><code
      >@user-id=132532813;badge-info=;badges=turbo/1;tmi-sent-ts=1673907853307;mod=0;historical=1;returning-chatter=0;flags=;color=#EF8A12;subscriber=0;rm-received-ts=1673907853790;user-type=;display-name=icelys;room-id=11148817;emotes=;turbo=1;id=c220d6d3-6a55-403f-9094-94a80ae2cdcd;first-msg=0 :icelys!icelys@icelys.tmi.twitch.tv PRIVMSG #pajlada FeelsBadMan</code
    ></pre>
  <p>
    Note the non-deterministic order of the tags as well as the lack of a "<code>
      :
    </code>" character before the "<code>FeelsBadMan</code>".
  </p>
  <p>
    If you are going to integrate the returned messages into a chat application
    that also connects to real chat at the same time, it's advisable to filter
    duplicate messages (messages that your chat client already received "live"
    that were also returned by the API). Your best bet is to base this on a
    "weak equality": Check for strict equality between the parsed IRC messages,
    but allow extra tags on the recent-messages one. (There is no guarantee that
    the tags documented here might not be extended one day)
  </p>
  <p>
    The <code>error</code> and <code>error_code</code> parameters are either
    both <code>null</code> or both set to some string value.
    <br />
    Despite their name, if <code>error</code> and <code>error_code</code> are
    not null, it does not signifiy a hard failure of the request. The API will
    still normally return messages.
    <strong>
      Most applications should therefore ignore the <code>error</code> parameter
    </strong>
    and also not show it to users. Its presence is purely informational.
  </p>
  <p>
    <code>error</code> is a readable error message, however it is not meant to
    be shown to the user. <code>error_code</code> is a machine-readable error code
    string.
  </p>
  <p>
    Currently, the only valid <code>error_code</code> is
    <code>channel_not_joined</code>, which signifies that the service is
    currently not listening to messages in that channel. This error can arise
    when recent messages are requested for nonexistant/deleted/suspended
    channels, and it will also be returned if this request is the first request
    for that channel. However there are many more combinations of internal
    events that can cause ta channel to currently not be joined, such as a
    service restart, a reconnect, etc.
  </p>

  <h6>Errors</h6>
  <p>
    If the provided channel is blacklisted from the service (ignored), HTTP
    Status Code 403 is returned with the following body:
  </p>
  <pre><code
      >{`{
    "status": 403,
    "status_message": "Forbidden",
    "error": "The channel login \`randers\` is excluded from this service",
    "error_code": "channel_ignored"
}`}</code
    ></pre>
  <p>
    If the provided channel name is of invalid format, HTTP Status Code 400 is
    returned with the following body:
  </p>
  <pre><code
      >{`{
    "status": 400,
    "status_message": "Bad Request",
    "error": "Invalid channel login \`this_name_is_way_tooooo_long\`",
    "error_code": "invalid_channel_login"
}`}</code
    ></pre>
</section>
