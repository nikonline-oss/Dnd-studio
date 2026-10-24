-- ADR-0023: plugins live inside the campaign. The whole description the core works with (name,
-- licence, the rules of a system plugin with its sheet schema and layout) is kept as JSON; the
-- package itself stays in the campaign folder under `assets/plugins/<archive_sha256>.dndplugin`.
-- The campaign's system plugin is the meta key `campaign.system_plugin`.
ALTER TABLE installed_plugins ADD COLUMN dto TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(dto));
