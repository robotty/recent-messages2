import config from "../../config";

export function getOpenApiSpec() {
  const maxBufferSize = Number.parseInt(config.max_buffer_size, 10) || 800;

  return {
    openapi: "3.1.0",
    info: {
      title: "recent-messages2 API",
      version: "2.0.0",
      description: "HTTP API for retrieving recently sent Twitch IRC messages.",
      license: {
        name: "GNU AGPL v3.0 or later",
        url: `${config.repo_url}/blob/master/LICENSE`,
      },
    },
    servers: [
      {
        url: config.api_base_url,
        description: "Configured API v2 base URL",
      },
    ],
    tags: [
      {
        name: "Messages",
        description:
          "Public endpoint for fetching recent channel chat messages.",
      },
    ],
    components: {
      schemas: {
        GetRecentMessagesResponse: {
          type: "object",
          required: ["messages", "error", "error_code"],
          properties: {
            messages: {
              type: "array",
              items: { type: "string" },
              description:
                "Raw IRCv3 messages ordered oldest-to-newest, each carrying historical=1 and rm-received-ts.",
            },
            error: {
              type: ["string", "null"],
              description:
                "Informational status message (not necessarily a hard failure; most clients should ignore this).",
            },
            error_code: {
              type: ["string", "null"],
              enum: ["channel_not_joined", null],
              description:
                "Machine-readable status code (e.g. channel_not_joined).",
            },
          },
          example: {
            messages: [
              "@badge-info=;badges=glhf-pledge/1;color=;display-name=purplereddish;emotes=;flags=;historical=1;id=dbb10be8-581e-4f22-ba12-2001e088529d;mod=0;rm-received-ts=1596061057185;room-id=71092938;subscriber=0;tmi-sent-ts=1596061056790;turbo=0;user-id=260969632;user-type= :purplereddish!purplereddish@purplereddish.tmi.twitch.tv PRIVMSG #xqcow LULW",
            ],
            error: null,
            error_code: null,
          },
        },
        ApiErrorResponse: {
          type: "object",
          required: ["status", "status_message", "error", "error_code"],
          properties: {
            status: { type: "integer", example: 400 },
            status_message: { type: "string", example: "Bad Request" },
            error: {
              type: "string",
              example:
                "Invalid channel login: Channel login must not be longer than 25 characters",
            },
            error_code: {
              type: "string",
              example: "invalid_channel_login",
            },
          },
        },
      },
    },
    paths: {
      "/recent-messages/{channel_login}": {
        get: {
          tags: ["Messages"],
          summary: "Get recent messages",
          description: `Returns up to ${config.max_buffer_size} recent raw IRCv3 messages for a Twitch channel, ordered oldest-to-newest.`,
          parameters: [
            {
              name: "channel_login",
              in: "path",
              required: true,
              description:
                "Twitch login name of the channel messages should be returned for.",
              schema: { type: "string" },
              example: "pajlada",
            },
            {
              name: "hide_moderation_messages",
              in: "query",
              required: false,
              description:
                "Omits CLEARCHAT and CLEARMSG messages from the response.",
              schema: { type: "boolean", default: false },
            },
            {
              name: "hide_moderated_messages",
              in: "query",
              required: false,
              description:
                "Omits all messages deleted by a CLEARCHAT or CLEARMSG moderation action.",
              schema: { type: "boolean", default: false },
            },
            {
              name: "clearchat_to_notice",
              in: "query",
              required: false,
              deprecated: true,
              description:
                "Legacy option converting CLEARCHAT messages into user-presentable NOTICE messages.",
              schema: { type: "boolean", default: false },
            },
            {
              name: "limit",
              in: "query",
              required: false,
              description:
                "Limit the number of newest messages returned (recommended: e.g. 50 or 100).",
              schema: {
                type: "integer",
                minimum: 0,
                maximum: maxBufferSize,
              },
              example: 50,
            },
            {
              name: "before",
              in: "query",
              required: false,
              description:
                "Only return messages received before (<) this unix timestamp in milliseconds (rm-received-ts).",
              schema: { type: "integer", format: "int64" },
            },
            {
              name: "after",
              in: "query",
              required: false,
              description:
                "Only return messages received after (>) this unix timestamp in milliseconds (rm-received-ts).",
              schema: { type: "integer", format: "int64" },
            },
          ],
          responses: {
            "200": {
              description: "Recent messages retrieved successfully.",
              content: {
                "application/json": {
                  schema: {
                    $ref: "#/components/schemas/GetRecentMessagesResponse",
                  },
                },
              },
            },
            "400": {
              description: "Invalid channel login or query parameters.",
              content: {
                "application/json": {
                  schema: { $ref: "#/components/schemas/ApiErrorResponse" },
                },
              },
            },
            "403": {
              description: "Channel is blacklisted (channel_ignored).",
              content: {
                "application/json": {
                  schema: { $ref: "#/components/schemas/ApiErrorResponse" },
                  example: {
                    status: 403,
                    status_message: "Forbidden",
                    error:
                      "The channel login `randers` is excluded from this service",
                    error_code: "channel_ignored",
                  },
                },
              },
            },
            "500": {
              description: "Internal server error.",
              content: {
                "application/json": {
                  schema: { $ref: "#/components/schemas/ApiErrorResponse" },
                },
              },
            },
          },
        },
      },
    },
  };
}
