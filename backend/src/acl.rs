//! Access-control model for documents.
//!
//! Effective access to a folder or file is the maximum of:
//! 1. role baseline — tenant owners/admins get FULL everywhere;
//! 2. ownership — the owner of any folder in the ancestor chain (or of the
//!    file itself) gets FULL;
//! 3. root kind — `common` roots grant EDIT to members and READ to guests;
//!    `project` roots grant access by project membership role;
//! 4. explicit shares on the resource or any ancestor folder, addressed to
//!    the user, one of the user's groups, or everyone in the tenant.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    auth::CurrentUser,
    error::{ApiError, ApiResult},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[repr(i16)]
pub enum Access {
    None = 0,
    Read = 1,
    Comment = 2,
    Edit = 3,
    Full = 4,
}

impl Access {
    pub fn from_i16(v: i16) -> Self {
        match v {
            1 => Access::Read,
            2 => Access::Comment,
            3 => Access::Edit,
            v if v >= 4 => Access::Full,
            _ => Access::None,
        }
    }

    pub fn require(self, needed: Access) -> ApiResult<()> {
        if self >= needed {
            Ok(())
        } else if self == Access::None {
            // Do not reveal existence of resources the user cannot see.
            Err(ApiError::NotFound)
        } else {
            Err(ApiError::Forbidden)
        }
    }
}

#[derive(sqlx::FromRow)]
struct ChainRow {
    id: Uuid,
    kind: String,
    owner_id: Uuid,
}

/// Folder ancestry from the given folder up to its root (inclusive).
async fn folder_chain(db: &PgPool, tenant_id: Uuid, folder_id: Uuid) -> ApiResult<Vec<ChainRow>> {
    let rows = sqlx::query_as::<_, ChainRow>(
        "WITH RECURSIVE chain AS (
             SELECT id, parent_id, kind, owner_id, 0 AS depth
               FROM folders WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL
             UNION ALL
             SELECT f.id, f.parent_id, f.kind, f.owner_id, c.depth + 1
               FROM folders f JOIN chain c ON f.id = c.parent_id
              WHERE c.depth < 64
         )
         SELECT id, kind, owner_id FROM chain ORDER BY depth",
    )
    .bind(folder_id)
    .bind(tenant_id)
    .fetch_all(db)
    .await?;
    if rows.is_empty() {
        return Err(ApiError::NotFound);
    }
    Ok(rows)
}

async fn compute(
    db: &PgPool,
    user: &CurrentUser,
    chain: &[ChainRow],
    file: Option<(Uuid, Uuid)>, // (file_id, file_owner_id)
) -> ApiResult<Access> {
    if user.is_admin() {
        return Ok(Access::Full);
    }
    if chain.iter().any(|f| f.owner_id == user.user_id) || file.is_some_and(|(_, o)| o == user.user_id) {
        return Ok(Access::Full);
    }
    let root = chain.last().expect("chain is non-empty");
    let mut access = match root.kind.as_str() {
        "common" if user.is_guest() => Access::Read,
        "common" => Access::Edit,
        "project" => {
            let role: Option<String> = sqlx::query_scalar(
                "SELECT pm.role FROM projects p JOIN project_members pm ON pm.project_id = p.id
                  WHERE p.folder_id = $1 AND pm.user_id = $2",
            )
            .bind(root.id)
            .bind(user.user_id)
            .fetch_optional(db)
            .await?;
            match role.as_deref() {
                Some("manager") => Access::Full,
                Some("member") => Access::Edit,
                Some("viewer") => Access::Read,
                _ => Access::None,
            }
        }
        // Company document folders follow CRM visibility of the company.
        "crm" => {
            if !user.crm_enabled() {
                Access::None
            } else {
                let row: Option<(Option<Uuid>, Option<Uuid>, bool)> = sqlx::query_as(
                    "SELECT c.responsible_id, c.created_by,
                            crm_company_visible(c.visibility, c.responsible_id, c.group_id, $2, $3)
                       FROM crm_companies c WHERE c.folder_id = $1",
                )
                .bind(root.id)
                .bind(user.user_id)
                .bind(user.crm_manager())
                .fetch_optional(db)
                .await?;
                match row {
                    Some((_, _, false)) | None => Access::None,
                    Some((resp, created, true)) => {
                        if user.crm_manager() || resp == Some(user.user_id) || created == Some(user.user_id) {
                            Access::Full
                        } else {
                            Access::Edit
                        }
                    }
                }
            }
        }
        _ => Access::None,
    };

    let folder_ids: Vec<Uuid> = chain.iter().map(|f| f.id).collect();
    let shared: Option<i16> = sqlx::query_scalar(
        "SELECT max(s.access) FROM shares s
          WHERE s.tenant_id = $1
            AND ((s.resource_type = 'folder' AND s.resource_id = ANY($2))
              OR (s.resource_type = 'file' AND s.resource_id = $3))
            AND (s.user_id = $4
              OR s.group_id IN (SELECT group_id FROM group_members WHERE user_id = $4)
              OR (s.user_id IS NULL AND s.group_id IS NULL))",
    )
    .bind(user.tenant_id)
    .bind(&folder_ids)
    .bind(file.map(|f| f.0))
    .bind(user.user_id)
    .fetch_one(db)
    .await?;
    if let Some(s) = shared {
        access = access.max(Access::from_i16(s));
    }
    // Guests can never exceed comment rights.
    if user.is_guest() {
        access = access.min(Access::Comment);
    }
    Ok(access)
}

pub async fn folder_access(db: &PgPool, user: &CurrentUser, folder_id: Uuid) -> ApiResult<Access> {
    let chain = folder_chain(db, user.tenant_id, folder_id).await?;
    compute(db, user, &chain, None).await
}

/// Returns the effective access and the file's folder id.
pub async fn file_access(db: &PgPool, user: &CurrentUser, file_id: Uuid) -> ApiResult<(Access, Uuid)> {
    let (folder_id, owner_id): (Uuid, Uuid) = sqlx::query_as(
        "SELECT folder_id, owner_id FROM files WHERE id = $1 AND tenant_id = $2 AND deleted_at IS NULL",
    )
    .bind(file_id)
    .bind(user.tenant_id)
    .fetch_optional(db)
    .await?
    .ok_or(ApiError::NotFound)?;
    let chain = folder_chain(db, user.tenant_id, folder_id).await?;
    Ok((compute(db, user, &chain, Some((file_id, owner_id))).await?, folder_id))
}
