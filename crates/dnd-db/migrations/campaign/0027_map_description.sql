-- Maps carry a free-text description the whole table can read in the inspector.
-- Maps created before the field existed get an empty description.

ALTER TABLE maps ADD COLUMN description TEXT NOT NULL DEFAULT '';
