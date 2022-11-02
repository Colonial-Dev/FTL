-- Records the metadata (and potentially contents) of all files ever checked into FTL.
-- This table is never "cleaned up" unless it is explicitly requested.
--
-- Many other parts of the database are FOREIGN KEYed to this table's ID column,
-- so deleting a row from here will trigger cascades through the entire system.
CREATE TABLE input_files (
    -- The file's ID, generated by hashing the concatenation of
    -- its path and data hash.
    id TEXT PRIMARY KEY,
    -- The site-source relative path to the file's original location.
    path TEXT,
    -- The data hash of the file's contents.
    hash TEXT,
    -- The file's extension, if any. Preceding dot excluded.
    extension TEXT,
    -- The file's contents, if inline is true.
    contents TEXT,
    -- Whether or not the file's contents are stored inline.
    -- Only textual files are inlined; binary blobs (such as images)
    -- are instead copied to the flat-file cache.
    inline INTEGER
);

-- Records metadata about site revisions.
-- A new revision is computed for each unique set of input data.
--
-- Much like input_files, significant parts of the database are 
-- cascade FOREIGN KEYd to this table's ID column.
CREATE TABLE revisions (
    -- The revision's ID, generated by hashing the hashes of
    -- all input files together.
    id TEXT PRIMARY KEY,
    -- The user-assigned name, if any.
    -- Defaults to NULL.
    name TEXT UNIQUE,
    -- The timestamp at which the revision was stabilized.
    timestamp TEXT,
    -- Boolean - whether or not the revision is "pinned."
    -- Pinned revisions and their dependencies are excluded
    -- when performing database cleanup.
    pinned INTEGER,
    -- Boolean - whether or not the revision is "stable."
    -- New revisions start as "unstable." Once a revision's output is 
    -- successfully evaluated, it becomes stable.
    -- 
    -- Revisions that do not successfully produce output are not stabilized,
    -- and will be discarded at next build time.
    stable INTEGER
);

-- Records one-to-many relationships between revisions and input files.
CREATE TABLE revision_files (
    revision TEXT,
    id TEXT,

    FOREIGN KEY (id)
    REFERENCES input_files (id)
        ON UPDATE CASCADE
        ON DELETE CASCADE,

    FOREIGN KEY (revision)
    REFERENCES revisions (id)
        ON UPDATE CASCADE
        ON DELETE CASCADE,

    UNIQUE (id, revision)
);

CREATE INDEX idx_rev_files_revision ON revision_files(revision);

-- Records information specific to "pages" - that is, Markdown documents.
-- This largely consists of TOML frontmatter information.
CREATE TABLE pages (
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

    FOREIGN KEY (id)
    REFERENCES input_files (id)
        ON UPDATE CASCADE
        ON DELETE CASCADE
);

CREATE TABLE page_attributes (
    id TEXT PRIMARY KEY,
    tag TEXT,
    alias TEXT,

    FOREIGN KEY (id)
    REFERENCES pages (id)
        ON UPDATE CASCADE
        ON DELETE CASCADE
);

CREATE TABLE routes (
    id TEXT PRIMARY KEY,
    revision TEXT,
    route TEXT,
    parent_route TEXT,
    kind INTEGER,

    FOREIGN KEY (id)
    REFERENCES input_files (id)
        ON UPDATE CASCADE
        ON DELETE CASCADE,

    FOREIGN KEY (revision)
    REFERENCES revisions (id)
        ON UPDATE CASCADE
        ON DELETE CASCADE,

    UNIQUE(id, revision)
);

CREATE TABLE templates (
    name TEXT PRIMARY KEY,
    id TEXT
);

CREATE TABLE dependencies (
    page_id TEXT,
    asset_id TEXT,

    FOREIGN KEY (page_id)
    REFERENCES pages (id)
        ON UPDATE CASCADE
        ON DELETE CASCADE,

    FOREIGN KEY (asset_id)
    REFERENCES input_files (id)
        ON UPDATE CASCADE
        ON DELETE CASCADE,

    UNIQUE(page_id, asset_id)
);

CREATE TABLE output (
    id TEXT PRIMARY KEY,
    revision TEXT,
    kind INTEGER,
    content TEXT,

    FOREIGN KEY (id)
    REFERENCES input_files (id)
        ON UPDATE CASCADE
        ON DELETE CASCADE
    
    FOREIGN KEY (revision)
    REFERENCES revisions (id)
        ON UPDATE CASCADE
        ON DELETE CASCADE
);