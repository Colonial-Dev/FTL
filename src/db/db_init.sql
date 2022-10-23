CREATE TABLE IF NOT EXISTS input_files (
    id TEXT PRIMARY KEY,
    path TEXT,
    hash TEXT,
    extension TEXT,
    contents TEXT,
    inline INTEGER,
    UNIQUE(id)
);

CREATE TABLE IF NOT EXISTS revision_files (
    revision TEXT,
    id TEXT,
    UNIQUE(revision, id)
);

CREATE TABLE IF NOT EXISTS pages (
    id TEXT PRIMARY KEY,
    path TEXT,
    route TEXT,
    offset INTEGER,
    title TEXT,
    date TEXT,
    publish_date TEXT,
    expire_date TEXT,
    description TEXT,
    summary TEXT,
    template TEXT,
    draft INTEGER,
    dynamic INTEGER,
    tags TEXT,
    collections TEXT,
    aliases TEXT,
    UNIQUE(id)
);

CREATE TABLE IF NOT EXISTS page_attributes (
    kind INTEGER,
    page_id TEXT,
    tag TEXT,
    alias TEXT
);

CREATE TABLE IF NOT EXISTS routes (
    revision TEXT,
    id TEXT,
    route TEXT,
    parent_route TEXT,
    kind INTEGER,
    UNIQUE(
        revision,
        id,
        route,
        parent_route,
        kind
    )
);

CREATE TABLE IF NOT EXISTS templates (
    name TEXT,
    id TEXT,
    UNIQUE(name, id)
);

CREATE TABLE IF NOT EXISTS dependencies (
    page_id TEXT,
    asset_id TEXT,
    UNIQUE(page_id, asset_id)
);

CREATE TABLE IF NOT EXISTS stylesheets (
    revision TEXT,
    content TEXT,
    UNIQUE (revision, content)
);

CREATE TABLE IF NOT EXISTS hypertext (
    id TEXT,
    revision TEXT,
    content TEXT,
    UNIQUE(
        id,
        revision,
        content
    )
);