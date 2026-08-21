-- Email notification opt-out flag (issue #73).
-- Users are "subscribed" to a post by authoring or commenting on it;
-- this column lets them opt out of all email notifications.
ALTER TABLE users ADD COLUMN notifications_opt_out INTEGER NOT NULL DEFAULT 0;
