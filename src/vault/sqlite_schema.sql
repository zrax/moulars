CREATE TABLE IF NOT EXISTS accounts
(
    idx             INTEGER PRIMARY KEY         AUTOINCREMENT,
    account_name    TEXT    NOT NULL    UNIQUE  COLLATE NOCASE,
    pass_hash       TEXT    NOT NULL,
    account_id      BLOB    NOT NULL    UNIQUE,
    account_flags   INTEGER NOT NULL            DEFAULT 0,
    billing_type    INTEGER NOT NULL            DEFAULT 0
);

CREATE TABLE IF NOT EXISTS api_tokens
(
    idx             INTEGER PRIMARY KEY         AUTOINCREMENT,
    account_id      BLOB    NOT NULL,
    token           TEXT    NOT NULL    UNIQUE  COLLATE NOCASE,
    comment         TEXT    NOT NULL
);

CREATE TABLE IF NOT EXISTS scores
(
    idx             INTEGER PRIMARY KEY     AUTOINCREMENT,
    create_time     INTEGER NOT NULL        DEFAULT 0,
    owner_idx       INTEGER NOT NULL,
    type            INTEGER NOT NULL        DEFAULT 1,
    name            TEXT    NOT NULL,
    points          INTEGER NOT NULL        DEFAULT 0
);

CREATE TABLE IF NOT EXISTS global_states
(
    idx             INTEGER PRIMARY KEY     AUTOINCREMENT,
    descriptor      TEXT    NOT NULL,
    sdl_blob        BLOB    NOT NULL
);

CREATE TABLE IF NOT EXISTS node_refs
(
    idx             INTEGER PRIMARY KEY     AUTOINCREMENT,
    parent_idx      INTEGER NOT NULL,
    child_idx       INTEGER NOT NULL,
    owner_idx       INTEGER NOT NULL        DEFAULT 0
);

CREATE TABLE IF NOT EXISTS nodes
(
    idx             INTEGER PRIMARY KEY     AUTOINCREMENT,
    create_time     INTEGER NOT NULL        DEFAULT 0,
    modify_time     INTEGER NOT NULL        DEFAULT 0,
    create_age_name TEXT,
    create_age_uuid BLOB,
    creator_uuid    BLOB    NOT NULL        DEFAULT X'00000000000000000000000000000000',
    creator_idx     INTEGER NOT NULL        DEFAULT 0,
    node_type       INTEGER NOT NULL,
    int32_1         INTEGER,
    int32_2         INTEGER,
    int32_3         INTEGER,
    int32_4         INTEGER,
    uint32_1        INTEGER,
    uint32_2        INTEGER,
    uint32_3        INTEGER,
    uint32_4        INTEGER,
    uuid_1          BLOB,
    uuid_2          BLOB,
    uuid_3          BLOB,
    uuid_4          BLOB,
    string64_1      TEXT,
    string64_2      TEXT,
    string64_3      TEXT,
    string64_4      TEXT,
    string64_5      TEXT,
    string64_6      TEXT,
    istring64_1     TEXT                    COLLATE NOCASE,
    istring64_2     TEXT                    COLLATE NOCASE,
    text_1          TEXT,
    text_2          TEXT,
    blob_1          BLOB,
    blob_2          BLOB
);
INSERT OR IGNORE INTO sqlite_sequence (seq, name) VALUES (10000, 'nodes');

CREATE TABLE IF NOT EXISTS servers
(
    idx             INTEGER PRIMARY KEY     AUTOINCREMENT,
    instance_uuid   BLOB    NOT NULL,
    age_filename    TEXT    NOT NULL,
    display_name    TEXT    NOT NULL,
    age_idx         INTEGER NOT NULL,
    sdl_idx         INTEGER NOT NULL,
    temporary       INTEGER NOT NULL        DEFAULT 0
);

CREATE TABLE IF NOT EXISTS age_states
(
    idx             INTEGER PRIMARY KEY     AUTOINCREMENT,
    server_idx      INTEGER NOT NULL,
    descriptor      TEXT    NOT NULL,
    object_key      TEXT    NOT NULL,
    sdl_blob        BLOB    NOT NULL
);
