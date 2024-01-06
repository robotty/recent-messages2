# Changelog

# Unreleased

- Breaking: Removed `recentmessages_get_recent_messages_endpoint_async_components_seconds` metric,
  has been renamed to the almost identical `recentmessages_get_recent_messages_endpoint_components_seconds`.
  This new metric now also observes the `export_stored_messages` component, which is not async, hence the rename.
  (#287)
- Added: `?before=` and `?after=` parameters to `/api/v2/recent-messages/:channel_login`. (#287)
- Fixed: Internal errors occurring in web request handlers are now printed. (#287)
- Fixed: Metric for performance of different endpoints now works again, was previously only showing one combined
  performance for all endpoints under endpoint "other". (#287)

# v0.1.0

Initial Release.
