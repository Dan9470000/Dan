// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashMap, sync::Arc};

use async_graphql::dataloader::Loader;
use diesel::sql_types::{Array, Bytea};
use sui_indexer_alt_schema::objects::StoredObjVersionKey;
use sui_types::base_types::ObjectID;

use crate::data::error::Error;

use super::pg_reader::PgReader;

/// Key for fetching the latest version of an object, not accounting for deletions or wraps. If the
/// object has been deleted or wrapped, the version before the delete/wrap is returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LatestObjectVersionKey(pub ObjectID);

#[async_trait::async_trait]
impl Loader<LatestObjectVersionKey> for PgReader {
    type Value = StoredObjVersionKey;
    type Error = Arc<Error>;

    async fn load(
        &self,
        keys: &[LatestObjectVersionKey],
    ) -> Result<HashMap<LatestObjectVersionKey, StoredObjVersionKey>, Self::Error> {
        if keys.is_empty() {
            return Ok(HashMap::new());
        }

        let mut conn = self.connect().await.map_err(Arc::new)?;

        let ids: Vec<_> = keys.iter().map(|k| k.0.into_bytes()).collect();
        let query = diesel::sql_query(
            r#"
                SELECT
                    k.object_id,
                    v.object_version
                FROM (
                    SELECT UNNEST($1) object_id
                ) k
                CROSS JOIN LATERAL (
                    SELECT
                        object_version
                    FROM
                        obj_versions
                    WHERE
                        obj_versions.object_id = k.object_id
                    ORDER BY
                        object_version DESC
                    LIMIT
                        1
                ) v
            "#,
        )
        .bind::<Array<Bytea>, _>(ids);

        let obj_versions: Vec<StoredObjVersionKey> = conn.results(query).await.map_err(Arc::new)?;
        let id_to_stored: HashMap<_, _> = obj_versions
            .into_iter()
            .map(|stored| (stored.object_id.clone(), stored))
            .collect();

        Ok(keys
            .iter()
            .filter_map(|key| {
                let slice: &[u8] = key.0.as_ref();
                Some((*key, id_to_stored.get(slice).cloned()?))
            })
            .collect())
    }
}
