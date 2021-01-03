module.exports = {
  client_id: "nld8y6rt7f5u7l1xuq4eni8pzp8mjo",
  redirect_uri: "https://recent-messages.robotty.de/authorized",
  // human readable strings for the home page and API documentation
  messages_expire_after: "24 hours",
  channels_expire_after: "24 hours",
  sessions_expire_after: "7 days",
  max_buffer_size: "800",
  github_link: "https://github.com/robotty/recent-messages2",
  // used for both the documentation as well as the actual API calls made by the web app. Don't include a trailing slash
  api_base_url: "https://recent-messages.robotty.de/api/v2",

  service_owner_info:
    "The recent-messages service is created and run by me, randers.",
  issues_url: "https://github.com/robotty/recent-messages2/issues",
  general_contact_email_rot13: "ehora.naqref@ebobggl.qr",
  // don't include a trailing slash
  repo_url: "https://github.com/robotty/recent-messages2",
  enable_donation_section: true,

  // Remember to update privacy_last_updated_on when updating these!
  privacy_how_do_i_store_your_data:
    "The collected data described above is securely stored at a server hosted " +
    "in Nuremberg (Nürnberg), Germany at my hosting provider netcup GmbH " +
    "(netcup GmbH, Daimlerstraße 25, 76185 Karlsruhe, Germany, Tel. " +
    "+4972175407550, E-Mail mail@netcup.de - " +
    "https://www.netcup.eu/kontakt/impressum.php). Netcup employs the necessary " +
    "security measures to ensure all data is kept safe. Netcup's data center is " +
    "subject to physical access control, 24/7 video surveillance and " +
    "supervision by an independent security company " +
    "(https://www.netcup.eu/ueber-netcup/rechenzentrum.php).",
  privacy_contact_email_rot13: "ehora.naqref@ebobggl.qr",
  privacy_last_updated_on: "13 December 2020",
};

if (process.env.NODE_ENV === "development") {
  module.exports = {
    ...module.exports,
    client_id: "iwucnx8zmbzucoan8ga33j355lb7nc",
    redirect_uri: "http://localhost:1234/authorized",
    api_base_url: "http://127.0.0.1:2790/api/v2",
  };
}
