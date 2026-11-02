CREATE TABLE plugin_runtime (id INTEGER PRIMARY KEY CHECK (id = 1), dto TEXT NOT NULL CHECK (json_valid(dto))) STRICT;
