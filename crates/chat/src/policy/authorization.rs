use std::{collections::HashSet, fmt};

use anyhow::Result;
use app_core::api::error::ApiError;
use app_core::auth::model::PrincipalContext;
use sqlx::PgPool;

use crate::knowledge::model::KnowledgeCatalog;

pub async fn project_admin_principal(
    principal: &mut PrincipalContext,
    catalog: &KnowledgeCatalog,
    fineract_pool: &PgPool,
) -> Result<()> {
    let capability_ids = catalog
        .capabilities
        .iter()
        .filter(|capability| capability.status == "approved_mvp")
        .map(|capability| capability.id.clone())
        .collect();
    let rows: Vec<(i64,)> = sqlx::query_as("SELECT id FROM m_office ORDER BY id")
        .fetch_all(fineract_pool)
        .await?;
    let tenant_office_ids = rows.into_iter().map(|(id,)| id).collect();
    let office_ids = intersect_office_scope(&principal.office_ids, tenant_office_ids);
    finalize_admin_projection(principal, capability_ids, office_ids)?;

    Ok(())
}

/// Intersect the tenant's full office set with the principal's existing scope
/// (carried from the API key). An empty principal scope means "unrestricted",
/// so the full tenant set is returned. A non-empty scope survives as the
/// intersection, dropping any office the key names that the tenant does not
/// have. An empty intersection is rejected downstream as `MissingOfficeScope`.
fn intersect_office_scope(principal_office_ids: &[i64], tenant_office_ids: Vec<i64>) -> Vec<i64> {
    if principal_office_ids.is_empty() {
        return tenant_office_ids;
    }

    let allowed: HashSet<i64> = principal_office_ids.iter().copied().collect();
    tenant_office_ids
        .into_iter()
        .filter(|office_id| allowed.contains(office_id))
        .collect()
}

fn finalize_admin_projection(
    principal: &mut PrincipalContext,
    capability_ids: Vec<String>,
    office_ids: Vec<i64>,
) -> Result<(), AuthorizationError> {
    if office_ids.is_empty() {
        return Err(AuthorizationError::MissingOfficeScope);
    }
    principal.capability_ids = capability_ids;
    principal.office_ids = office_ids;
    principal.can_view_pii = true;

    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub enum AuthorizationError {
    CapabilityNotAllowed(String),

    MissingOfficeScope,

    OfficeNotAllowed(i64),

    PiiNotAllowed,
}

impl fmt::Display for AuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapabilityNotAllowed(capability) => {
                write!(
                    formatter,
                    "principal is not allowed to run capability `{capability}`"
                )
            }
            Self::MissingOfficeScope => write!(formatter, "principal has no office scope"),
            Self::OfficeNotAllowed(office_id) => {
                write!(
                    formatter,
                    "requested office `{office_id}` is outside principal scope"
                )
            }
            Self::PiiNotAllowed => {
                write!(formatter, "PII output is not allowed for this principal")
            }
        }
    }
}

impl std::error::Error for AuthorizationError {}

impl From<AuthorizationError> for ApiError {
    fn from(error: AuthorizationError) -> Self {
        Self::forbidden(error.to_string())
    }
}

pub fn ensure_capability_allowed(
    principal: &PrincipalContext,
    capability: &str,
) -> Result<(), AuthorizationError> {
    if principal
        .capability_ids
        .iter()
        .any(|allowed| allowed == capability)
    {
        Ok(())
    } else {
        Err(AuthorizationError::CapabilityNotAllowed(
            capability.to_string(),
        ))
    }
}

pub fn effective_office_scope(
    principal: &PrincipalContext,
    requested_office_ids: Option<&[i64]>,
) -> Result<Vec<i64>, AuthorizationError> {
    if principal.office_ids.is_empty() {
        return Err(AuthorizationError::MissingOfficeScope);
    }

    let allowed: HashSet<i64> = principal.office_ids.iter().copied().collect();

    let office_ids = match requested_office_ids {
        Some(requested) => {
            for office_id in requested {
                if !allowed.contains(office_id) {
                    return Err(AuthorizationError::OfficeNotAllowed(*office_id));
                }
            }

            requested.to_vec()
        }
        None => principal.office_ids.clone(),
    };

    Ok(office_ids)
}

pub fn ensure_pii_allowed(
    principal: &PrincipalContext,
    output_requires_pii: bool,
) -> Result<(), AuthorizationError> {
    if !output_requires_pii || principal.can_view_pii {
        Ok(())
    } else {
        Err(AuthorizationError::PiiNotAllowed)
    }
}

pub fn pii_output_allowed(principal: &PrincipalContext, output_requires_pii: bool) -> bool {
    ensure_pii_allowed(principal, output_requires_pii).is_ok()
}

// TODO(reporting): call these guards from the reporting execution plan before any
// Fineract SQL is executed, then select only fields allowed by the capability and
// PII policy. Office filtering must happen inside approved SQL, not after fetching.

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn client() -> PrincipalContext {
        PrincipalContext {
            user_id: Uuid::nil(),
            role: "admin".to_string(),
            office_ids: vec![1, 2],
            capability_ids: vec!["savings_deposit_total".to_string()],
            can_view_pii: false,
            legacy_api_key_id: None,
        }
    }

    #[test]
    fn allows_configured_capability() {
        let client = client();

        assert!(ensure_capability_allowed(&client, "savings_deposit_total").is_ok());
    }

    #[test]
    fn rejects_unconfigured_capability() {
        let client = client();

        assert_eq!(
            ensure_capability_allowed(&client, "savings_deposit_top_n"),
            Err(AuthorizationError::CapabilityNotAllowed(
                "savings_deposit_top_n".to_string()
            ))
        );
    }

    #[test]
    fn uses_all_allowed_offices_when_request_omits_scope() {
        let client = client();

        assert_eq!(effective_office_scope(&client, None), Ok(vec![1, 2]));
    }

    #[test]
    fn allows_requested_subset_of_offices() {
        let client = client();

        assert_eq!(effective_office_scope(&client, Some(&[2])), Ok(vec![2]));
    }

    #[test]
    fn rejects_office_outside_scope() {
        let client = client();

        assert_eq!(
            effective_office_scope(&client, Some(&[3])),
            Err(AuthorizationError::OfficeNotAllowed(3))
        );
    }

    #[test]
    fn rejects_empty_office_scope() {
        let mut client = client();
        client.office_ids.clear();

        assert_eq!(
            effective_office_scope(&client, None),
            Err(AuthorizationError::MissingOfficeScope)
        );
    }

    #[test]
    fn intersect_office_scope_empty_principal_gets_full_tenant() {
        assert_eq!(
            intersect_office_scope(&[], vec![1, 2, 3, 40]),
            vec![1, 2, 3, 40]
        );
    }

    #[test]
    fn intersect_office_scope_restricts_to_key_scope() {
        assert_eq!(
            intersect_office_scope(&[2, 40], vec![1, 2, 3, 40]),
            vec![2, 40]
        );
    }

    #[test]
    fn intersect_office_scope_drops_offices_absent_from_tenant() {
        assert_eq!(intersect_office_scope(&[2, 99], vec![1, 2, 3]), vec![2]);
    }

    #[test]
    fn intersect_office_scope_disjoint_scope_is_empty() {
        assert!(intersect_office_scope(&[99], vec![1, 2, 3]).is_empty());
    }

    #[test]
    fn admin_projection_rejects_empty_offices_without_granting_access() {
        let mut principal = PrincipalContext {
            capability_ids: Vec::new(),
            office_ids: Vec::new(),
            ..client()
        };

        assert_eq!(
            finalize_admin_projection(&mut principal, vec!["approved".into()], Vec::new()),
            Err(AuthorizationError::MissingOfficeScope)
        );
        assert!(principal.capability_ids.is_empty());
        assert!(principal.office_ids.is_empty());
        assert!(!principal.can_view_pii);
    }

    #[test]
    fn rejects_pii_when_api_key_cannot_view_pii() {
        let client = client();

        assert_eq!(
            ensure_pii_allowed(&client, true),
            Err(AuthorizationError::PiiNotAllowed)
        );
    }

    #[test]
    fn allows_pii_when_api_key_can_view_pii() {
        let mut client = client();
        client.can_view_pii = true;

        assert!(ensure_pii_allowed(&client, true).is_ok());
    }
}
