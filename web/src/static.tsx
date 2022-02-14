import * as React from "react";
import { Link } from "react-router-dom";
import config from "../config";

function rot13(s: string): string {
  return s.replace(
    /[A-Z]/gi,
    (c) =>
      "NOPQRSTUVWXYZABCDEFGHIJKLMnopqrstuvwxyzabcdefghijklm"[
        "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz".indexOf(c)
      ]
  );
}

export function Home() {
  return (
    <>
      <h1>recent-messages Home</h1>
      <section>
        <h4>What is this?</h4>
        <p>
          Normally, on Twitch chat, you can't see any messages that were sent
          before you joined a certain channel's chat. This service fills that
          gap. It continuously listens to a large number of channels - and when
          somebody wants to open a channel's chat, their chat client can use
          this service to fetch a list of recent messages.
        </p>
        <p>
          This service is already integrated into a number of Twitch chat
          clients, such as <a href="https://chatterino.com/">Chatterino</a>{" "}
          (Windows, macOS, Linux) and{" "}
          <a href="https://play.google.com/store/apps/details?id=com.flxrs.dankchat&hl=de">
            DankChat
          </a>{" "}
          (Android app).
        </p>
      </section>
      <section>
        <h4>How does it work?</h4>
        <p>
          Any chat client can integrate with this service using the provided{" "}
          <Link to="/api">API</Link>. Whenever somebody requests a channel's
          recent messages via the API, the service will open up that channel's
          chat and start listening and collecting messages. It will stay
          connected to that channel as long as somebody keeps requesting the
          history for that channel.
        </p>
        <p>
          The service temporarily saves messages received from all connected
          channels, for a maximum time of {config.messages_expire_after}.
          Additionally, on channels with a lot of messages, the service will not
          store more than {config.max_buffer_size} messages at once (old
          messages will be deleted to make room in case the limit is reached).
        </p>
        <p>
          If a channel's message history is not requested by anyone within a
          timespan of {config.channels_expire_after}, the service will
          automatically stop listening for messages in that channel. As the
          messages reach the expiry mark of {config.messages_expire_after}{" "}
          mentioned above, they will then also be removed.
        </p>
      </section>
      <section>
        <h4>What can I do here?</h4>
        <p>
          As a channel owner, you can use the{" "}
          <Link to="/settings">Settings</Link> page to disable this service for
          your channel, or just purge the currently stored recent messages as a
          one-time thing.
        </p>
        <p>
          If you are a developer looking to integrate a chat client with this
          service, see the <Link to="/api">API documentation</Link>.
        </p>
      </section>
      <section>
        <h4>Contact and Owner information</h4>
        {config.service_owner_info} You can contact me, if you need:
        <ul>
          {config.issues_url != null ? (
            <li>
              If it's about some general issue with the service, please use the{" "}
              <a href={config.issues_url}>GitHub issue tracker</a>.
            </li>
          ) : null}
          <li>
            {/* the rot13 thing is to prevent plain email scraping from GitHub/hosted JS files, to reduce spam :) */}
            For everything else, send an email:{" "}
            <a href={"mailto:" + rot13(config.general_contact_email_rot13)}>
              {rot13(config.general_contact_email_rot13)}
            </a>
          </li>
        </ul>
      </section>
      <section>
        <h4>About the service</h4>
        <p>
          Everything about this service is free and open source. You can find
          the source code <a href={config.repo_url}>on GitHub</a>.
          <br />
          This web service is written in Rust and licensed under the GNU Affero
          General Public License (
          <a href={`${config.repo_url}/blob/master/LICENSE`}>GNU AGPL</a>)
          version 3 or later.
        </p>
        <p>
          This version of the recent-messages service is a rewrite of the{" "}
          <a href="https://github.com/robotty/recent-messages">
            "version 1" of the service
          </a>
          .
        </p>
      </section>
      {config.enable_donation_section ? (
        <>
          <section>
            <h4 id="donate">Donate</h4>
            <p>
              I made and run this project in my free time, and I don't want to
              collect money for doing that. However <em>running</em> the service
              requires a server, which is not free. I currently pay about 26€
              per month to run this service (that's just the server costs). If
              you are feeling generous, you can help pay for the server costs by
              becoming a monthly sponsor through GitHub Sponsors, or by giving a
              one-time donation:
            </p>
            <div
              className="d-flex flex-row flex-wrap"
              style={{ margin: "-0.25rem" }}
            >
              <a
                href="https://github.com/sponsors/RAnders00"
                target="_blank"
                className="btn btn-primary m-1"
              >
                Become a GitHub sponsor
              </a>
              <a
                href="https://paypal.me/RAnders00"
                target="_blank"
                className="btn btn-outline-primary m-1"
              >
                Donate using PayPal
              </a>
              <a
                href="https://streamelements.com/randers/tip"
                target="_blank"
                className="btn btn-outline-primary m-1"
              >
                Donate using StreamElements
              </a>
            </div>
          </section>
          <section>
            <h4>Rent servers at netcup</h4>
            <p>
              You can also, at no additional cost, use my referral code when
              renting out servers from my hosting provider{" "}
              <strong>netcup</strong>, you save 5€ on your first order, and you
              are giving me a small comission:{" "}
              <a href="https://www.netcup.eu" target="_blank">
                Visit netcup
              </a>
              . Use promo code <code>36nc15963703760</code> on your first order
              to get 5€ off.
            </p>
          </section>
        </>
      ) : null}
    </>
  );
}

export function API() {
  return (
    <>
      <section>
        <h4>General information</h4>
        <p>
          This service exposes an API at the base URL{" "}
          <code>{config.api_base_url}/</code>.
        </p>
        <p>
          CORS is enabled for the entire API, for all origins. You are free to
          use this API from client-side web apps and applications.
        </p>
        <p>
          If you are developing a user-facing application and planning to
          integrate with this service, you must follow these basic guidelines:
          <br />
          If feasible, you should always prefer to get user's consent via a
          opt-in before the integration is enabled for them. On the Opt-in
          dialog, include a short paragraph about what the service does and what
          feature(s) it provides in your application, and give a link to this
          website (<code>https://recent-messages.robotty.de/</code>).{" "}
          <a href="https://imgur.com/a/VjPxPBE">
            Here is a good example that you can follow.
          </a>
        </p>
        <p>
          If it's not realistic/possible to use an opt-in system, you must at
          the very least have an opt-out settings section with the same sort of
          information that would otherwise be shown on the opt-in dialog (info
          text, opt-out toggle, link to website).
        </p>
        <p>
          If in doubt, please contact me about your integration and ask for
          help/confirmation before going forward. Contact details can be found
          on the <Link to="/">home page</Link>.
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
            <code>channel_login</code>: Twitch login name of the channel
            messages should be returned for
          </li>
        </ul>
        <h6>Query parameters:</h6>
        <ul>
          <li>
            <code>?hide_moderation_messages=true/false</code>: Omits{" "}
            <code>CLEARCHAT</code> and <code>CLEARMSG</code> messages from the
            response. Optional, defaults to <code>false</code>.
          </li>
          <li>
            <code>?hide_moderated_messages=true/false</code>: Omits all messages
            from the response that have been deleted by a <code>CLEARCHAT</code>{" "}
            or <code>CLEARMSG</code> message. Optional, defaults to{" "}
            <code>false</code>.
          </li>
          <li>
            <p>
              <code>?clearchat_to_notice=true/false</code>: Converts{" "}
              <code>CLEARCHAT</code> messages into <code>NOTICE</code> messages
              with a user-presentable message.
            </p>
            <p>
              Examples:
              <br />
              <code>
                @historical=1;msg-id=rm-clearchat;rm-received-ts=1596058443362
                :tmi.twitch.tv NOTICE #randers :Chat has been cleared by a
                moderator.
              </code>
              <br />
              <code>
                @historical=1;msg-id=rm-timeout;rm-received-ts=1596058460738
                :tmi.twitch.tv NOTICE #randers :ed0mer has been timed out for
                10m 30s.
              </code>
              <br />
              <code>
                @historical=1;msg-id=rm-permaban;rm-received-ts=1596058421611
                :tmi.twitch.tv NOTICE #pajlada :a_bad_user has been permanently
                banned.
              </code>
              <br />
            </p>
            <p>
              The <code>msg-id</code> will be set to <code>rm-clearchat</code>,{" "}
              <code>rm-timeout</code> or <code>rm-permaban</code>, corresponding
              with the type of message.
            </p>
            <p>
              This option was originally introduced as a "quick fix" to get
              recent-messages integration to work with Chatterino. This option
              is now only retained to keep compatibility with old versions of
              Chatterino. You should try and avoid using this option if
              possible.
            </p>
            <p>
              Optional, defaults to <code>false</code>.
            </p>
          </li>
          <li>
            <code>?limit=n</code>: Limit the number of messages returned. If
            more than <code>n</code> messages are available for the requested
            channel, the response is limited to the <code>n</code> newest
            messages. Optional, defaults to no limit (up to{" "}
            {config.max_buffer_size} messages).{" "}
            <strong>
              It is strongly recommended that you set this to some reasonable
              number based on your application, e.g. 50 or 100 tends to be more
              than enough for general-purpose chat clients.
            </strong>
          </li>
        </ul>
        <h6>Response format:</h6>
        <pre>
          <code>
            {`{
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
}`}
          </code>
        </pre>
        <p>
          Returns up to {config.max_buffer_size} messages. Messages are ordered
          oldest-to-newest. Messages are retured in raw IRC format, without
          trailing newline(s). The API returns <code>PRIVMSG</code>,{" "}
          <code>CLEARCHAT</code>, <code>CLEARMSG</code>, <code>USERNOTICE</code>
          , <code>NOTICE</code> and <code>ROOMSTATE</code> messages.
        </p>
        <p>
          All messages returned carry the <code>historical=1</code> tag.
        </p>
        <p>
          In addition to the IRC tags originally sent by the Twitch IRC server,
          all messages additionally carry the <code>rm-received-ts</code> tag.
          Its format is similar to the <code>tmi-sent-ts</code> tag sent by
          Twitch on some message types (the value is a timestamp, it contains
          the number of milliseconds since Jan 01 1970 00:00:00 UTC, the unix
          epoch). The difference/advantage of the <code>rm-received-ts</code>{" "}
          tag is that it is present on <em>all</em> messages, allowing clients
          to use it like one would use the current clock time when receiving
          normal messages.
        </p>
        <p>
          Messages that were deleted by some moderation action additionally
          carry the <code>rm-deleted=1</code> tag. Note that although typically
          one would only consider <code>PRIVMSG</code> and{" "}
          <code>USERNOTICE</code> messages to be something that "can be
          deleted", the <code>rm-deleted=1</code> can actually be placed on
          every type of message, as a result of a moderator clearing the entire
          chat.
        </p>
        <p>
          If you are going to integrate the returned messages into a chat
          application that also connects to real chat at the same time, it's
          advisable to filter duplicate messages (messages that your chat client
          already received "live" that were also returned by the API). Your best
          bet is to base this on a "weak equality": Check for strict equality
          between the parsed IRC messages, but allow extra tags on the
          recent-messages one. (There is no guarantee that the tags documented
          here might not be extended one day)
        </p>
        <p>
          The <code>error</code> and <code>error_code</code> parameters are
          either both <code>null</code> or both set to some string value.
          <br />
          Despite their name, if <code>error</code> and <code>error_code</code>{" "}
          are not null, it does not signifiy a hard failure of the request. The
          API will still normally return messages.{" "}
          <strong>
            Most applications should therefore ignore the <code>error</code>{" "}
            parameter
          </strong>{" "}
          and also not show it to users. Its presence is purely informational.
        </p>
        <p>
          <code>error</code> is a readable error message, however it is not
          meant to be shown to the user. <code>error_code</code> is a
          machine-readable error code string.
        </p>
        <p>
          Currently, the only valid <code>error_code</code> is{" "}
          <code>channel_not_joined</code>, which signifies that the service is
          currently not listening to messages in that channel. This error can
          arise when recent messages are requested for
          nonexistant/deleted/suspended channels, and it will also be returned
          if this request is the first request for that channel. However there
          are many more combinations of internal events that can cause ta
          channel to currently not be joined, such as a service restart, a
          reconnect, etc.
        </p>
        <h6>Errors</h6>
        If the provided channel is blacklisted from the service (ignored), HTTP
        Status Code 403 is returned with the following body:
        <pre>
          <code>
            {`{
    "status": 403,
    "status_message": "Forbidden",
    "error": "The channel login \`randers\` is excluded from this service",
    "error_code": "channel_ignored"
}`}
          </code>
        </pre>
        If the provided channel name is of invalid format, HTTP Status Code 400
        is returned with the following body:
        <pre>
          <code>
            {`{
    "status": 400,
    "status_message": "Bad Request",
    "error": "Invalid channel login \`this_name_is_way_tooooo_long\`",
    "error_code": "invalid_channel_login"
}`}
          </code>
        </pre>
      </section>
    </>
  );
}

export function DonationThankYou() {
  return (
    <>
      <h1>Thank you for donating</h1>
      <p>
        Thank you very much for donating. Your generosity is greatly appreciated
        and will help keep this project running.
      </p>
      <p>
        <Link to="/">Click here to return to the home page.</Link>
      </p>
    </>
  );
}

export function Privacy() {
  // Remember to update privacy_last_updated_on in config.js when making changes here!
  return (
    <>
      <h1>Privacy Policy</h1>
      <p>
        This privacy policy will explain how I use your personal data I collect
        from you when you use my website and service.
      </p>
      <h4>
        What data do I collect? How do I collect your data? How will I use your
        data? How do I store your data?
      </h4>
      I collect the following data:
      <ul>
        <li>
          Individual chat messages sent to Twitch.tv channels that this service
          is actively listening to.
          <br />
          These chat messages are received from a public Twitch.tv messaging API
          (https://dev.twitch.tv/docs/irc).
          <br />
          The service only collects messages for a channel given that any user
          has requested the messages for that channel within the last{" "}
          {config.channels_expire_after}.<br />
          The service automatically stops listening to the messages sent to a
          Twitch.tv channel within the next 30 minutes once the channel's
          historical messages have not been accessed by anyone for{" "}
          {config.channels_expire_after}. A stored chat message is automatically
          permanently deleted within the next 30 minutes once it becomes older
          than {config.messages_expire_after}.
        </li>
        <li>
          Your basic Twitch.tv profile data (Twitch username, Twitch user ID,
          Link to profile picture).
          <br />
          This information is collected from the public Twitch.tv API
          (https://dev.twitch.tv/docs/api/).
          <br />
          This information is only collected if and when you complete the Login
          feature of the website.
          <br />
          User name and ID are only used to confirm your authorization as the
          owner of a given Twitch.tv channel (authorization).
          <br />
          The link to the profile picture is only used for the website to be
          able to show you your own profile picture. The actual profile picture
          image is not downloaded or stored at any time.
          <br />
          This information is stored on my server. Additionally, the information
          is stored in your browser's local storage to allow you to revisit the
          website without having to log in again. The information is
          automatically permanently deleted from your browser's local storage in
          case you visit the website after not having visited the website for{" "}
          {config.sessions_expire_after}. On the server-side, the information
          automatically becomes expired and inaccessible once{" "}
          {config.sessions_expire_after} have passed without you having visited
          the website with the same browser. Additionally, the information is
          also permanently from both the server as well as the browser's local
          storage if you explicitly "Log out" on the website.
        </li>
      </ul>
      <p>
        I justify my collection of chat messages (first point above) with
        Article 6 Section 1 Point (e) of the GDPR: "Processing shall be lawful
        only if and to the extent that at least one of the following applies:
        [...] (e) processing is necessary for the performance of a task carried
        out in the public interest or in the exercise of official authority
        vested in the controller".
        <br />I justify my collection of basic Twitch.tv user data (second point
        above) because this information is strictly necessary to provide the
        user with the service and to automatically identify them.
      </p>
      <h4>How do I store your data?</h4>
      {config.privacy_how_do_i_store_your_data}
      <h4>What are your data protection rights?</h4>I would like to make sure
      you are fully aware of all of your data protection rights. Every user is
      entitled to the following:
      <ul>
        <li>
          <em>The right to access</em> – You have the right to request me for
          copies of your personal data. I may charge you a small fee for this
          service.
        </li>
        <li>
          <em>The right to rectification</em> – You have the right to request
          that I correct any information you believe is inaccurate. You also
          have the right to request me to complete the information you believe
          is incomplete.
        </li>
        <li>
          <em>The right to erasure</em> – You have the right to request that I
          erase your personal data, under certain conditions.
        </li>
        <li>
          <em>The right to restrict processing</em> – You have the right to
          request that I restrict the processing of your personal data, under
          certain conditions.
        </li>
        <li>
          <em>The right to object to processing</em> – You have the right to
          object to my processing of your personal data, under certain
          conditions.
        </li>
        <li>
          <em>The right to data portability</em> – You have the right to request
          that I transfer the data that I have collected to another
          organization, or directly to you, under certain conditions.
        </li>
      </ul>
      <p>
        If you make a request, I have one month to respond to you. If you would
        like to exercise any of these rights, please contact me at my email:{" "}
        {rot13(config.privacy_contact_email_rot13)}
      </p>
      <p>
        Regardless of these rights, this service offers an automated opt-out
        service that allows you to delete the historical messages for your own
        Twitch.tv channel(s), as well as to blacklist your channel so that the
        service will not collect messages from it anymore. Please visit the{" "}
        <Link to="/settings">Settings</Link> page linked above to use this
        feature.
      </p>
      <h4>Privacy policies of other websites</h4>
      This website contains links to other websites. My privacy policy applies
      only to my website, so if you click on a link to another website, you
      should read their privacy policy.
      <h4>Changes to our privacy policy</h4>I keep my privacy policy under
      regular review and place any updates on this web page. This privacy policy
      was last updated on {config.privacy_last_updated_on}.
      <h4>How to contact me</h4>
      If you have any questions about my privacy policy, the data I hold on you,
      or you would like to exercise one of your data protection rights, please
      do not hesitate to contact me. Email me at:{" "}
      {rot13(config.privacy_contact_email_rot13)}
    </>
  );
}
