/* This file is part of moulars.
 *
 * moulars is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * moulars is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with moulars.  If not, see <http://www.gnu.org/licenses/>.
 */

use std::str::FromStr;
use std::sync::Arc;
use std::time::SystemTime;

use anyhow::Result;
use async_trait::async_trait;
use sqlx::{FromRow, Row, SqlitePool, QueryBuilder};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions, SqliteRow};
use tracing::{warn, info, debug};
use uuid::Uuid;

use crate::auth_srv::auth_hash::create_pass_hash;
use crate::hashes::ShaDigest;
use crate::netcli::{NetResult, NetResultCode};
use crate::vault::NodeRef;
use crate::vault::vault_node::{VaultNode, StandardNode, NodeType};
use super::db_interface::{DbInterface, AccountInfo, PlayerInfo, GameServer};

pub struct DbSqlite {
    pool: SqlitePool,
    volatile: bool,
}

impl DbSqlite {
    const SCHEMA_VERSION: i32 = 1;

    pub async fn new(db_uri: &str) -> Result<Self> {
        // Sqlite's :memory: URI can be specified a few different ways...
        let db = if db_uri.contains(":memory:") {
            // Ensure this pool uses a single persistent connection.
            let pool = SqlitePoolOptions::new()
                            .max_connections(1)
                            .idle_timeout(None)
                            .max_lifetime(None)
                            .connect(db_uri).await?;
            Self { pool, volatile: true }
        } else {
            let options = SqliteConnectOptions::from_str(db_uri)?
                            .create_if_missing(true);
            let pool = SqlitePoolOptions::new()
                            .max_connections(4)
                            .connect_with(options).await?;
            Self { pool, volatile: false }
        };

        db.init_schema().await?;
        Ok(db)
    }

    async fn init_schema(&self) -> Result<()> {
        let _ = sqlx::raw_sql("CREATE TABLE IF NOT EXISTS meta ( \
                                   name    TEXT NOT NULL UNIQUE, \
                                   value   TEXT NOT NULL)")
            .execute(&self.pool).await?;

        let schema_version: Option<i32> =
            sqlx::query("SELECT value FROM meta WHERE name = 'schema_version'")
                .fetch_optional(&self.pool).await?
                .map(|row| {
                    let version_str: String = row.get(0);
                    version_str.parse::<i32>()
                }).transpose()?;

        match schema_version {
            Some(Self::SCHEMA_VERSION) => Ok(()),
            None => {
                // Initialize the current schema
                debug!("Initializing database schema...");
                let _ = sqlx::raw_sql(include_str!("sqlite_schema.sql"))
                    .execute(&self.pool).await?;
                let _ = sqlx::query("INSERT INTO meta (name, value) VALUES ('schema_version', $1)")
                    .bind(Self::SCHEMA_VERSION)
                    .execute(&self.pool).await?;
                Ok(())
            }
            Some(version) => {
                Err(anyhow::anyhow!("Unexpected database schema version {version}!"))
            }
        }
    }
}

macro_rules! iter_node {
    ($node:ident, $closure:expr) => {
        if $node.has_create_time() {
            $closure("create_time", $node.create_time());
        }
        if $node.has_modify_time() {
            $closure("modify_time", $node.modify_time());
        }
        if $node.has_create_age_name() {
            $closure("create_age_name", $node.create_age_name());
        }
        if $node.has_create_age_uuid() {
            $closure("create_age_uuid", $node.create_age_uuid());
        }
        if $node.has_creator_uuid() {
            $closure("creator_uuid", $node.creator_uuid());
        }
        if $node.has_creator_id() {
            $closure("creator_idx", $node.creator_id());
        }
        if $node.has_node_type() {
            $closure("node_type", $node.node_type());
        }
        if $node.has_int32_1() {
            $closure("int32_1", $node.int32_1());
        }
        if $node.has_int32_2() {
            $closure("int32_2", $node.int32_2());
        }
        if $node.has_int32_3() {
            $closure("int32_3", $node.int32_3());
        }
        if $node.has_int32_4() {
            $closure("int32_4", $node.int32_4());
        }
        if $node.has_uint32_1() {
            $closure("uint32_1", $node.uint32_1());
        }
        if $node.has_uint32_2() {
            $closure("uint32_2", $node.uint32_2());
        }
        if $node.has_uint32_3() {
            $closure("uint32_3", $node.uint32_3());
        }
        if $node.has_uint32_4() {
            $closure("uint32_4", $node.uint32_4());
        }
        if $node.has_uuid_1() {
            $closure("uuid_1", $node.uuid_1());
        }
        if $node.has_uuid_2() {
            $closure("uuid_2", $node.uuid_2());
        }
        if $node.has_uuid_3() {
            $closure("uuid_3", $node.uuid_3());
        }
        if $node.has_uuid_4() {
            $closure("uuid_4", $node.uuid_4());
        }
        if $node.has_string64_1() {
            $closure("string64_1", $node.string64_1());
        }
        if $node.has_string64_2() {
            $closure("string64_2", $node.string64_2());
        }
        if $node.has_string64_3() {
            $closure("string64_3", $node.string64_3());
        }
        if $node.has_string64_4() {
            $closure("string64_4", $node.string64_4());
        }
        if $node.has_string64_5() {
            $closure("string64_5", $node.string64_5());
        }
        if $node.has_string64_6() {
            $closure("string64_6", $node.string64_6());
        }
        if $node.has_istring64_1() {
            $closure("istring64_1", $node.istring64_1());
        }
        if $node.has_istring64_2() {
            $closure("istring64_2", $node.istring64_2());
        }
        if $node.has_text_1() {
            $closure("text_1", $node.text_1());
        }
        if $node.has_text_2() {
            $closure("text_2", $node.text_2());
        }
        if $node.has_blob_1() {
            $closure("blob_1", $node.blob_1());
        }
        if $node.has_blob_2() {
            $closure("blob_2", $node.blob_2());
        }
    };
}

#[async_trait]
impl DbInterface for DbSqlite {
    async fn get_account(&self, account_name: &str) -> NetResult<Option<AccountInfo>> {
        let account: Option<AccountInfo> =
            sqlx::query_as("SELECT account_name, pass_hash, account_id, account_flags, billing_type \
                                FROM accounts WHERE account_name = $1")
                .bind(account_name)
                .fetch_optional(&self.pool).await
                .map_err(|err| {
                    warn!("Failed to fetch account: {err}");
                    NetResultCode::NetAccountNotFound
                })?;

        if account.is_none() && self.volatile {
            // When using a volatile database, any account login attempt
            // automatically creates a new ADMIN account with a blank password
            // and an associated API token.  This greatly simplifies testing
            // and development.
            let pass_hash = create_pass_hash(account_name, "").map_err(|err| {
                                warn!("Failed to create password hash: {err}");
                                NetResultCode::NetInternalError
                            })?;
            let new_account = self.create_account(account_name, pass_hash,
                                                  AccountInfo::ADMIN).await?;
            if let Ok(api_token) = self.create_api_token(&new_account.account_id,
                                                         "Autogenerated").await {
                info!("API token for '{account_name}' is {api_token}");
            }
            Ok(Some(new_account))
        } else {
            Ok(account)
        }
    }

    async fn get_account_for_token(&self, api_token: &str) -> NetResult<Option<AccountInfo>> {
        let account: Option<AccountInfo> =
            sqlx::query_as("SELECT account_name, pass_hash, account_id, account_flags, billing_type \
                                FROM accounts \
                                WHERE account_id = (SELECT account_id FROM api_tokens WHERE token = $1)")
                .bind(api_token)
                .fetch_optional(&self.pool).await
                .map_err(|err| {
                    warn!("Failed to fetch account: {err}");
                    NetResultCode::NetAccountNotFound
                })?;
        if let Some(account) = &account {
            debug!("Matched {} account {} for API token {api_token}",
                if account.is_admin() { "ADMIN" } else { "normal" },
                account.account_name);
        } else {
            debug!("No matching account found for API token {api_token}");
        }
        Ok(account)
    }

    async fn create_account(&self, account_name: &str, pass_hash: ShaDigest,
                            account_flags: u32) -> NetResult<AccountInfo> {
        let account: Option<String> =
            sqlx::query("SELECT account_id FROM accounts WHERE account_name = $1")
                .bind(account_name)
                .fetch_optional(&self.pool).await
                .map_err(|err| {
                    warn!("Failed to fetch account: {err}");
                    NetResultCode::NetInternalError
                })?
                .map(|row| row.get(0));
        if let Some(account_id) = account {
            warn!("Account with name '{account_name}' already exists: {account_id}");
            return Err(NetResultCode::NetAccountAlreadyExists);
        }

        let account_id = Uuid::new_v4();
        let billing_type = 1;
        let _ = sqlx::query("INSERT INTO accounts (account_name, pass_hash, account_id, \
                                                   account_flags, billing_type) \
                                    VALUES ($1, $2, $3, $4, $5)")
            .bind(account_name)
            .bind(pass_hash.as_hex())
            .bind(account_id)
            .bind(account_flags)
            .bind(billing_type)
            .execute(&self.pool).await
            .map_err(|err| {
                warn!("Failed to create account: {err}");
                NetResultCode::NetInternalError
            })?;

        info!("Created account '{account_name}': {account_id}");
        Ok(AccountInfo {
            account_name: account_name.to_owned(),
            pass_hash,
            account_id,
            account_flags,
            billing_type,
        })
    }

    async fn create_api_token(&self, account_id: &Uuid, comment: &str) -> NetResult<String> {
        let api_token = format!("{}{}", Uuid::now_v7(), Uuid::new_v4());
        let _ = sqlx::query("INSERT INTO api_tokens (account_id, token, comment) \
                                VALUES ($1, $2, $3)")
            .bind(account_id)
            .bind(&api_token)
            .bind(comment)
            .execute(&self.pool).await
            .map_err(|err| {
                warn!("Failed to add API token for '{account_id}': {err}");
                NetResultCode::NetInternalError
            })?;
        Ok(api_token)
    }

    async fn set_all_players_offline(&self) -> NetResult<()> {
        let _ = sqlx::query("UPDATE nodes SET int32_1 = 0 WHERE node_type = $1")
            .bind(NodeType::PlayerInfo as i32)
            .execute(&self.pool).await
            .map_err(|err| {
                warn!("Failed to set all players offline: {err}");
                NetResultCode::NetInternalError
            })?;
        Ok(())
    }

    async fn get_players(&self, account_id: &Uuid) -> NetResult<Vec<PlayerInfo>> {
        let players: Vec<PlayerInfo> =
            sqlx::query_as("SELECT idx AS player_id, istring64_1 AS player_name, \
                                   string64_1 AS avatar_shape, int32_2 AS explorer \
                                FROM nodes WHERE node_type = $1 AND uuid_1 = $2")
                .bind(NodeType::Player as i32)
                .bind(account_id)
                .fetch_all(&self.pool).await
                .map_err(|err| {
                    warn!("Failed to fetch players for account: {err}");
                    NetResultCode::NetInternalError
                })?;
        Ok(players)
    }

    async fn count_players(&self, account_id: &Uuid) -> NetResult<u64> {
        let count: u64 = sqlx::query("SELECT COUNT(*) FROM nodes WHERE node_type = $1 AND uuid_1 = $2")
            .bind(NodeType::Player as i32)
            .bind(account_id)
            .fetch_one(&self.pool).await
            .map_err(|err| {
                warn!("Failed to count players for account: {err}");
                NetResultCode::NetInternalError
            })?
            .get(0);
        Ok(count)
    }

    async fn player_exists(&self, player_name: &str) -> NetResult<bool> {
        let player_id: Option<i32> =
            sqlx::query("SELECT idx FROM nodes \
                             WHERE node_type = $1 AND istring64_1 = $2
                             LIMIT 1")
                .bind(NodeType::Player as i32)
                .bind(player_name)
                .fetch_optional(&self.pool).await
                .map_err(|err| {
                    warn!("Failed to query for player: {err}");
                    NetResultCode::NetInternalError
                })?
                .map(|row| row.get(0));
        Ok(player_id.is_some())
    }

    async fn add_game_server(&self, server: GameServer) -> NetResult<()> {
        sqlx::query("INSERT INTO servers (instance_uuid, age_filename, display_name,
                                          age_idx, sdl_idx, temporary) \
                        VALUES ($1, $2, $3, $4, $5, $6)")
            .bind(server.instance_id)
            .bind(server.age_filename)
            .bind(server.display_name)
            .bind(server.age_id)
            .bind(server.sdl_id)
            .bind(server.temporary)
            .execute(&self.pool).await
            .map_err(|err| {
                warn!("Failed to add game server: {err}");
                NetResultCode::NetInternalError
            })?;
        Ok(())
    }

    async fn create_node(&self, mut node: VaultNode) -> NetResult<u32> {
        // WARNING: Not Y2038/Y2106 compatible (but then, neither is Plasma)
        #[allow(clippy::cast_possible_truncation)]
        let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .expect("Failed to get system time")
                    .as_secs() as u32;
        node.set_create_time(now);
        node.set_modify_time(now);

        let mut query = QueryBuilder::new("INSERT INTO nodes (");
        let mut field_names = query.separated(", ");
        iter_node!(node, |name, _| { field_names.push(name); });

        query.push(") VALUES (");
        let mut values = query.separated(", ");
        iter_node!(node, |_, value| { values.push_bind(value); });

        query.push(") RETURNING idx");
        Ok(query.build()
            .fetch_one(&self.pool).await
            .map_err(|err| {
                warn!("Failed to create vault node: {err}");
                NetResultCode::NetInternalError
            })?
            .get(0))
    }

    async fn fetch_node(&self, node_id: u32) -> NetResult<Arc<VaultNode>> {
        let node: Option<VaultNode> =
            sqlx::query_as("SELECT * FROM nodes WHERE idx = $1")
                .bind(node_id)
                .fetch_optional(&self.pool).await
                .map_err(|err| {
                    warn!("Failed to fetch node: {err}");
                    NetResultCode::NetInternalError
                })?;
        if let Some(node) = node {
            Ok(Arc::new(node))
        } else {
            Err(NetResultCode::NetVaultNodeNotFound)
        }
    }

    async fn update_node(&self, mut node: VaultNode) -> NetResult<Vec<u32>> {
        // WARNING: Not Y2038/Y2106 compatible (but then, neither is Plasma)
        #[allow(clippy::cast_possible_truncation)]
        let now = SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .expect("Failed to get system time")
                    .as_secs() as u32;
        node.clear_create_time();
        node.set_modify_time(now);

        let mut query = QueryBuilder::new("UPDATE nodes SET ");
        let mut fields = query.separated(", ");
        iter_node!(node, |name, value| {
            fields.push(name);
            fields.push_unseparated(" = ");
            fields.push_bind_unseparated(value);
        });

        query.push(" WHERE idx = ");
        query.push_bind(node.node_id());
        query.push(" RETURNING idx");
        let updated = query.build()
            .fetch_all(&self.pool).await
            .map_err(|err| {
                warn!("Failed to update vault node: {err}");
                NetResultCode::NetInternalError
            })?.iter().map(|row| row.get(0)).collect();

        Ok(updated)
    }

    async fn find_nodes(&self, template: VaultNode) -> NetResult<Vec<u32>> {
        let mut query = QueryBuilder::new("SELECT idx FROM nodes WHERE ");
        let mut fields = query.separated(" AND ");

        iter_node!(template, |name, value| {
            fields.push(name);
            fields.push_unseparated(" = ");
            fields.push_bind_unseparated(value);
        });

        Ok(query.build()
            .fetch_all(&self.pool).await
            .map_err(|err| {
                warn!("Failed to update vault node: {err}");
                NetResultCode::NetInternalError
            })?
            .into_iter().map(|row| row.get(0))
            .collect())
    }

    async fn get_system_node(&self) -> NetResult<u32> {
        sqlx::query("SELECT idx FROM nodes WHERE node_type = $1 LIMIT 1")
            .bind(NodeType::System as i32)
            .fetch_optional(&self.pool).await
            .map_err(|err| {
                warn!("Failed to fetch System node: {err}");
                NetResultCode::NetInternalError
            })?
            .map_or(Err(NetResultCode::NetVaultNodeNotFound), |row| Ok(row.get(0)))
    }

    async fn get_all_players_node(&self) -> NetResult<u32> {
        sqlx::query("SELECT idx FROM nodes WHERE node_type = $1 AND int32_1 = $2 LIMIT 1")
            .bind(NodeType::PlayerInfoList as i32)
            .bind(StandardNode::AllPlayersFolder as i32)
            .fetch_optional(&self.pool).await
            .map_err(|err| {
                warn!("Failed to fetch AllPlayersFolder node: {err}");
                NetResultCode::NetInternalError
            })?
            .map_or(Err(NetResultCode::NetVaultNodeNotFound), |row| Ok(row.get(0)))
    }

    async fn get_player_info_node(&self, player_id: u32) -> NetResult<Arc<VaultNode>> {
        let node: Option<VaultNode> =
            sqlx::query_as("SELECT * FROM nodes WHERE \
                                idx IN (SELECT child_idx FROM node_refs WHERE parent_idx = $2) AND \
                                node_type = $1 AND uint32_1 = $2 LIMIT 1")
                .bind(NodeType::PlayerInfo as i32)
                .bind(player_id)
                .fetch_optional(&self.pool).await
                .map_err(|err| {
                    warn!("Failed to fetch PlayerInfo node for player {player_id}: {err}");
                    NetResultCode::NetInternalError
                })?;
        if let Some(node) = node {
            Ok(Arc::new(node))
        } else {
            Err(NetResultCode::NetVaultNodeNotFound)
        }
    }

    async fn ref_node(&self, parent: u32, child: u32, owner: u32) -> NetResult<()> {
        let _ = sqlx::query("INSERT INTO node_refs (parent_idx, child_idx, owner_idx) \
                                    VALUES ($1, $2, $3)")
            .bind(parent).bind(child).bind(owner)
            .execute(&self.pool).await
            .map_err(|err| {
                warn!("Failed to add node ref: {err}");
                NetResultCode::NetInternalError
            })?;
        Ok(())
    }

    async fn fetch_refs(&self, parent: u32, recursive: bool) -> NetResult<Vec<NodeRef>> {
        let mut refs = Vec::<NodeRef>::new();
        for node_ref in sqlx::query_as("SELECT parent_idx, child_idx, owner_idx FROM node_refs WHERE parent_idx = $1")
                            .bind(parent)
                            .fetch_all(&self.pool).await
                            .map_err(|err| {
                                warn!("Failed to fetch node refs: {err}");
                                NetResultCode::NetInternalError
                            })?
        {
            refs.push(node_ref);
            if recursive {
                let children = self.fetch_refs(node_ref.child(), true).await?;
                refs.extend_from_slice(&children);
            }
        }
        Ok(refs)
    }
}

impl FromRow<'_, SqliteRow> for AccountInfo {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self {
            account_name: row.try_get("account_name")?,
            pass_hash: ShaDigest::from_hex(row.try_get("pass_hash")?)
                .map_err(|err| sqlx::Error::Decode(err.into()))?,
            account_id: row.try_get("account_id")?,
            account_flags: row.try_get("account_flags")?,
            billing_type: row.try_get("billing_type")?,
        })
    }
}

impl FromRow<'_, SqliteRow> for PlayerInfo {
    // TODO: Get rid of PlayerInfo and use VaultPlayerNode directly instead.
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(Self {
            player_id: row.try_get("player_id")?,
            player_name: row.try_get("player_name")?,
            avatar_shape: row.try_get("avatar_shape")?,
            explorer: row.try_get("explorer")?,
        })
    }
}

impl FromRow<'_, SqliteRow> for VaultNode {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        let mut node = VaultNode::default();

        node.set_node_id(row.try_get("idx")?);
        node.set_create_time(row.try_get("create_time")?);
        node.set_modify_time(row.try_get("modify_time")?);
        let create_age_name: Option<String> = row.try_get("create_age_name")?;
        if let Some(create_age_name) = create_age_name {
            node.set_create_age_name(&create_age_name);
        }
        if let Some(create_age_uuid) = row.try_get("create_age_uuid")? {
            node.set_create_age_uuid(&create_age_uuid);
        }
        node.set_creator_uuid(&row.try_get("creator_uuid")?);
        node.set_creator_id(row.try_get("creator_idx")?);
        node.set_node_type(row.try_get("node_type")?);
        if let Some(int32_1) = row.try_get("int32_1")? {
            node.set_int32_1(int32_1);
        }
        if let Some(int32_2) = row.try_get("int32_2")? {
            node.set_int32_2(int32_2);
        }
        if let Some(int32_3) = row.try_get("int32_3")? {
            node.set_int32_3(int32_3);
        }
        if let Some(int32_4) = row.try_get("int32_4")? {
            node.set_int32_4(int32_4);
        }
        if let Some(uint32_1) = row.try_get("uint32_1")? {
            node.set_uint32_1(uint32_1);
        }
        if let Some(uint32_2) = row.try_get("uint32_2")? {
            node.set_uint32_2(uint32_2);
        }
        if let Some(uint32_3) = row.try_get("uint32_3")? {
            node.set_uint32_3(uint32_3);
        }
        if let Some(uint32_4) = row.try_get("uint32_4")? {
            node.set_uint32_4(uint32_4);
        }
        if let Some(uuid_1) = row.try_get("uuid_1")? {
            node.set_uuid_1(&uuid_1);
        }
        if let Some(uuid_2) = row.try_get("uuid_2")? {
            node.set_uuid_2(&uuid_2);
        }
        if let Some(uuid_3) = row.try_get("uuid_3")? {
            node.set_uuid_3(&uuid_3);
        }
        if let Some(uuid_4) = row.try_get("uuid_4")? {
            node.set_uuid_4(&uuid_4);
        }
        let string64_1: Option<String> = row.try_get("string64_1")?;
        if let Some(string64_1) = string64_1 {
            node.set_string64_1(&string64_1);
        }
        let string64_2: Option<String> = row.try_get("string64_2")?;
        if let Some(string64_2) = string64_2 {
            node.set_string64_2(&string64_2);
        }
        let string64_3: Option<String> = row.try_get("string64_3")?;
        if let Some(string64_3) = string64_3 {
            node.set_string64_3(&string64_3);
        }
        let string64_4: Option<String> = row.try_get("string64_4")?;
        if let Some(string64_4) = string64_4 {
            node.set_string64_4(&string64_4);
        }
        let string64_5: Option<String> = row.try_get("string64_5")?;
        if let Some(string64_5) = string64_5 {
            node.set_string64_5(&string64_5);
        }
        let string64_6: Option<String> = row.try_get("string64_6")?;
        if let Some(string64_6) = string64_6 {
            node.set_string64_6(&string64_6);
        }
        let istring64_1: Option<String> = row.try_get("istring64_1")?;
        if let Some(istring64_1) = istring64_1 {
            node.set_istring64_1(&istring64_1);
        }
        let istring64_2: Option<String> = row.try_get("istring64_2")?;
        if let Some(istring64_2) = istring64_2 {
            node.set_istring64_2(&istring64_2);
        }
        let text_1: Option<String> = row.try_get("text_1")?;
        if let Some(text_1) = text_1 {
            node.set_text_1(&text_1);
        }
        let text_2: Option<String> = row.try_get("text_2")?;
        if let Some(text_2) = text_2 {
            node.set_text_2(&text_2);
        }
        let blob_1: Option<Vec<u8>> = row.try_get("blob_1")?;
        if let Some(blob_1) = blob_1 {
            node.set_blob_1(&blob_1);
        }
        let blob_2: Option<Vec<u8>> = row.try_get("blob_2")?;
        if let Some(blob_2) = blob_2 {
            node.set_blob_2(&blob_2);
        }
        Ok(node)
    }
}

impl FromRow<'_, SqliteRow> for NodeRef {
    fn from_row(row: &SqliteRow) -> sqlx::Result<Self> {
        Ok(NodeRef::new(row.try_get("parent_idx")?,
                        row.try_get("child_idx")?,
                        row.try_get("owner_idx")?))
    }
}
