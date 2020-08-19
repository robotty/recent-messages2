module.exports = {
  client_id: "nld8y6rt7f5u7l1xuq4eni8pzp8mjo",
  redirect_uri: "https://recent-messages.robotty.de/authorized",
  // human readable strings for the home page and API documentation
  messages_expire_after: "24 hours",
  channels_expire_after: "24 hours",
  max_buffer_size: "800",
  github_link: "https://github.com/robotty/recent-messages2",
  // used for both the documentation as well as the actual API calls made by the web app. Don't include a trailing slash
  api_base_url: "https://recent-messages.robotty.de/api/v2",
};

if (process.env.NODE_ENV === "development") {
  module.exports = {
    ...module.exports,
    client_id: "iwucnx8zmbzucoan8ga33j355lb7nc",
    redirect_uri: "http://localhost:1234/authorized",
    api_base_url: "http://127.0.0.1:2790/api/v2",
  };
}
