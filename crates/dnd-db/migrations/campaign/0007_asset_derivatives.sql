-- ADR-0017: imported images get derived WebP files. `usage` decides limits and tiling; maps are
-- drawn from 2048² tiles (JSON list of {x, y, width, height, sha256}), tokens from one display
-- image. Files live in `assets/<sha256>.webp`; the original upload in `assets/originals/`.
ALTER TABLE assets ADD COLUMN usage TEXT NOT NULL DEFAULT 'map' CHECK (usage IN ('map', 'token'));
ALTER TABLE assets ADD COLUMN display_sha256 TEXT
    CHECK (display_sha256 IS NULL OR length(display_sha256) = 64);
ALTER TABLE assets ADD COLUMN tiles TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(tiles));
