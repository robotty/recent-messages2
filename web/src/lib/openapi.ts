import config from "../../config";

export function getOpenApiSpec() {
  const maxBufferSize = Number.parseInt(config.max_buffer_size, 10) || 800;

  return {
    openapi: "3.1.0",
    info: {
      title: "recent-messages2 API",
      version: "2.0.0",
      description:
        "HTTP API for retrieving recently sent Twitch IRC messages and managing channel opt-out/purge settings.",
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
      {
        name: "Channel Settings",
        description:
          "Authenticated endpoints for channel owners to blacklist their channel or purge stored messages.",
      },
      {
        name: "Authentication",
        description:
          "Session endpoints backed by Twitch OAuth authorization codes.",
      },
    ],
    components: {
      securitySchemes: {
        bearerAuth: {
          type: "http",
          scheme: "bearer",
          description:
            "Session access token returned by POST /auth/create or POST /auth/extend.",
        },
      },
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
        GetIgnoredResponse: {
          type: "object",
          required: ["ignored"],
          properties: {
            ignored: {
              type: "boolean",
              description:
                "Whether the authenticated channel is currently blacklisted from the service.",
            },
          },
        },
        SetIgnoredBody: {
          type: "object",
          required: ["ignored"],
          properties: {
            ignored: {
              type: "boolean",
              description:
                "Set true to blacklist the channel and purge its messages, or false to re-enable collection.",
            },
          },
        },
        UserAuthorizationResponse: {
          type: "object",
          required: [
            "access_token",
            "valid_until",
            "user_id",
            "user_login",
            "user_name",
            "user_profile_image_url",
            "user_details_valid_until",
          ],
          properties: {
            access_token: { type: "string" },
            valid_until: { type: "string", format: "date-time" },
            user_id: { type: "string" },
            user_login: { type: "string" },
            user_name: { type: "string" },
            user_profile_image_url: { type: "string", format: "uri" },
            user_details_valid_until: { type: "string", format: "date-time" },
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
      "/ignored": {
        get: {
          tags: ["Channel Settings"],
          summary: "Get channel blacklist status",
          security: [{ bearerAuth: [] }],
          responses: {
            "200": {
              description: "Current blacklist status for the user's channel.",
              content: {
                "application/json": {
                  schema: { $ref: "#/components/schemas/GetIgnoredResponse" },
                },
              },
            },
            "401": {
              description: "Missing or invalid authorization token.",
              content: {
                "application/json": {
                  schema: { $ref: "#/components/schemas/ApiErrorResponse" },
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
        post: {
          tags: ["Channel Settings"],
          summary: "Set channel blacklist status",
          description:
            "Enables or disables message collection for the authenticated user's channel. Enabling also purges stored messages.",
          security: [{ bearerAuth: [] }],
          requestBody: {
            required: true,
            content: {
              "application/json": {
                schema: { $ref: "#/components/schemas/SetIgnoredBody" },
              },
            },
          },
          responses: {
            "204": { description: "Channel blacklist status updated." },
            "400": {
              description: "Invalid JSON payload.",
              content: {
                "application/json": {
                  schema: { $ref: "#/components/schemas/ApiErrorResponse" },
                },
              },
            },
            "401": {
              description: "Missing or invalid authorization token.",
              content: {
                "application/json": {
                  schema: { $ref: "#/components/schemas/ApiErrorResponse" },
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
      "/purge": {
        post: {
          tags: ["Channel Settings"],
          summary: "Purge channel messages",
          description:
            "Deletes all messages currently stored for the authenticated user's channel.",
          security: [{ bearerAuth: [] }],
          responses: {
            "204": { description: "Channel messages purged." },
            "401": {
              description: "Missing or invalid authorization token.",
              content: {
                "application/json": {
                  schema: { $ref: "#/components/schemas/ApiErrorResponse" },
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
      "/auth/create": {
        post: {
          tags: ["Authentication"],
          summary: "Create session from Twitch OAuth code",
          parameters: [
            {
              name: "code",
              in: "query",
              required: true,
              description: "Authorization code returned by Twitch OAuth2.",
              schema: { type: "string" },
            },
          ],
          responses: {
            "200": {
              description: "Session created.",
              content: {
                "application/json": {
                  schema: {
                    $ref: "#/components/schemas/UserAuthorizationResponse",
                  },
                },
              },
            },
            "400": {
              description: "Invalid or missing authorization code.",
              content: {
                "application/json": {
                  schema: { $ref: "#/components/schemas/ApiErrorResponse" },
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
      "/auth/extend": {
        post: {
          tags: ["Authentication"],
          summary: "Extend and revalidate current session",
          security: [{ bearerAuth: [] }],
          responses: {
            "200": {
              description: "Session extended.",
              content: {
                "application/json": {
                  schema: {
                    $ref: "#/components/schemas/UserAuthorizationResponse",
                  },
                },
              },
            },
            "401": {
              description: "Missing or invalid authorization token.",
              content: {
                "application/json": {
                  schema: { $ref: "#/components/schemas/ApiErrorResponse" },
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
      "/auth/revoke": {
        post: {
          tags: ["Authentication"],
          summary: "Revoke current session",
          security: [{ bearerAuth: [] }],
          responses: {
            "204": { description: "Session revoked." },
            "401": {
              description: "Missing or invalid authorization token.",
              content: {
                "application/json": {
                  schema: { $ref: "#/components/schemas/ApiErrorResponse" },
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
